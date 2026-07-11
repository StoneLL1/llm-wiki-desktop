use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use serde::{Deserialize, Serialize};

use crate::errors::{BackendError, IMPORT_V2_COMMIT_CONFLICT, IMPORT_V2_COMMIT_FAILED};

#[cfg(test)]
thread_local! {
    static BEFORE_CHECKED_DISPLACE_HOOK: std::cell::RefCell<Option<Box<dyn FnMut(&Path) -> bool>>> =
        std::cell::RefCell::new(None);
    static FAIL_NEXT_CANDIDATE_INSTALL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static BEFORE_NEW_INSTALL_HOOK: std::cell::RefCell<Option<Box<dyn FnMut(&Path) -> bool>>> = std::cell::RefCell::new(None);
    static FAIL_NEXT_CLEANUP: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);
    static FAIL_NEXT_IDENTITY_QUERY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn set_fail_next_candidate_install() {
    FAIL_NEXT_CANDIDATE_INSTALL.with(|flag| flag.set(true));
}

#[cfg(test)]
pub(super) fn set_before_new_install_hook(hook: impl FnMut(&Path) -> bool + 'static) {
    BEFORE_NEW_INSTALL_HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

#[cfg(test)]
fn set_fail_next_cleanup(kind: &str) {
    FAIL_NEXT_CLEANUP.with(|slot| *slot.borrow_mut() = Some(kind.to_string()));
}

#[cfg(test)]
fn set_fail_next_identity_query() {
    FAIL_NEXT_IDENTITY_QUERY.with(|flag| flag.set(true));
}

#[cfg(test)]
pub(super) fn set_before_checked_displace_hook(hook: impl FnMut(&Path) -> bool + 'static) {
    BEFORE_CHECKED_DISPLACE_HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

fn run_before_checked_displace_hook(path: &Path) {
    #[cfg(test)]
    BEFORE_CHECKED_DISPLACE_HOOK.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        if let Some(hook) = borrowed.as_mut() {
            if hook(path) {
                borrowed.take();
            }
        }
    });
    #[cfg(not(test))]
    let _ = path;
}

#[derive(Clone, Deserialize, Serialize)]
struct JournalEntry {
    relative_path: String,
    previous: Option<Vec<u8>>,
    desired_hash: String,
}

#[derive(Deserialize, Serialize)]
struct Journal {
    entries: Vec<JournalEntry>,
}

pub struct FileTransaction {
    backups: Vec<(PathBuf, Option<Vec<u8>>)>,
    created_dirs: Vec<PathBuf>,
    recovery_artifacts: Vec<PathBuf>,
    installed_ownership: std::collections::HashMap<PathBuf, InstalledOwnership>,
    unverified_installs: std::collections::HashSet<PathBuf>,
    guard_by_destination: std::collections::HashMap<PathBuf, PathBuf>,
    project_root: Option<PathBuf>,
    journal_path: Option<PathBuf>,
    journal_entries: Vec<JournalEntry>,
    finished: bool,
}

impl FileTransaction {
    pub fn new() -> Self {
        Self {
            backups: Vec::new(),
            created_dirs: Vec::new(),
            recovery_artifacts: Vec::new(),
            installed_ownership: std::collections::HashMap::new(),
            unverified_installs: std::collections::HashSet::new(),
            guard_by_destination: std::collections::HashMap::new(),
            project_root: None,
            journal_path: None,
            journal_entries: Vec::new(),
            finished: false,
        }
    }

    pub fn new_for_project(root: &Path) -> Self {
        let mut transaction = Self::new();
        transaction.project_root = Some(root.to_path_buf());
        transaction
    }

