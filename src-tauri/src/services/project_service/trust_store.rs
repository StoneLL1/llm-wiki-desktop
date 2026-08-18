use std::fs::{self, File, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::errors::BackendError;
use crate::models::project::ProjectTrustKind;
use crate::services::{project_identity, FileStore};

const PROJECT_TRUST_FILE: &str = "project-trust.json";
const PROJECT_TRUST_LOCK_FILE: &str = "project-trust.lock";
const PROJECT_MUTATION_LOCK_FILE: &str = "project-mutations.lock";
const PROJECT_TRUST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerifiedProjectTrust {
    pub canonical_root: PathBuf,
    pub canonical_identity_key: String,
    pub identity_revision: String,
    pub trust_kind: ProjectTrustKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredProjectTrust {
    canonical_path: String,
    canonical_identity_key: String,
    identity_revision: String,
    trust_kind: ProjectTrustKind,
    granted_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectTrustFile {
    schema_version: u32,
    entries: Vec<StoredProjectTrust>,
}

impl Default for ProjectTrustFile {
    fn default() -> Self {
        Self {
            schema_version: PROJECT_TRUST_SCHEMA_VERSION,
            entries: Vec::new(),
        }
    }
}

pub(crate) struct ProjectTrustStore {
    root: PathBuf,
    path: PathBuf,
    lock: RwLock<()>,
}

impl ProjectTrustStore {
    pub(crate) fn new(config_dir: &Path) -> Self {
        Self {
            root: config_dir.parent().unwrap_or(config_dir).to_path_buf(),
            path: config_dir.join(PROJECT_TRUST_FILE),
            lock: RwLock::new(()),
        }
    }

    pub(crate) fn grant(
        &self,
        root: &Path,
        trust_kind: ProjectTrustKind,
        expected_identity_key: &str,
        expected_identity_revision: &str,
    ) -> Result<VerifiedProjectTrust, BackendError> {
        let _guard = self.lock.write().map_err(|_| trust_store_locked())?;
        let _file_lock =
            acquire_project_trust_lock(&self.path.with_file_name(PROJECT_TRUST_LOCK_FILE))?;
        let identity = project_identity(root).map_err(project_identity_error)?;
        if identity.canonical_identity_key != expected_identity_key
            || identity.identity_revision != expected_identity_revision
        {
            return Err(BackendError::new(
                "PROJECT_TRUST_IDENTITY_CHANGED",
                "The project identity changed while trust was being granted.",
                true,
                true,
            ));
        }
        let mut file = self.read_file_strict()?;
        let canonical_path = identity.canonical_root.to_string_lossy().into_owned();
        let canonical_key = normalize_canonical_path(&canonical_path);
        file.entries
            .retain(|entry| normalize_canonical_path(&entry.canonical_path) != canonical_key);
        file.entries.push(StoredProjectTrust {
            canonical_path,
            canonical_identity_key: identity.canonical_identity_key.clone(),
            identity_revision: identity.identity_revision.clone(),
            trust_kind,
            granted_at: Utc::now().to_rfc3339(),
        });
        self.write_file(&file)?;
        Ok(VerifiedProjectTrust {
            canonical_root: identity.canonical_root,
            canonical_identity_key: identity.canonical_identity_key,
            identity_revision: identity.identity_revision,
            trust_kind,
        })
    }

    pub(crate) fn restore(
        &self,
        root: &Path,
    ) -> Result<Option<VerifiedProjectTrust>, BackendError> {
        let _guard = self.lock.write().map_err(|_| trust_store_locked())?;
        let _file_lock =
            match acquire_project_trust_lock(&self.path.with_file_name(PROJECT_TRUST_LOCK_FILE)) {
                Ok(lock) => lock,
                Err(error) => {
                    eprintln!(
                        "Failed to lock project trust settings {}: {}",
                        self.path.display(),
                        error.message
                    );
                    return Ok(None);
                }
            };
        let identity = project_identity(root).map_err(project_identity_error)?;
        let mut file = match self.read_file_strict() {
            Ok(file) => file,
            Err(error) => {
                eprintln!(
                    "Failed to load project trust settings {}: {}",
                    self.path.display(),
                    error.message
                );
                return Ok(None);
            }
        };
        let canonical_path = identity.canonical_root.to_string_lossy().into_owned();
        let canonical_key = normalize_canonical_path(&canonical_path);
        let mut restored = None;
        let mut changed = false;
        file.entries.retain(|entry| {
            if normalize_canonical_path(&entry.canonical_path) != canonical_key {
                return true;
            }
            let matches = entry.canonical_identity_key == identity.canonical_identity_key
                && entry.identity_revision == identity.identity_revision
                && restored.is_none();
            if matches {
                restored = Some(VerifiedProjectTrust {
                    canonical_root: identity.canonical_root.clone(),
                    canonical_identity_key: identity.canonical_identity_key.clone(),
                    identity_revision: identity.identity_revision.clone(),
                    trust_kind: entry.trust_kind,
                });
                true
            } else {
                changed = true;
                false
            }
        });
        if changed {
            if let Err(error) = self.write_file(&file) {
                eprintln!(
                    "Failed to prune stale project trust settings {}: {}",
                    self.path.display(),
                    error.message
                );
            }
        }
        Ok(restored)
    }

    pub(crate) fn revoke(&self, root: &Path) -> Result<(), BackendError> {
        let _guard = self.lock.write().map_err(|_| trust_store_locked())?;
        let _file_lock =
            acquire_project_trust_lock(&self.path.with_file_name(PROJECT_TRUST_LOCK_FILE))?;
        let canonical_root = root.canonicalize().map_err(|error| {
            BackendError::new(
                "PROJECT_TRUST_PATH_INVALID",
                "The trusted project path could not be resolved.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "error": error.to_string() }))
        })?;
        let canonical_key = normalize_canonical_path(&canonical_root.to_string_lossy());
        let mut file = self.read_file_strict()?;
        let before = file.entries.len();
        file.entries
            .retain(|entry| normalize_canonical_path(&entry.canonical_path) != canonical_key);
        if before != file.entries.len() {
            self.write_file(&file)?;
        }
        Ok(())
    }

    /// Serializes short, high-risk project mutations across LLM Wiki desktop
    /// processes that share this application's configuration directory. The
    /// project writer still validates every project path immediately before
    /// use; this guard closes races between cooperating app instances rather
    /// than claiming to control arbitrary external filesystem actors.
    pub(crate) fn acquire_project_mutation_lock(
        &self,
    ) -> Result<ProjectTrustFileLock, BackendError> {
        acquire_project_trust_lock(&self.path.with_file_name(PROJECT_MUTATION_LOCK_FILE)).map_err(
            |error| {
                BackendError::new(
                    "PROJECT_MUTATION_LOCKED",
                    "Another LLM Wiki process is applying a project change. Try again shortly.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({
                    "error": error.message,
                    "details": error.details,
                }))
            },
        )
    }

    fn read_file_strict(&self) -> Result<ProjectTrustFile, BackendError> {
        let contents = match fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Ok(ProjectTrustFile::default())
            }
            Err(error) => {
                return Err(trust_store_file_error(
                    "PROJECT_TRUST_STORE_READ_FAILED",
                    "Project trust settings could not be read.",
                    &self.path,
                    error.to_string(),
                ));
            }
        };
        let parsed = match serde_json::from_str::<ProjectTrustFile>(&contents) {
            Ok(parsed) => parsed,
            Err(error) => {
                return Err(trust_store_file_error(
                    "PROJECT_TRUST_STORE_CORRUPT",
                    "Project trust settings are corrupt.",
                    &self.path,
                    error.to_string(),
                ));
            }
        };
        if parsed.schema_version != PROJECT_TRUST_SCHEMA_VERSION {
            return Err(trust_store_file_error(
                "PROJECT_TRUST_STORE_SCHEMA_UNSUPPORTED",
                "Project trust settings use an unsupported schema version.",
                &self.path,
                parsed.schema_version.to_string(),
            ));
        }
        Ok(parsed)
    }

    fn write_file(&self, file: &ProjectTrustFile) -> Result<(), BackendError> {
        FileStore.write_json_atomic_absolute(&self.root, &self.path, file)
    }
}