    pub fn reconcile_project(root: &Path) -> Result<(), BackendError> {
        let directory = root.join(".app/import-v2-journal");
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(io_error(error, &directory)),
        };
        for entry in entries {
            let path = entry.map_err(|error| io_error(error, &directory))?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") { continue; }
            let journal: Journal = serde_json::from_slice(
                &std::fs::read(&path).map_err(|error| io_error(error, &path))?,
            ).map_err(|_| staging_safe_io_error())?;
            for intent in journal.entries.iter().rev() {
                let target = root.join(&intent.relative_path);
                let current = std::fs::read(&target).ok();
                let current_hash = current.as_deref().map(digest_bytes);
                let previous_hash = intent.previous.as_deref().map(digest_bytes);
                if current_hash == previous_hash { continue; }
                if current_hash.as_deref() != Some(&intent.desired_hash) {
                    return Err(conflict_error());
                }
                match &intent.previous {
                    Some(bytes) => write_atomic_bytes(&target, bytes)?,
                    None => match std::fs::remove_file(&target) {
                        Ok(()) => if let Some(parent) = target.parent() { sync_parent(parent)?; },
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
                        Err(error) => return Err(io_error(error, &target)),
                    },
                }
            }
            std::fs::remove_file(&path).map_err(|error| io_error(error, &path))?;
            sync_parent(&directory)?;
        }
        Ok(())
    }

    fn record_intent(&mut self, path: &Path, previous: Option<Vec<u8>>, bytes: &[u8]) -> Result<(), BackendError> {
        let Some(root) = self.project_root.as_deref() else { return Ok(()); };
        let relative = path.strip_prefix(root).map_err(|_| BackendError::new(
            "PATH_INVALID", "Commit target is outside the project.", false, true,
        ))?.to_string_lossy().replace('\\', "/");
        self.journal_entries.push(JournalEntry { relative_path: relative, previous, desired_hash: digest_bytes(bytes) });
        let journal_path = self.journal_path.get_or_insert_with(|| root.join(format!(
            ".app/import-v2-journal/{}.json", uuid::Uuid::new_v4()
        ))).clone();
        if let Some(parent) = journal_path.parent() { std::fs::create_dir_all(parent).map_err(|error| io_error(error, parent))?; }
        let bytes = serde_json::to_vec(&Journal { entries: self.journal_entries.clone() })
            .map_err(|_| staging_safe_io_error())?;
        write_atomic_bytes(&journal_path, &bytes)
    }

    fn finish_journal(&mut self) -> Result<(), BackendError> {
        if let Some(path) = self.journal_path.take() {
            match std::fs::remove_file(&path) {
                Ok(()) => if let Some(parent) = path.parent() { sync_parent(parent)?; },
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
                Err(error) => return Err(io_error(error, &path)),
            }
        }
        Ok(())
    }

    pub fn write(&mut self, path: &Path, bytes: &[u8]) -> Result<(), BackendError> {
        let was_absent = !path.exists();
        if let Some(parent) = path.parent() {
            let mut missing: Vec<_> = parent
                .ancestors()
                .take_while(|candidate| !candidate.exists())
                .map(Path::to_path_buf)
                .collect();
            missing.reverse();
            for directory in missing {
                std::fs::create_dir(&directory).map_err(|error| io_error(error, &directory))?;
                self.created_dirs.push(directory);
            }
        }
        self.track(path)?;
        write_atomic_bytes(path, bytes)?;
        let _ = was_absent;
        self.capture_installed(path, bytes)?;
        Ok(())
    }

    pub fn write_new(&mut self, path: &Path, bytes: &[u8]) -> Result<(), BackendError> {
        self.ensure_parent(path)?;
        let parent = path.parent().unwrap();
        let temporary = write_synced_temp(parent, path, bytes)?;
        self.record_intent(path, None, bytes)?;
        self.recovery_artifacts.push(temporary.clone());
        #[cfg(test)]
        BEFORE_NEW_INSTALL_HOOK.with(|slot| {
            let mut borrowed = slot.borrow_mut();
            if let Some(hook) = borrowed.as_mut() {
                if hook(path) {
                    borrowed.take();
                }
            }
        });
        if let Err(error) = install_candidate(&temporary, path) {
            let cleanup = self.cleanup_artifact(&temporary);
            return match cleanup {
                Ok(()) => Err(io_error(error, path)),
                Err(cleanup) => Err(rollback_failure(
                    "New-file install failed and temporary cleanup failed.",
                    vec!["candidate install failed".to_string(), cleanup.message],
                )),
            };
        }
        self.backups.push((path.to_path_buf(), None));
        self.capture_installed(path, bytes)?;
        self.cleanup_artifact(&temporary)
    }

    fn ensure_parent(&mut self, path: &Path) -> Result<(), BackendError> {
        if let Some(parent) = path.parent() {
            let mut missing: Vec<_> = parent
                .ancestors()
                .take_while(|candidate| !candidate.exists())
                .map(Path::to_path_buf)
                .collect();
            missing.reverse();
            for directory in missing {
                std::fs::create_dir(&directory).map_err(|error| io_error(error, &directory))?;
                self.created_dirs.push(directory);
            }
        }
        Ok(())
    }

    fn cleanup_artifact(&mut self, path: &Path) -> Result<(), BackendError> {
        #[cfg(test)]
        {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let should_fail = FAIL_NEXT_CLEANUP.with(|slot| {
                let matches = slot.borrow().as_deref().is_some_and(|kind| {
                    (kind == "guard" && name.contains("guard"))
                        || (kind == "tmp" && name.contains("tmp"))
                });
                if matches {
                    slot.borrow_mut().take();
                }
                matches
            });
            if should_fail {
                return Err(io_error(
                    std::io::Error::other("injected cleanup failure"),
                    path,
                ));
            }
        }
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error(error, path)),
        }
        self.recovery_artifacts
            .retain(|candidate| candidate != path);
        Ok(())
    }

    pub fn track(&mut self, path: &Path) -> Result<(), BackendError> {
        if !self.backups.iter().any(|(existing, _)| existing == path) {
            let previous = path
                .exists()
                .then(|| std::fs::read(path).map_err(|error| io_error(error, path)))
                .transpose()?;
            self.backups.push((path.to_path_buf(), previous));
        }
        Ok(())
    }

    pub fn write_if_hash_matches(
        &mut self,
        path: &Path,
        bytes: &[u8],
        expected_hash: &str,
    ) -> Result<(), BackendError> {
        let parent = path.parent().ok_or_else(|| {
            BackendError::new(
                "PATH_INVALID",
                "Cannot determine parent directory.",
                false,
                true,
            )
        })?;
        let temporary = write_synced_temp(parent, path, bytes)?;
        let guard = parent.join(format!(".wiki-guard-{}", uuid::Uuid::new_v4()));
        self.recovery_artifacts.push(temporary.clone());
        run_before_checked_displace_hook(path);
        if let Err(error) = std::fs::hard_link(path, &guard) {
            let _ = self.cleanup_artifact(&temporary);
            return Err(io_error(error, path));
        }
        self.recovery_artifacts.push(guard.clone());
        self.guard_by_destination
            .insert(path.to_path_buf(), guard.clone());
        let before_identity = file_identity(path)?;
        let guard_identity = file_identity(&guard)?;
        let previous_before = match std::fs::read(&guard) {
            Ok(bytes) => bytes,
            Err(error) => {
                let primary = io_error(error, &guard);
                let _ = self.cleanup_artifact(&temporary);
                let _ = self.cleanup_artifact(&guard);
                return Err(primary);
            }
        };
        if before_identity != guard_identity
            || format!("{:x}", Sha256::digest(&previous_before)) != expected_hash
        {
            let _ = self.cleanup_artifact(&temporary);
            let _ = self.cleanup_artifact(&guard);
            return Err(conflict_error());
        }
        self.record_intent(path, Some(previous_before.clone()), bytes)?;
        if let Err(error) = replace_existing(&temporary, path) {
            let _ = self.cleanup_artifact(&temporary);
            let _ = self.cleanup_artifact(&guard);
            return Err(io_error(error, path));
        }
        self.recovery_artifacts
            .retain(|candidate| candidate != &temporary);
        let previous = std::fs::read(&guard).map_err(|_| staging_safe_io_error())?;
        if format!("{:x}", Sha256::digest(&previous)) != expected_hash {
            replace_existing(&guard, path).map_err(|error| io_error(error, path))?;
            self.recovery_artifacts.retain(|candidate| candidate != &guard);
            sync_parent(parent)?;
            return Err(conflict_error());
        }
        sync_parent(parent)?;
        self.backups.push((path.to_path_buf(), Some(previous)));
        self.capture_installed(path, bytes)?;
        self.cleanup_artifact(&temporary)?;
        self.cleanup_artifact(&guard)
    }

    pub fn commit(mut self) -> Result<(), BackendError> {
        for artifact in self.recovery_artifacts.clone() {
            if let Err(error) = self.cleanup_artifact(&artifact) {
                return Err(self.rollback_after(error));
            }
        }
        self.finish_journal()?;
        self.finished = true;
        Ok(())
    }

    fn capture_installed(&mut self, path: &Path, bytes: &[u8]) -> Result<(), BackendError> {
        self.unverified_installs.insert(path.to_path_buf());
        #[cfg(test)]
        if FAIL_NEXT_IDENTITY_QUERY.with(|flag| flag.replace(false)) {
            return Err(io_error(
                std::io::Error::other("injected identity query failure"),
                path,
            ));
        }
        let anchor = std::fs::File::open(path).map_err(|error| io_error(error, path))?;
        let identity = file_identity_from_file(&anchor, path)?;
        self.installed_ownership.insert(
            path.to_path_buf(),
            InstalledOwnership {
                identity,
                hash: digest_bytes(bytes),
                _anchor: anchor,
            },
        );
        self.unverified_installs.remove(path);
        Ok(())
    }

    pub(super) fn rollback(&mut self) -> Result<(), BackendError> {
        let mut failures = Vec::new();
        for (path, previous) in self.backups.iter().rev() {
            if self.unverified_installs.contains(path) {
                failures.push(format!("{}: installed ownership could not be verified; preserved instead of rolling back", self.actionable_path(path)));
                continue;
            }
            if let Some(ownership) = self.installed_ownership.get(path) {
                if !path.exists() {
                    failures.push(format!(
                        "{}: destination was deleted externally; preserved instead of rolling back",
                        self.actionable_path(path)
                    ));
                    continue;
                } else {
                    let identity_changed =
                        file_identity(path).ok().as_ref() != Some(&ownership.identity);
                    let content_changed = std::fs::read(path)
                        .ok()
                        .map(|bytes| digest_bytes(&bytes))
                        .as_ref()
                        != Some(&ownership.hash);
                    if identity_changed || content_changed {
                        failures.push(format!(
                            "{}: destination changed externally; preserved instead of rolling back",
                            self.actionable_path(path)
                        ));
                        continue;
                    }
                }
            }
            match previous {
                Some(bytes) => {
                    if let Err(_error) = write_atomic_bytes(path, bytes) {
                        failures.push(format!("{}: rollback write failed", self.actionable_path(path)));
                    }
                }
                None => {
                    if let Err(error) = std::fs::remove_file(path) {
                        if error.kind() != std::io::ErrorKind::NotFound {
                            failures.push(format!("{}: rollback removal failed", self.actionable_path(path)));
                        }
                    }
                }
            }
        }
        for artifact in self.recovery_artifacts.clone().iter().rev() {
            if self
                .guard_by_destination
                .iter()
                .any(|(destination, guard)| {
                    guard == artifact && self.unverified_installs.contains(destination)
                })
            {
                failures.push(format!(
                    "{}: retained original recovery guard after unverified install",
                    self.actionable_path(artifact)
                ));
                continue;
            }
            if let Err(_error) = self.cleanup_artifact(artifact) {
                failures.push(format!("{}: recovery cleanup failed", self.actionable_path(artifact)));
            }
        }
        for directory in self.created_dirs.iter().rev() {
            if let Err(error) = std::fs::remove_dir(directory) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    failures.push(format!("{}: directory cleanup failed", self.actionable_path(directory)));
                }
            }
        }
        self.finished = true;
        if failures.is_empty() {
            if let Err(_error) = self.finish_journal() {
                failures.push(".app/import-v2-journal: cleanup failed".to_string());
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(rollback_failure(
                "Import rollback was incomplete.",
                failures,
            ))
        }
    }

    fn actionable_path(&self, path: &Path) -> String {
        self.project_root
            .as_deref()
            .and_then(|root| path.strip_prefix(root).ok())
            .map(|relative| relative.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|| path.to_string_lossy().replace('\\', "/"))
    }

    pub(super) fn rollback_after(&mut self, primary: BackendError) -> BackendError {
        match self.rollback() {
            Ok(()) => primary,
            Err(rollback) => BackendError::new(
                IMPORT_V2_COMMIT_FAILED,
                "Import commit failed and rollback was incomplete.",
                false,
                true,
            )
            .with_details(serde_json::json!({
                "primaryCode": primary.code,
                "primaryError": primary.message,
                "rollbackError": rollback.message,
                "rollbackDetails": rollback.details,
            })),
        }
    }
}