pub(crate) struct ProjectTrustFileLock {
    file: File,
}

#[cfg(unix)]
impl Drop for ProjectTrustFileLock {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        // SAFETY: the descriptor belongs to the live File held by this guard.
        let _ = unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
    }
}

#[cfg(windows)]
impl Drop for ProjectTrustFileLock {
    fn drop(&mut self) {
        // Closing the no-share file handle releases the cross-process lock.
        let _ = &self.file;
    }
}

#[cfg(unix)]
fn acquire_project_trust_lock(path: &Path) -> Result<ProjectTrustFileLock, BackendError> {
    use std::os::fd::AsRawFd;
    use std::time::Duration;

    let file = open_project_trust_lock_file(path)?;
    for _ in 0..200 {
        // SAFETY: flock receives a valid descriptor owned by `file`. LOCK_NB
        // keeps settings access bounded when another process is suspended.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(ProjectTrustFileLock { file });
        }
        let error = std::io::Error::last_os_error();
        if error
            .raw_os_error()
            .is_some_and(|code| code == libc::EWOULDBLOCK || code == libc::EAGAIN)
        {
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }
        return Err(trust_store_file_error(
            "PROJECT_TRUST_STORE_LOCKED",
            "Project trust settings are temporarily unavailable.",
            path,
            error.to_string(),
        ));
    }
    Err(trust_store_file_error(
        "PROJECT_TRUST_STORE_LOCKED",
        "Project trust settings are temporarily unavailable.",
        path,
        "Timed out waiting for the project trust settings lock".into(),
    ))
}