impl Drop for FileTransaction {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.rollback();
        }
    }
}

fn write_atomic_bytes(path: &Path, bytes: &[u8]) -> Result<(), BackendError> {
    let parent = path.parent().ok_or_else(|| {
        BackendError::new(
            "PATH_INVALID",
            "Cannot determine parent directory.",
            false,
            true,
        )
    })?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let temporary = parent.join(format!(".{file_name}.{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file =
            std::fs::File::create(&temporary).map_err(|error| io_error(error, &temporary))?;
        file.write_all(bytes)
            .map_err(|error| io_error(error, &temporary))?;
        file.sync_all()
            .map_err(|error| io_error(error, &temporary))?;
        if path.exists() {
            replace_existing(&temporary, path).map_err(|error| io_error(error, path))?;
        } else {
            std::fs::rename(&temporary, path).map_err(|error| io_error(error, path))?;
        }
        sync_parent(parent)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn write_synced_temp(parent: &Path, path: &Path, bytes: &[u8]) -> Result<PathBuf, BackendError> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let temporary = parent.join(format!(".{file_name}.{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file =
            std::fs::File::create(&temporary).map_err(|error| io_error(error, &temporary))?;
        file.write_all(bytes)
            .map_err(|error| io_error(error, &temporary))?;
        file.sync_all()
            .map_err(|error| io_error(error, &temporary))?;
        Ok(temporary.clone())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn install_candidate(temporary: &Path, path: &Path) -> Result<(), std::io::Error> {
    #[cfg(test)]
    if FAIL_NEXT_CANDIDATE_INSTALL.with(|flag| flag.replace(false)) {
        return Err(std::io::Error::other("injected candidate install failure"));
    }
    std::fs::hard_link(temporary, path)?;
    if let Some(parent) = path.parent() {
        sync_parent(parent).map_err(|error| std::io::Error::other(error.message))?;
    }
    Ok(())
}

fn conflict_error() -> BackendError {
    BackendError::new(
        IMPORT_V2_COMMIT_CONFLICT,
        "A commit target changed concurrently.",
        true,
        true,
    )
}

fn staging_safe_io_error() -> BackendError {
    BackendError::new(
        "FILE_READ_FAILED",
        "A project file could not be read safely.",
        true,
        true,
    )
}

#[cfg(unix)]
fn replace_existing(temporary: &Path, path: &Path) -> Result<(), std::io::Error> {
    std::fs::rename(temporary, path)
}

#[cfg(windows)]
fn replace_existing(temporary: &Path, path: &Path) -> Result<(), std::io::Error> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn ReplaceFileW(replaced: *const u16, replacement: *const u16, backup: *const u16, flags: u32, exclude: *mut std::ffi::c_void, reserved: *mut std::ffi::c_void) -> i32;
    }
    let replaced: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let replacement: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: both paths are valid NUL-terminated UTF-16 buffers for the duration
    // of the synchronous Win32 call; optional pointers are null as documented.
    let result = unsafe {
        ReplaceFileW(
            replaced.as_ptr(), replacement.as_ptr(), std::ptr::null(), 0,
            std::ptr::null_mut(), std::ptr::null_mut(),
        )
    };
    if result == 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) }
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> Result<(), BackendError> {
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| io_error(error, parent))
}

#[cfg(windows)]
fn sync_parent(_parent: &Path) -> Result<(), BackendError> {
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn sync_parent(_parent: &Path) -> Result<(), BackendError> {
    Ok(())
}

fn rollback_failure(message: &str, failures: Vec<String>) -> BackendError {
    BackendError::new(IMPORT_V2_COMMIT_FAILED, message, false, true)
        .with_details(serde_json::json!({ "rollbackFailures": failures }))
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity(u64, u64);

struct InstalledOwnership {
    identity: FileIdentity,
    hash: String,
    _anchor: std::fs::File,
}

#[cfg(unix)]
fn file_identity(path: &Path) -> Result<FileIdentity, BackendError> {
    let file = std::fs::File::open(path).map_err(|error| io_error(error, path))?;
    file_identity_from_file(&file, path)
}

#[cfg(unix)]
fn file_identity_from_file(
    file: &std::fs::File,
    path: &Path,
) -> Result<FileIdentity, BackendError> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata().map_err(|error| io_error(error, path))?;
    Ok(FileIdentity(metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
fn file_identity(path: &Path) -> Result<FileIdentity, BackendError> {
    let file = std::fs::File::open(path).map_err(|error| io_error(error, path))?;
    file_identity_from_file(&file, path)
}

#[cfg(windows)]
fn file_identity_from_file(
    file: &std::fs::File,
    path: &Path,
) -> Result<FileIdentity, BackendError> {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;

    #[repr(C)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[repr(C)]
    struct ByHandleFileInformation {
        attributes: u32,
        creation: FileTime,
        last_access: FileTime,
        last_write: FileTime,
        volume_serial: u32,
        size_high: u32,
        size_low: u32,
        links: u32,
        index_high: u32,
        index_low: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandle(
            handle: *mut c_void,
            information: *mut ByHandleFileInformation,
        ) -> i32;
    }

    let mut information = std::mem::MaybeUninit::<ByHandleFileInformation>::uninit();
    // SAFETY: `file` owns a live Windows handle for the duration of the call and
    // `information` points to writable storage of the exact Win32 structure layout.
    let succeeded = unsafe {
        GetFileInformationByHandle(file.as_raw_handle().cast(), information.as_mut_ptr())
    };
    if succeeded == 0 {
        return Err(io_error(std::io::Error::last_os_error(), path));
    }
    // SAFETY: Win32 reports success only after initializing the full output structure.
    let information = unsafe { information.assume_init() };
    Ok(FileIdentity(
        u64::from(information.volume_serial),
        (u64::from(information.index_high) << 32) | u64::from(information.index_low),
    ))
}

#[cfg(not(any(unix, windows)))]
fn file_identity(path: &Path) -> Result<FileIdentity, BackendError> {
    Err(BackendError::new(
        IMPORT_V2_COMMIT_FAILED,
        "Stable file identity is unavailable on this platform.",
        false,
        true,
    )
    .with_details(serde_json::json!({ "path": path.to_string_lossy() })))
}

#[cfg(not(any(unix, windows)))]
fn file_identity_from_file(
    _file: &std::fs::File,
    path: &Path,
) -> Result<FileIdentity, BackendError> {
    file_identity(path)
}

fn io_error(error: std::io::Error, path: &Path) -> BackendError {
    let _ = (error, path);
    BackendError::new(
        "FILE_WRITE_FAILED",
        "A project file operation failed.",
        true,
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        set_before_checked_displace_hook, set_before_new_install_hook,
        set_fail_next_candidate_install, set_fail_next_cleanup, set_fail_next_identity_query,
        digest_bytes, FileTransaction, IMPORT_V2_COMMIT_CONFLICT,
    };
    use sha2::Digest;

    #[test]
    fn dropped_transaction_restores_overwrites_and_removes_created_tree() {
        let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let existing = root.join("existing.md");
        std::fs::write(&existing, b"before").unwrap();
        {
            let mut transaction = FileTransaction::new();
            transaction.write(&existing, b"after").unwrap();
            transaction
                .write(&root.join("new/tree/file.bin"), b"created")
                .unwrap();
        }
        assert_eq!(std::fs::read(&existing).unwrap(), b"before");
        assert!(!root.join("new").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn committed_transaction_keeps_all_writes() {
        let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        let path = root.join("nested/file.bin");
        let mut transaction = FileTransaction::new();
        transaction.write(&path, b"kept").unwrap();
        transaction.commit().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"kept");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tracked_external_write_is_rolled_back() {
        let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("session.json");
        std::fs::write(&path, b"preview").unwrap();
        {
            let mut transaction = FileTransaction::new();
            transaction.track(&path).unwrap();
            std::fs::write(&path, b"completed").unwrap();
        }
        assert_eq!(std::fs::read(&path).unwrap(), b"preview");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn checked_write_rejects_hash_drift_without_mutation() {
        let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("wiki.md");
        std::fs::write(&path, b"external edit").unwrap();
        let mut transaction = FileTransaction::new();
        let error = transaction
            .write_if_hash_matches(&path, b"candidate", "stale")
            .unwrap_err();
        assert_eq!(error.code, crate::errors::IMPORT_V2_COMMIT_CONFLICT);
        assert_eq!(std::fs::read(&path).unwrap(), b"external edit");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn checked_replace_detects_edit_injected_at_compare_replace_boundary() {
        let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("wiki.md");
        std::fs::write(&path, b"expected").unwrap();
        let expected = format!("{:x}", sha2::Sha256::digest(b"expected"));
        set_before_checked_displace_hook(|path| {
            std::fs::write(path, b"external edit").unwrap();
            true
        });
        let mut transaction = FileTransaction::new();
        let error = transaction
            .write_if_hash_matches(&path, b"candidate", &expected)
            .unwrap_err();
        assert_eq!(error.code, crate::errors::IMPORT_V2_COMMIT_CONFLICT);
        assert_eq!(std::fs::read(&path).unwrap(), b"external edit");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explicit_rollback_surfaces_restore_failure() {
        let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("existing");
        std::fs::write(&path, b"before").unwrap();
        let mut transaction = FileTransaction::new();
        transaction.write(&path, b"after").unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        let error = transaction.rollback().unwrap_err();
        assert_eq!(error.code, crate::errors::IMPORT_V2_COMMIT_FAILED);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explicit_rollback_surfaces_created_file_removal_failure() {
        let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("created");
        let mut transaction = FileTransaction::new();
        transaction.write(&path, b"new").unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        let error = transaction.rollback().unwrap_err();
        assert_eq!(error.code, crate::errors::IMPORT_V2_COMMIT_FAILED);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rollback_after_primary_failure_reports_hard_recovery_error() {
        let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("created");
        let mut transaction = FileTransaction::new();
        transaction.write(&path, b"new").unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        let primary = crate::errors::BackendError::new("PRIMARY", "primary failure", true, false);
        let error = transaction.rollback_after(primary);
        assert_eq!(error.code, crate::errors::IMPORT_V2_COMMIT_FAILED);
        let details = error.details.unwrap();
        assert_eq!(details["primaryCode"], "PRIMARY");
        assert!(details["rollbackError"]
            .as_str()
            .unwrap()
            .contains("incomplete"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn candidate_install_failure_restores_original_canonical_wiki() {
        let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("wiki.md");
        std::fs::write(&path, b"expected").unwrap();
        let expected = format!("{:x}", sha2::Sha256::digest(b"expected"));
        set_fail_next_candidate_install();
        let mut transaction = FileTransaction::new();
        let error = transaction
            .write_if_hash_matches(&path, b"candidate", &expected)
            .unwrap_err();
        assert_eq!(error.code, "FILE_WRITE_FAILED");
        assert_eq!(std::fs::read(&path).unwrap(), b"expected");
        assert!(!std::fs::read_dir(&root).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("wiki-guard")));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn new_write_never_clobbers_concurrent_target() {
        let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("new.bin");
        set_before_new_install_hook(|path| {
            std::fs::write(path, b"external").unwrap();
            true
        });
        let mut transaction = FileTransaction::new();
        transaction.write_new(&path, b"candidate").unwrap_err();
        assert_eq!(std::fs::read(&path).unwrap(), b"external");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn temp_cleanup_failure_is_retried_by_explicit_rollback() {
        let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("new.bin");
        set_fail_next_cleanup("tmp");
        let mut transaction = FileTransaction::new();
        transaction.write_new(&path, b"candidate").unwrap();
        transaction.commit().unwrap_err();
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn guard_cleanup_failure_is_retried_by_explicit_rollback() {
        let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("wiki.md");
        std::fs::write(&path, b"expected").unwrap();
        let expected = format!("{:x}", sha2::Sha256::digest(b"expected"));
        set_fail_next_cleanup("guard");
        let mut transaction = FileTransaction::new();
        transaction
            .write_if_hash_matches(&path, b"candidate", &expected)
            .unwrap_err();
        transaction.rollback().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"expected");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rollback_preserves_externally_replaced_new_destination() {
        let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("new.bin");
        let mut transaction = FileTransaction::new();
        transaction.write_new(&path, b"candidate").unwrap();
        std::fs::write(&path, b"external replacement").unwrap();
        let error = transaction.rollback().unwrap_err();
        assert_eq!(error.code, crate::errors::IMPORT_V2_COMMIT_FAILED);
        assert_eq!(std::fs::read(&path).unwrap(), b"external replacement");
        assert!(error.details.unwrap()["rollbackFailures"][0]
            .as_str()
            .unwrap()
            .contains(path.to_string_lossy().as_ref()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rollback_preserves_same_content_external_replacement_by_identity() {
        let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("new.bin");
        let mut transaction = FileTransaction::new();
        transaction.write_new(&path, b"same bytes").unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, b"same bytes").unwrap();
        transaction.rollback().unwrap_err();
        assert_eq!(std::fs::read(&path).unwrap(), b"same bytes");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn overwrite_rollback_preserves_post_write_external_replacement() {
        let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("existing.bin");
        std::fs::write(&path, b"before").unwrap();
        let mut transaction = FileTransaction::new();
        transaction.write(&path, b"after").unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, b"after").unwrap();
        transaction.rollback().unwrap_err();
        assert_eq!(std::fs::read(&path).unwrap(), b"after");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn overwrite_rollback_preserves_external_deletion() {
        let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("existing.bin");
        std::fs::write(&path, b"before").unwrap();
        let mut transaction = FileTransaction::new();
        transaction.write(&path, b"after").unwrap();
        std::fs::remove_file(&path).unwrap();
        transaction.rollback().unwrap_err();
        assert!(!path.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn identity_failure_preserves_candidate_and_original_guard_with_actionable_path() {
        let outer = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        let root = outer.join("wiki/projects/demo");
        let wiki_dir = root.join("wiki/notes");
        std::fs::create_dir_all(&wiki_dir).unwrap();
        let path = wiki_dir.join("page.md");
        std::fs::write(&path, b"original").unwrap();
        let expected = format!("{:x}", sha2::Sha256::digest(b"original"));
        set_fail_next_identity_query();
        let mut transaction = FileTransaction::new_for_project(&root);
        let primary = transaction
            .write_if_hash_matches(&path, b"candidate", &expected)
            .unwrap_err();
        let error = transaction.rollback_after(primary);
        assert_eq!(error.code, crate::errors::IMPORT_V2_COMMIT_FAILED);
        assert_eq!(std::fs::read(&path).unwrap(), b"candidate");
        let guards: Vec<_> = std::fs::read_dir(&wiki_dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .contains("wiki-guard")
            })
            .collect();
        assert_eq!(guards.len(), 1);
        assert_eq!(std::fs::read(&guards[0]).unwrap(), b"original");
        assert_eq!(std::fs::read_dir(&wiki_dir).unwrap().count(), 2);
        let details = error.details.unwrap();
        assert!(details.to_string().contains("wiki/notes/.wiki-guard-"));
        std::fs::remove_dir_all(outer).unwrap();
    }

    #[test]
    fn durable_journal_reconciles_formal_and_session_writes_after_abandonment() {
        let root = std::env::temp_dir().join(format!("import-v2-journal-{}", uuid::Uuid::new_v4()));
        let formal = root.join("wiki/new.md");
        let session = root.join(".app/import-sessions/s1/session.json");
        std::fs::create_dir_all(session.parent().unwrap()).unwrap();
        std::fs::write(&session, b"before").unwrap();
        let expected = digest_bytes(b"before");
        let mut transaction = FileTransaction::new_for_project(&root);
        transaction.write_new(&formal, b"formal").unwrap();
        transaction.write_if_hash_matches(&session, b"after", &expected).unwrap();
        std::mem::forget(transaction);

        assert_eq!(std::fs::read(&formal).unwrap(), b"formal");
        assert_eq!(std::fs::read(&session).unwrap(), b"after");
        FileTransaction::reconcile_project(&root).unwrap();
        assert!(!formal.exists());
        assert_eq!(std::fs::read(&session).unwrap(), b"before");
        assert_eq!(std::fs::read_dir(root.join(".app/import-v2-journal")).unwrap().count(), 0);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn durable_journal_preserves_external_drift_during_reconciliation() {
        let root = std::env::temp_dir().join(format!("import-v2-journal-{}", uuid::Uuid::new_v4()));
        let target = root.join("wiki/new.md");
        let mut transaction = FileTransaction::new_for_project(&root);
        transaction.write_new(&target, b"committed").unwrap();
        std::mem::forget(transaction);
        std::fs::write(&target, b"external").unwrap();

        let error = FileTransaction::reconcile_project(&root).unwrap_err();
        assert_eq!(error.code, IMPORT_V2_COMMIT_CONFLICT);
        assert_eq!(std::fs::read(&target).unwrap(), b"external");
        std::fs::remove_dir_all(root).unwrap();
    }
}