#[cfg(windows)]
fn acquire_project_trust_lock(path: &Path) -> Result<ProjectTrustFileLock, BackendError> {
    use std::os::windows::fs::OpenOptionsExt;
    use std::time::Duration;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            trust_store_file_error(
                "PROJECT_TRUST_STORE_LOCKED",
                "Project trust settings are temporarily unavailable.",
                path,
                error.to_string(),
            )
        })?;
    }
    for _ in 0..200 {
        match OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .share_mode(0)
            .open(path)
        {
            Ok(file) => return Ok(ProjectTrustFileLock { file }),
            Err(error) if matches!(error.raw_os_error(), Some(32 | 33)) => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                return Err(trust_store_file_error(
                    "PROJECT_TRUST_STORE_LOCKED",
                    "Project trust settings are temporarily unavailable.",
                    path,
                    error.to_string(),
                ));
            }
        }
    }
    Err(trust_store_file_error(
        "PROJECT_TRUST_STORE_LOCKED",
        "Project trust settings are temporarily unavailable.",
        path,
        "Timed out waiting for the project trust settings lock".into(),
    ))
}

#[cfg(unix)]
fn open_project_trust_lock_file(path: &Path) -> Result<File, BackendError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            trust_store_file_error(
                "PROJECT_TRUST_STORE_LOCKED",
                "Project trust settings are temporarily unavailable.",
                path,
                error.to_string(),
            )
        })?;
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)
        .map_err(|error| {
            trust_store_file_error(
                "PROJECT_TRUST_STORE_LOCKED",
                "Project trust settings are temporarily unavailable.",
                path,
                error.to_string(),
            )
        })
}

fn normalize_canonical_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    if cfg!(windows) {
        normalized.to_lowercase()
    } else {
        normalized
    }
}

fn trust_store_file_error(
    code: &'static str,
    message: &'static str,
    path: &Path,
    error: String,
) -> BackendError {
    BackendError::new(code, message, true, true).with_details(serde_json::json!({
        "path": path.to_string_lossy(),
        "error": error,
    }))
}

fn project_identity_error(message: String) -> BackendError {
    BackendError::new(
        "PROJECT_IDENTITY_FAILED",
        "Project identity could not be established.",
        true,
        true,
    )
    .with_details(serde_json::json!({ "error": message }))
}

fn trust_store_locked() -> BackendError {
    BackendError::new(
        "PROJECT_TRUST_STORE_LOCKED",
        "Project trust settings are temporarily unavailable.",
        true,
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant_current(store: &ProjectTrustStore, root: &Path) -> VerifiedProjectTrust {
        let identity = project_identity(root).unwrap();
        store
            .grant(
                root,
                ProjectTrustKind::Compatible,
                &identity.canonical_identity_key,
                &identity.identity_revision,
            )
            .unwrap()
    }

    fn compatible_root(label: &str) -> tempfile::TempDir {
        let root = tempfile::Builder::new()
            .prefix(&format!("llm-wiki-trust-{label}-"))
            .tempdir()
            .unwrap();
        fs::create_dir_all(root.path().join(".obsidian")).unwrap();
        fs::write(root.path().join("资料.md"), "# 资料").unwrap();
        root
    }

    #[test]
    fn missing_trust_file_is_untrusted() {
        let config = tempfile::tempdir().unwrap();
        let project = compatible_root("missing");
        let store = ProjectTrustStore::new(config.path());

        assert_eq!(store.restore(project.path()).unwrap(), None);
        assert!(!project.path().join(".app").exists());
    }

    #[test]
    fn compatible_trust_round_trips_from_global_settings_with_cjk_path() {
        let config = tempfile::tempdir().unwrap();
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("中文知识库");
        fs::create_dir_all(root.join(".obsidian")).unwrap();
        fs::write(root.join("资料.md"), "# 资料").unwrap();
        let store = ProjectTrustStore::new(config.path());

        let granted = grant_current(&store, &root);
        let restored = ProjectTrustStore::new(config.path())
            .restore(&root)
            .unwrap()
            .unwrap();

        assert_eq!(restored, granted);
        assert!(config.path().join(PROJECT_TRUST_FILE).is_file());
        assert!(!root.join(".app").exists());
    }

    #[test]
    fn corrupt_trust_file_is_treated_as_empty_without_panicking() {
        let config = tempfile::tempdir().unwrap();
        let project = compatible_root("corrupt");
        fs::write(config.path().join(PROJECT_TRUST_FILE), "{not-json").unwrap();
        let store = ProjectTrustStore::new(config.path());

        assert_eq!(store.restore(project.path()).unwrap(), None);
        assert_eq!(
            fs::read_to_string(config.path().join(PROJECT_TRUST_FILE)).unwrap(),
            "{not-json"
        );
    }

    #[test]
    fn matching_path_with_identity_mismatch_is_removed() {
        let config = tempfile::tempdir().unwrap();
        let project = compatible_root("mismatch");
        let store = ProjectTrustStore::new(config.path());
        grant_current(&store, project.path());
        let mut file: ProjectTrustFile = serde_json::from_str(
            &fs::read_to_string(config.path().join(PROJECT_TRUST_FILE)).unwrap(),
        )
        .unwrap();
        file.entries[0].identity_revision = "replaced-folder".into();
        fs::write(
            config.path().join(PROJECT_TRUST_FILE),
            serde_json::to_string_pretty(&file).unwrap(),
        )
        .unwrap();

        assert_eq!(store.restore(project.path()).unwrap(), None);
        let cleaned: ProjectTrustFile = serde_json::from_str(
            &fs::read_to_string(config.path().join(PROJECT_TRUST_FILE)).unwrap(),
        )
        .unwrap();
        assert!(cleaned.entries.is_empty());
    }

    #[test]
    fn grant_rejects_an_identity_that_changed_after_backend_validation() {
        let config = tempfile::tempdir().unwrap();
        let project = compatible_root("grant-drift");
        let store = ProjectTrustStore::new(config.path());

        let error = store
            .grant(
                project.path(),
                ProjectTrustKind::Compatible,
                "stale-key",
                "stale-revision",
            )
            .expect_err("stale confirmation identity must fail closed");

        assert_eq!(error.code, "PROJECT_TRUST_IDENTITY_CHANGED");
        assert!(!config.path().join(PROJECT_TRUST_FILE).exists());
    }

    #[test]
    fn corrupt_or_future_store_cannot_be_overwritten_by_a_grant() {
        let config = tempfile::tempdir().unwrap();
        let project = compatible_root("no-downgrade");
        let store_path = config.path().join(PROJECT_TRUST_FILE);
        let store = ProjectTrustStore::new(config.path());

        fs::write(&store_path, "{not-json").unwrap();
        let identity = project_identity(project.path()).unwrap();
        let corrupt = store
            .grant(
                project.path(),
                ProjectTrustKind::Compatible,
                &identity.canonical_identity_key,
                &identity.identity_revision,
            )
            .unwrap_err();
        assert_eq!(corrupt.code, "PROJECT_TRUST_STORE_CORRUPT");
        assert_eq!(fs::read_to_string(&store_path).unwrap(), "{not-json");

        fs::write(&store_path, r#"{"schemaVersion":99,"entries":[]}"#).unwrap();
        let future = store
            .grant(
                project.path(),
                ProjectTrustKind::Compatible,
                &identity.canonical_identity_key,
                &identity.identity_revision,
            )
            .unwrap_err();
        assert_eq!(future.code, "PROJECT_TRUST_STORE_SCHEMA_UNSUPPORTED");
        assert_eq!(
            fs::read_to_string(&store_path).unwrap(),
            r#"{"schemaVersion":99,"entries":[]}"#
        );
    }

    #[test]
    fn restoring_one_project_does_not_prune_an_offline_project() {
        let config = tempfile::tempdir().unwrap();
        let current = compatible_root("current");
        let offline = compatible_root("offline");
        let offline_path = offline.path().to_path_buf();
        let store = ProjectTrustStore::new(config.path());
        grant_current(&store, current.path());
        grant_current(&store, offline.path());
        drop(offline);

        assert!(store.restore(current.path()).unwrap().is_some());

        let file: ProjectTrustFile = serde_json::from_str(
            &fs::read_to_string(config.path().join(PROJECT_TRUST_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(file.entries.len(), 2);
        assert!(file.entries.iter().any(|entry| {
            PathBuf::from(&entry.canonical_path)
                .file_name()
                .is_some_and(|name| offline_path.file_name() == Some(name))
        }));
    }

    #[test]
    fn separate_store_instances_serialize_read_modify_write_updates() {
        use std::sync::{Arc, Barrier};

        let config = tempfile::tempdir().unwrap();
        let projects = (0..8)
            .map(|index| compatible_root(&format!("concurrent-{index}")))
            .collect::<Vec<_>>();
        let barrier = Arc::new(Barrier::new(projects.len()));
        let handles = projects
            .iter()
            .map(|project| {
                let root = project.path().to_path_buf();
                let config = config.path().to_path_buf();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    let store = ProjectTrustStore::new(&config);
                    let identity = project_identity(&root).unwrap();
                    barrier.wait();
                    store
                        .grant(
                            &root,
                            ProjectTrustKind::Compatible,
                            &identity.canonical_identity_key,
                            &identity.identity_revision,
                        )
                        .unwrap();
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle.join().unwrap();
        }

        let file: ProjectTrustFile = serde_json::from_str(
            &fs::read_to_string(config.path().join(PROJECT_TRUST_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(file.entries.len(), projects.len());
    }

    #[test]
    fn mutation_lock_serializes_separate_store_instances() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Barrier};
        use std::time::Duration;

        let config = tempfile::tempdir().unwrap();
        let worker_count = 6;
        let barrier = Arc::new(Barrier::new(worker_count));
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let handles = (0..worker_count)
            .map(|_| {
                let config = config.path().to_path_buf();
                let barrier = Arc::clone(&barrier);
                let active = Arc::clone(&active);
                let peak = Arc::clone(&peak);
                std::thread::spawn(move || {
                    let store = ProjectTrustStore::new(&config);
                    barrier.wait();
                    let _guard = store.acquire_project_mutation_lock().unwrap();
                    let concurrent = active.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(concurrent, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(15));
                    active.fetch_sub(1, Ordering::SeqCst);
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(peak.load(Ordering::SeqCst), 1);
        assert!(config.path().join(PROJECT_MUTATION_LOCK_FILE).is_file());
    }

    #[test]
    fn mutation_lock_is_exclusive_across_processes() {
        const CHILD_CONFIG_ENV: &str = "LLM_WIKI_MUTATION_LOCK_CHILD_CONFIG";
        const TEST_NAME: &str =
            "services::project_service::trust_store::tests::mutation_lock_is_exclusive_across_processes";

        if let Some(config) = std::env::var_os(CHILD_CONFIG_ENV) {
            let store = ProjectTrustStore::new(Path::new(&config));
            let error = match store.acquire_project_mutation_lock() {
                Ok(_) => panic!("child process acquired a lock held by its parent"),
                Err(error) => error,
            };
            assert_eq!(error.code, "PROJECT_MUTATION_LOCKED");
            return;
        }

        let config = tempfile::tempdir().unwrap();
        let store = ProjectTrustStore::new(config.path());
        let _guard = store.acquire_project_mutation_lock().unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST_NAME, "--nocapture"])
            .env(CHILD_CONFIG_ENV, config.path())
            .status()
            .unwrap();

        assert!(status.success());
    }
}
