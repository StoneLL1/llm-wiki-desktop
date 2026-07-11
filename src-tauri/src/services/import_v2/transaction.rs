use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::errors::{BackendError, IMPORT_V2_COMMIT_CONFLICT, IMPORT_V2_COMMIT_FAILED};

#[cfg(test)]
thread_local! {
    static BEFORE_CHECKED_DISPLACE_HOOK: std::cell::RefCell<Option<Box<dyn FnMut(&Path) -> bool>>> =
        std::cell::RefCell::new(None);
    static FAIL_NEXT_CANDIDATE_INSTALL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static BEFORE_NEW_INSTALL_HOOK: std::cell::RefCell<Option<Box<dyn FnMut(&Path) -> bool>>> = std::cell::RefCell::new(None);
    static FAIL_NEXT_CLEANUP: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);
    static FAIL_NEXT_IDENTITY_QUERY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static BEFORE_RECOVERY_MUTATION_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce(&Path)>>> =
        std::cell::RefCell::new(None);
    static SIMULATED_PROCESS_ABORT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn commit_fault_boundary(phase: &str, relative: Option<&str>) {
    if super::commit::run_commit_persistence_hook(phase, relative) {
        SIMULATED_PROCESS_ABORT.with(|flag| flag.set(true));
        panic!("simulated Import V2 process abort at {phase}");
    }
}

#[cfg(test)]
fn set_before_recovery_mutation_hook(hook: impl FnOnce(&Path) + 'static) {
    BEFORE_RECOVERY_MUTATION_HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

fn run_before_recovery_mutation_hook(path: &Path) {
    #[cfg(test)]
    BEFORE_RECOVERY_MUTATION_HOOK.with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook(path);
        }
    });
    #[cfg(not(test))]
    let _ = path;
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
    #[serde(default)]
    installed_identity: Option<FileIdentity>,
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
enum JournalState {
    InProgress,
    Committed,
}

#[derive(Deserialize, Serialize)]
struct Journal {
    state: JournalState,
    entries: Vec<JournalEntry>,
    #[serde(default)]
    recovery_artifacts: Vec<String>,
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
    journal_artifacts: Vec<String>,
    finished: bool,
}

impl FileTransaction {
    #[cfg(test)]
    fn simulate_process_crash(mut self) {
        // A real process death closes ownership anchors but skips Drop/rollback.
        self.installed_ownership.clear();
        std::mem::forget(self);
    }
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
            journal_artifacts: Vec::new(),
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
        match std::fs::symlink_metadata(&directory) {
            Ok(metadata)
                if !metadata.is_dir()
                    || metadata.file_type().is_symlink()
                    || transaction_is_reparse_point(&metadata) =>
            {
                return Err(staging_safe_io_error());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(io_error(error, &directory)),
        }
        // Retain the journal parent namespace while enumerating, reading, and
        // deleting entries. Windows denies a directory swap because this handle
        // omits FILE_SHARE_DELETE; Unix journal entries are additionally opened
        // with O_NOFOLLOW below.
        let journal_binding = bind_recovery_parent(root, &directory.join("entry"))?;
        let entries: Vec<PathBuf> = match std::fs::read_dir(&directory) {
            Ok(entries) => entries
                .map(|entry| {
                    entry
                        .map(|entry| entry.path())
                        .map_err(|error| io_error(error, &directory))
                })
                .collect::<Result<_, _>>()?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(io_error(error, &directory)),
        };
        // Collect first so Windows' ReadDirectoryChanges/read-directory handle is
        // closed before we reopen the directory for a durable metadata flush.
        for path in entries {
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let metadata =
                std::fs::symlink_metadata(&path).map_err(|error| io_error(error, &path))?;
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || transaction_is_reparse_point(&metadata)
            {
                return Err(staging_safe_io_error());
            }
            let journal: Journal =
                serde_json::from_slice(&read_regular_nofollow(&journal_binding, &path)?)
                    .map_err(|_| staging_safe_io_error())?;
            let intents: Box<dyn Iterator<Item = &JournalEntry>> =
                if journal.state == JournalState::Committed {
                    Box::new(journal.entries.iter())
                } else {
                    Box::new(journal.entries.iter().rev())
                };
            for intent in intents {
                let target = safe_journal_target(root, &intent.relative_path)?;
                let parent_binding = bind_recovery_parent(root, &target)?;
                let current = match std::fs::read(&target) {
                    Ok(bytes) => Some(bytes),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => return Err(io_error(error, &target)),
                };
                let current_hash = current.as_deref().map(digest_bytes);
                if journal.state == JournalState::Committed {
                    if current_hash.as_deref() != Some(&intent.desired_hash)
                        || !journal_identity_matches(&target, intent)?
                    {
                        return Err(conflict_error());
                    }
                    continue;
                }
                let previous_hash = intent.previous.as_deref().map(digest_bytes);
                if current_hash != previous_hash {
                    if current_hash.as_deref() != Some(&intent.desired_hash)
                        || !journal_identity_matches(&target, intent)?
                    {
                        return Err(conflict_error());
                    }
                    match &intent.previous {
                        Some(bytes) => {
                            bound_restore_bytes(&parent_binding, &target, bytes)?;
                        }
                        None => {
                            run_before_recovery_mutation_hook(&target);
                            match bound_remove_file(&parent_binding, &target) {
                                Ok(()) => {
                                    if let Some(parent) = target.parent() {
                                        sync_parent(parent)?;
                                    }
                                }
                                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                                Err(error) => return Err(io_error(error, &target)),
                            }
                        }
                    }
                }
            }
            for relative in &journal.recovery_artifacts {
                let artifact = safe_journal_target(root, relative)?;
                let parent_binding = bind_recovery_parent(root, &artifact)?;
                let name = artifact
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default();
                if !(name.contains(".tmp") || name.contains(".wiki-guard-")) {
                    return Err(staging_safe_io_error());
                }
                run_before_recovery_mutation_hook(&artifact);
                match bound_remove_file(&parent_binding, &artifact) {
                    Ok(()) => {
                        if let Some(parent) = artifact.parent() {
                            sync_parent(parent)?;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(io_error(error, &artifact)),
                }
            }
            bound_remove_file(&journal_binding, &path).map_err(|error| io_error(error, &path))?;
            sync_parent(&directory)?;
        }
        Ok(())
    }

    fn record_intent(
        &mut self,
        path: &Path,
        previous: Option<Vec<u8>>,
        bytes: &[u8],
        candidate_identity: FileIdentity,
    ) -> Result<(), BackendError> {
        let Some(root) = self.project_root.as_deref() else {
            return Ok(());
        };
        let relative = path
            .strip_prefix(root)
            .map_err(|_| {
                BackendError::new(
                    "PATH_INVALID",
                    "Commit target is outside the project.",
                    false,
                    true,
                )
            })?
            .to_string_lossy()
            .replace('\\', "/");
        self.journal_entries.push(JournalEntry {
            relative_path: relative.clone(),
            previous,
            desired_hash: digest_bytes(bytes),
            // The same-volume install primitives below preserve the candidate's
            // native identity. Persist it before the namespace mutation so a
            // crash immediately after install is still recoverable.
            installed_identity: Some(candidate_identity),
        });
        let journal_path = self
            .journal_path
            .get_or_insert_with(|| {
                root.join(format!(
                    ".app/import-v2-journal/{}.json",
                    uuid::Uuid::new_v4()
                ))
            })
            .clone();
        if let Some(parent) = journal_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| io_error(error, parent))?;
        }
        let bytes = serde_json::to_vec(&Journal {
            state: JournalState::InProgress,
            entries: self.journal_entries.clone(),
            recovery_artifacts: self.journal_artifacts.clone(),
        })
        .map_err(|_| staging_safe_io_error())?;
        write_atomic_bytes(&journal_path, &bytes)?;
        #[cfg(test)]
        commit_fault_boundary("intent", Some(&relative));
        Ok(())
    }

    fn record_recovery_artifact(&mut self, path: &Path) -> Result<(), BackendError> {
        let Some(root) = self.project_root.as_deref() else {
            return Ok(());
        };
        self.journal_artifacts.push(
            path.strip_prefix(root)
                .map_err(|_| staging_safe_io_error())?
                .to_string_lossy()
                .replace('\\', "/"),
        );
        let journal_path = self
            .journal_path
            .as_deref()
            .ok_or_else(staging_safe_io_error)?;
        let bytes = serde_json::to_vec(&Journal {
            state: JournalState::InProgress,
            entries: self.journal_entries.clone(),
            recovery_artifacts: self.journal_artifacts.clone(),
        })
        .map_err(|_| staging_safe_io_error())?;
        write_atomic_bytes(journal_path, &bytes)
    }

    fn mark_journal_committed(&self) -> Result<(), BackendError> {
        let Some(path) = self.journal_path.as_deref() else {
            return Ok(());
        };
        let bytes = serde_json::to_vec(&Journal {
            state: JournalState::Committed,
            entries: self.journal_entries.clone(),
            recovery_artifacts: self.journal_artifacts.clone(),
        })
        .map_err(|_| staging_safe_io_error())?;
        // Atomic write syncs the marker file and then its containing directory.
        write_atomic_bytes(path, &bytes)
    }

    fn finish_journal(&mut self) -> Result<(), BackendError> {
        if let Some(path) = self.journal_path.take() {
            match std::fs::remove_file(&path) {
                Ok(()) => {
                    if let Some(parent) = path.parent() {
                        sync_parent(parent)?;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
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
        let candidate_identity = file_identity(&temporary)?;
        self.record_intent(path, None, bytes, candidate_identity)?;
        self.recovery_artifacts.push(temporary.clone());
        self.record_recovery_artifact(&temporary)?;
        let parent_binding = self.bind_mutation_parent(path)?;
        #[cfg(test)]
        BEFORE_NEW_INSTALL_HOOK.with(|slot| {
            let mut borrowed = slot.borrow_mut();
            if let Some(hook) = borrowed.as_mut() {
                if hook(path) {
                    borrowed.take();
                }
            }
        });
        if let Err(error) = install_candidate(&parent_binding, &temporary, path) {
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
        #[cfg(test)]
        if let Some(entry) = self.journal_entries.last() {
            commit_fault_boundary("installed", Some(entry.relative_path.as_str()));
        }
        self.capture_installed_expected(path, bytes, candidate_identity)?;
        // Keep the linked staging name tracked until commit so cleanup failure is
        // a transaction failure and rollback can retry it deterministically.
        Ok(())
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

    fn bind_mutation_parent(&self, path: &Path) -> Result<RecoveryParentBinding, BackendError> {
        if let Some(root) = self.project_root.as_deref() {
            return bind_recovery_parent(root, path);
        }
        let parent = path.parent().ok_or_else(staging_safe_io_error)?;
        let metadata =
            std::fs::symlink_metadata(parent).map_err(|error| io_error(error, parent))?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || transaction_is_reparse_point(&metadata)
        {
            return Err(staging_safe_io_error());
        }
        Ok(RecoveryParentBinding {
            components: vec![(parent.to_path_buf(), namespace_identity(parent)?)],
            parent: parent.to_path_buf(),
            _anchor: open_directory_anchor(parent)?,
        })
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
        let parent_binding = self.bind_mutation_parent(path)?;
        run_before_checked_displace_hook(path);
        if let Err(error) = bound_hard_link(&parent_binding, path, &guard) {
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
        let candidate_identity = file_identity(&temporary)?;
        self.record_intent(
            path,
            Some(previous_before.clone()),
            bytes,
            candidate_identity,
        )?;
        self.record_recovery_artifact(&temporary)?;
        self.record_recovery_artifact(&guard)?;
        if let Err(error) = bound_replace_existing(&parent_binding, &temporary, path) {
            let _ = self.cleanup_artifact(&temporary);
            let _ = self.cleanup_artifact(&guard);
            return Err(io_error(error, path));
        }
        #[cfg(test)]
        if let Some(entry) = self.journal_entries.last() {
            commit_fault_boundary("installed", Some(entry.relative_path.as_str()));
        }
        self.recovery_artifacts
            .retain(|candidate| candidate != &temporary);
        let previous = std::fs::read(&guard).map_err(|_| staging_safe_io_error())?;
        if format!("{:x}", Sha256::digest(&previous)) != expected_hash {
            bound_replace_existing(&parent_binding, &guard, path)
                .map_err(|error| io_error(error, path))?;
            self.recovery_artifacts
                .retain(|candidate| candidate != &guard);
            sync_parent(parent)?;
            return Err(conflict_error());
        }
        sync_parent(parent)?;
        self.backups.push((path.to_path_buf(), Some(previous)));
        self.capture_installed_expected(path, bytes, candidate_identity)?;
        self.cleanup_artifact(&temporary)?;
        self.cleanup_artifact(&guard)
    }

    pub fn commit(mut self) -> Result<(), BackendError> {
        for artifact in self.recovery_artifacts.clone() {
            if let Err(error) = self.cleanup_artifact(&artifact) {
                return Err(self.rollback_after(error));
            }
        }
        self.mark_journal_committed()?;
        #[cfg(test)]
        commit_fault_boundary("committed", None);
        self.finish_journal()?;
        #[cfg(test)]
        commit_fault_boundary("deleted", None);
        self.finished = true;
        Ok(())
    }

    fn capture_installed(&mut self, path: &Path, bytes: &[u8]) -> Result<(), BackendError> {
        let identity = file_identity(path)?;
        self.capture_installed_expected(path, bytes, identity)
    }

    fn capture_installed_expected(
        &mut self,
        path: &Path,
        bytes: &[u8],
        expected_identity: FileIdentity,
    ) -> Result<(), BackendError> {
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
        if identity != expected_identity {
            return Err(conflict_error());
        }
        self.installed_ownership.insert(
            path.to_path_buf(),
            InstalledOwnership {
                identity,
                hash: digest_bytes(bytes),
                _anchor: anchor,
            },
        );
        if let (Some(root), Some(journal_path)) =
            (self.project_root.as_deref(), self.journal_path.as_deref())
        {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| staging_safe_io_error())?
                .to_string_lossy()
                .replace('\\', "/");
            let entry = self
                .journal_entries
                .iter_mut()
                .rev()
                .find(|entry| entry.relative_path == relative)
                .ok_or_else(staging_safe_io_error)?;
            entry.installed_identity = Some(identity);
            let journal = serde_json::to_vec(&Journal {
                state: JournalState::InProgress,
                entries: self.journal_entries.clone(),
                recovery_artifacts: self.journal_artifacts.clone(),
            })
            .map_err(|_| staging_safe_io_error())?;
            write_atomic_bytes(journal_path, &journal)?;
        }
        self.unverified_installs.remove(path);
        Ok(())
    }

    pub(super) fn rollback(&mut self) -> Result<(), BackendError> {
        let mut failures = Vec::new();
        let mut ownership_valid = std::collections::HashMap::new();
        for (path, ownership) in &self.installed_ownership {
            let valid = path.exists()
                && file_identity(path).ok().as_ref() == Some(&ownership.identity)
                && std::fs::read(path)
                    .ok()
                    .map(|bytes| digest_bytes(&bytes))
                    .as_ref()
                    == Some(&ownership.hash);
            ownership_valid.insert(path.clone(), valid);
        }
        // Windows std File anchors intentionally deny replacement/deletion. They
        // have served their identity purpose; close them before rollback mutation.
        self.installed_ownership.clear();
        for (path, previous) in self.backups.iter().rev() {
            if self.unverified_installs.contains(path) {
                failures.push(format!("{}: installed ownership could not be verified; preserved instead of rolling back", self.actionable_path(path)));
                continue;
            }
            if let Some(valid) = ownership_valid.get(path) {
                if !path.exists() {
                    failures.push(format!(
                        "{}: destination was deleted externally; preserved instead of rolling back",
                        self.actionable_path(path)
                    ));
                    continue;
                } else {
                    if !valid {
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
                        failures.push(format!(
                            "{}: rollback write failed",
                            self.actionable_path(path)
                        ));
                    }
                }
                None => {
                    if let Err(error) = std::fs::remove_file(path) {
                        if error.kind() != std::io::ErrorKind::NotFound {
                            failures.push(format!(
                                "{}: rollback removal failed",
                                self.actionable_path(path)
                            ));
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
                failures.push(format!(
                    "{}: recovery cleanup failed",
                    self.actionable_path(artifact)
                ));
            }
        }
        for directory in self.created_dirs.iter().rev() {
            if let Err(error) = std::fs::remove_dir(directory) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    failures.push(format!(
                        "{}: directory cleanup failed",
                        self.actionable_path(directory)
                    ));
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
        #[cfg(test)]
        if SIMULATED_PROCESS_ABORT.with(|flag| flag.replace(false)) {
            self.installed_ownership.clear();
            self.finished = true;
            return;
        }
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

fn install_candidate(
    binding: &RecoveryParentBinding,
    temporary: &Path,
    path: &Path,
) -> Result<(), std::io::Error> {
    #[cfg(test)]
    if FAIL_NEXT_CANDIDATE_INSTALL.with(|flag| flag.replace(false)) {
        return Err(std::io::Error::other("injected candidate install failure"));
    }
    bound_hard_link(binding, temporary, path)?;
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

fn safe_journal_target(root: &Path, relative: &str) -> Result<PathBuf, BackendError> {
    if relative.is_empty()
        || relative.contains('\\')
        || relative.contains(':')
        || Path::new(relative)
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(BackendError::new(
            "PATH_INVALID",
            "Recovery target is invalid.",
            false,
            true,
        ));
    }
    let canonical_root = std::fs::canonicalize(root).map_err(|error| io_error(error, root))?;
    let target = root.join(relative);
    let existing_anchor = target
        .ancestors()
        .find(|candidate| candidate.exists())
        .ok_or_else(|| {
            BackendError::new("PATH_INVALID", "Recovery target is invalid.", false, true)
        })?;
    let canonical_anchor =
        std::fs::canonicalize(existing_anchor).map_err(|error| io_error(error, existing_anchor))?;
    if !canonical_anchor.starts_with(&canonical_root) {
        return Err(BackendError::new(
            "PATH_INVALID",
            "Recovery target is invalid.",
            false,
            true,
        ));
    }
    Ok(target)
}

struct RecoveryParentBinding {
    components: Vec<(PathBuf, FileIdentity)>,
    parent: PathBuf,
    _anchor: std::fs::File,
}

fn bind_recovery_parent(root: &Path, target: &Path) -> Result<RecoveryParentBinding, BackendError> {
    let parent = target.parent().ok_or_else(staging_safe_io_error)?;
    let relative = parent
        .strip_prefix(root)
        .map_err(|_| staging_safe_io_error())?;
    let mut current = root.to_path_buf();
    let mut components = vec![(current.clone(), namespace_identity(&current)?)];
    for part in relative.components() {
        current.push(part.as_os_str());
        let metadata =
            std::fs::symlink_metadata(&current).map_err(|error| io_error(error, &current))?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || transaction_is_reparse_point(&metadata)
        {
            return Err(staging_safe_io_error());
        }
        components.push((current.clone(), namespace_identity(&current)?));
    }
    let anchor = open_directory_anchor(parent)?;
    let anchored_identity = file_identity_from_file(&anchor, parent)?;
    if components.last().map(|(_, identity)| *identity) != Some(anchored_identity) {
        return Err(conflict_error());
    }
    Ok(RecoveryParentBinding {
        components,
        parent: parent.to_path_buf(),
        _anchor: anchor,
    })
}

#[cfg(unix)]
fn open_directory_anchor(path: &Path) -> Result<std::fs::File, BackendError> {
    use std::os::unix::fs::OpenOptionsExt;
    #[cfg(target_os = "linux")]
    const O_DIRECTORY: i32 = 0x10000;
    #[cfg(target_os = "linux")]
    const O_NOFOLLOW: i32 = 0x20000;
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    const O_DIRECTORY: i32 = 0x100000;
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    const O_NOFOLLOW: i32 = 0x100;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(O_DIRECTORY | O_NOFOLLOW)
        .open(path)
        .map_err(|error| io_error(error, path))
}

#[cfg(windows)]
fn open_directory_anchor(path: &Path) -> Result<std::fs::File, BackendError> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    // Deliberately omit FILE_SHARE_DELETE: the validated parent cannot be
    // renamed or replaced until the mutation completes.
    std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|error| io_error(error, path))
}

#[cfg(not(any(unix, windows)))]
fn open_directory_anchor(path: &Path) -> Result<std::fs::File, BackendError> {
    std::fs::File::open(path).map_err(|error| io_error(error, path))
}

#[cfg(unix)]
fn bound_remove_file(binding: &RecoveryParentBinding, path: &Path) -> Result<(), std::io::Error> {
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    unsafe extern "C" {
        fn unlinkat(dirfd: i32, pathname: *const std::ffi::c_char, flags: i32) -> i32;
    }
    revalidate_recovery_parent(binding).map_err(|error| std::io::Error::other(error.message))?;
    let name = std::ffi::CString::new(path.file_name().unwrap().as_bytes())?;
    // SAFETY: dirfd is a retained open directory and name is a live NUL-terminated
    // single component. flags=0 removes only a non-directory entry.
    if unsafe { unlinkat(binding._anchor.as_raw_fd(), name.as_ptr(), 0) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(unix)]
fn bound_hard_link(
    binding: &RecoveryParentBinding,
    existing: &Path,
    new_path: &Path,
) -> Result<(), std::io::Error> {
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    unsafe extern "C" {
        fn linkat(
            olddirfd: i32,
            oldpath: *const std::ffi::c_char,
            newdirfd: i32,
            newpath: *const std::ffi::c_char,
            flags: i32,
        ) -> i32;
    }
    revalidate_recovery_parent(binding).map_err(|error| std::io::Error::other(error.message))?;
    let old = std::ffi::CString::new(existing.file_name().unwrap().as_bytes())?;
    let new = std::ffi::CString::new(new_path.file_name().unwrap().as_bytes())?;
    // SAFETY: both are single components relative to the same retained parent.
    if unsafe {
        linkat(
            binding._anchor.as_raw_fd(),
            old.as_ptr(),
            binding._anchor.as_raw_fd(),
            new.as_ptr(),
            0,
        )
    } == 0
    {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(unix))]
fn bound_hard_link(
    binding: &RecoveryParentBinding,
    existing: &Path,
    new_path: &Path,
) -> Result<(), std::io::Error> {
    revalidate_recovery_parent(binding).map_err(|error| std::io::Error::other(error.message))?;
    std::fs::hard_link(existing, new_path)
}

#[cfg(not(unix))]
fn bound_remove_file(binding: &RecoveryParentBinding, path: &Path) -> Result<(), std::io::Error> {
    revalidate_recovery_parent(binding).map_err(|error| std::io::Error::other(error.message))?;
    std::fs::remove_file(path)
}

fn bound_restore_bytes(
    binding: &RecoveryParentBinding,
    path: &Path,
    bytes: &[u8],
) -> Result<(), BackendError> {
    let temporary = write_synced_temp(&binding.parent, path, bytes)?;
    run_before_recovery_mutation_hook(path);
    revalidate_recovery_parent(binding)?;
    bound_replace_existing(binding, &temporary, path).map_err(|error| io_error(error, path))?;
    sync_parent(&binding.parent)
}

#[cfg(unix)]
fn bound_replace_existing(
    binding: &RecoveryParentBinding,
    temporary: &Path,
    path: &Path,
) -> Result<(), std::io::Error> {
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    unsafe extern "C" {
        fn renameat(
            olddirfd: i32,
            oldpath: *const std::ffi::c_char,
            newdirfd: i32,
            newpath: *const std::ffi::c_char,
        ) -> i32;
    }
    let old = std::ffi::CString::new(temporary.file_name().unwrap().as_bytes())?;
    let new = std::ffi::CString::new(path.file_name().unwrap().as_bytes())?;
    // SAFETY: both names are single live NUL-terminated components and both
    // dirfds refer to the same retained validated parent.
    if unsafe {
        renameat(
            binding._anchor.as_raw_fd(),
            old.as_ptr(),
            binding._anchor.as_raw_fd(),
            new.as_ptr(),
        )
    } == 0
    {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(unix))]
fn bound_replace_existing(
    _binding: &RecoveryParentBinding,
    temporary: &Path,
    path: &Path,
) -> Result<(), std::io::Error> {
    replace_existing(temporary, path)
}

#[cfg(unix)]
fn read_regular_nofollow(
    binding: &RecoveryParentBinding,
    path: &Path,
) -> Result<Vec<u8>, BackendError> {
    use std::io::Read;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    unsafe extern "C" {
        fn openat(dirfd: i32, pathname: *const std::ffi::c_char, flags: i32, mode: u32) -> i32;
    }
    const O_RDONLY: i32 = 0;
    #[cfg(target_os = "linux")]
    const O_NOFOLLOW: i32 = 0x20000;
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    const O_NOFOLLOW: i32 = 0x100;
    let name = std::ffi::CString::new(path.file_name().unwrap().as_bytes())
        .map_err(|_| staging_safe_io_error())?;
    // SAFETY: dirfd is a retained directory and name is one live NUL-terminated
    // component. On success ownership of the returned fd transfers to File.
    let fd = unsafe {
        openat(
            binding._anchor.as_raw_fd(),
            name.as_ptr(),
            O_RDONLY | O_NOFOLLOW,
            0,
        )
    };
    if fd < 0 {
        return Err(io_error(std::io::Error::last_os_error(), path));
    }
    // SAFETY: openat returned a new owned descriptor exactly once.
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    if !file
        .metadata()
        .map_err(|error| io_error(error, path))?
        .is_file()
    {
        return Err(staging_safe_io_error());
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| io_error(error, path))?;
    Ok(bytes)
}

#[cfg(windows)]
fn read_regular_nofollow(
    _binding: &RecoveryParentBinding,
    path: &Path,
) -> Result<Vec<u8>, BackendError> {
    use std::io::Read;
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|error| io_error(error, path))?;
    let metadata = file.metadata().map_err(|error| io_error(error, path))?;
    if !metadata.is_file() || transaction_is_reparse_point(&metadata) {
        return Err(staging_safe_io_error());
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| io_error(error, path))?;
    Ok(bytes)
}

#[cfg(not(any(unix, windows)))]
fn read_regular_nofollow(
    _binding: &RecoveryParentBinding,
    path: &Path,
) -> Result<Vec<u8>, BackendError> {
    std::fs::read(path).map_err(|error| io_error(error, path))
}

fn revalidate_recovery_parent(binding: &RecoveryParentBinding) -> Result<(), BackendError> {
    for (path, expected) in &binding.components {
        let metadata = std::fs::symlink_metadata(path).map_err(|_| conflict_error())?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || transaction_is_reparse_point(&metadata)
            || namespace_identity(path).map_err(|_| conflict_error())? != *expected
        {
            return Err(conflict_error());
        }
    }
    Ok(())
}

#[cfg(unix)]
fn namespace_identity(path: &Path) -> Result<FileIdentity, BackendError> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::symlink_metadata(path).map_err(|error| io_error(error, path))?;
    Ok(FileIdentity(metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
fn namespace_identity(path: &Path) -> Result<FileIdentity, BackendError> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2 | 0x4)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|error| io_error(error, path))?;
    file_identity_from_file(&file, path)
}

#[cfg(not(any(unix, windows)))]
fn namespace_identity(path: &Path) -> Result<FileIdentity, BackendError> {
    file_identity(path)
}

#[cfg(windows)]
fn transaction_is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn transaction_is_reparse_point(_metadata: &std::fs::Metadata) -> bool {
    false
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
    #[cfg(test)]
    if FAIL_NEXT_CANDIDATE_INSTALL.with(|flag| flag.replace(false)) {
        return Err(std::io::Error::other("injected candidate install failure"));
    }
    std::fs::rename(temporary, path)
}

#[cfg(windows)]
fn replace_existing(temporary: &Path, path: &Path) -> Result<(), std::io::Error> {
    #[cfg(test)]
    if FAIL_NEXT_CANDIDATE_INSTALL.with(|flag| flag.replace(false)) {
        return Err(std::io::Error::other("injected candidate install failure"));
    }
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, new_name: *const u16, flags: u32) -> i32;
    }
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    let existing: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
    let new_name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: both paths are valid NUL-terminated UTF-16 buffers for the duration
    // of the synchronous call. MoveFileExW with REPLACE_EXISTING performs the
    // same-volume namespace swap without an unlink/visibility gap; WRITE_THROUGH
    // asks Windows not to return before the move reaches durable storage.
    let result = unsafe {
        MoveFileExW(
            existing.as_ptr(),
            new_name.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> Result<(), BackendError> {
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| io_error(error, parent))
}

#[cfg(windows)]
fn sync_parent(parent: &Path) -> Result<(), BackendError> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            security: *mut std::ffi::c_void,
            creation: u32,
            flags: u32,
            template: *mut std::ffi::c_void,
        ) -> *mut std::ffi::c_void;
        fn FlushFileBuffers(handle: *mut std::ffi::c_void) -> i32;
        fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
    }
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const FILE_SHARE_ALL: u32 = 0x1 | 0x2 | 0x4;
    const OPEN_EXISTING: u32 = 3;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    let name: Vec<u16> = parent.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: `name` is a live NUL-terminated UTF-16 buffer. BACKUP_SEMANTICS is
    // required to obtain a directory handle. The handle is closed on every path.
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_ALL,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if handle as isize == -1 {
        return Err(io_error(std::io::Error::last_os_error(), parent));
    }
    // SAFETY: `handle` was returned by CreateFileW and remains owned here.
    let flushed = unsafe { FlushFileBuffers(handle) };
    let flush_error = (flushed == 0).then(std::io::Error::last_os_error);
    // SAFETY: closing the valid owned handle exactly once.
    unsafe {
        CloseHandle(handle);
    }
    flush_error.map_or(Ok(()), |error| Err(io_error(error, parent)))
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

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
struct FileIdentity(u64, u64);

fn journal_identity_matches(path: &Path, entry: &JournalEntry) -> Result<bool, BackendError> {
    let Some(expected) = entry.installed_identity else {
        return Ok(false);
    };
    file_identity(path).map(|actual| actual == expected)
}

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
        digest_bytes, set_before_checked_displace_hook, set_before_new_install_hook,
        set_before_recovery_mutation_hook, set_fail_next_candidate_install, set_fail_next_cleanup,
        set_fail_next_identity_query, FileTransaction, IMPORT_V2_COMMIT_CONFLICT,
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
            .contains(&path.to_string_lossy().replace('\\', "/")));
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
    fn restart_recovery_preserves_same_content_external_new_replacement() {
        let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("new.bin");
        let mut transaction = FileTransaction::new_for_project(&root);
        transaction.write_new(&path, b"same bytes").unwrap();
        transaction.simulate_process_crash();
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, b"same bytes").unwrap();

        let error = FileTransaction::reconcile_project(&root).unwrap_err();
        assert_eq!(error.code, IMPORT_V2_COMMIT_CONFLICT);
        assert_eq!(std::fs::read(&path).unwrap(), b"same bytes");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn restart_recovery_preserves_same_content_external_overwrite_replacement() {
        let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("existing.bin");
        std::fs::write(&path, b"before").unwrap();
        let mut transaction = FileTransaction::new_for_project(&root);
        transaction
            .write_if_hash_matches(&path, b"after", &digest_bytes(b"before"))
            .unwrap();
        transaction.simulate_process_crash();
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, b"after").unwrap();

        let error = FileTransaction::reconcile_project(&root).unwrap_err();
        assert_eq!(error.code, IMPORT_V2_COMMIT_CONFLICT);
        assert_eq!(std::fs::read(&path).unwrap(), b"after");
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
        transaction
            .write_if_hash_matches(&session, b"after", &expected)
            .unwrap();
        transaction.simulate_process_crash();

        assert_eq!(std::fs::read(&formal).unwrap(), b"formal");
        assert_eq!(std::fs::read(&session).unwrap(), b"after");
        FileTransaction::reconcile_project(&root).unwrap();
        assert!(!formal.exists());
        assert_eq!(std::fs::read(&session).unwrap(), b"before");
        assert_eq!(
            std::fs::read_dir(root.join(".app/import-v2-journal"))
                .unwrap()
                .count(),
            0
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn durable_journal_preserves_external_drift_during_reconciliation() {
        let root = std::env::temp_dir().join(format!("import-v2-journal-{}", uuid::Uuid::new_v4()));
        let target = root.join("wiki/new.md");
        let mut transaction = FileTransaction::new_for_project(&root);
        transaction.write_new(&target, b"committed").unwrap();
        transaction.simulate_process_crash();
        std::fs::write(&target, b"external").unwrap();

        let error = FileTransaction::reconcile_project(&root).unwrap_err();
        assert_eq!(error.code, IMPORT_V2_COMMIT_CONFLICT);
        assert_eq!(std::fs::read(&target).unwrap(), b"external");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn committed_marker_survives_crash_before_journal_delete_and_never_rolls_back() {
        let root = std::env::temp_dir().join(format!("import-v2-journal-{}", uuid::Uuid::new_v4()));
        let created = root.join("wiki/new.md");
        let existing = root.join(".app/import-sessions/s1/session.json");
        std::fs::create_dir_all(existing.parent().unwrap()).unwrap();
        std::fs::write(&existing, b"before").unwrap();
        let mut transaction = FileTransaction::new_for_project(&root);
        transaction.write_new(&created, b"created").unwrap();
        transaction
            .write_if_hash_matches(&existing, b"after", &digest_bytes(b"before"))
            .unwrap();
        transaction.mark_journal_committed().unwrap();
        transaction.simulate_process_crash();

        FileTransaction::reconcile_project(&root).unwrap();
        assert_eq!(std::fs::read(&created).unwrap(), b"created");
        assert_eq!(std::fs::read(&existing).unwrap(), b"after");
        assert_eq!(
            std::fs::read_dir(root.join(".app/import-v2-journal"))
                .unwrap()
                .count(),
            0
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn restart_recovery_rejects_parent_directory_swap_before_delete() {
        let root =
            std::env::temp_dir().join(format!("import-v2-parent-swap-{}", uuid::Uuid::new_v4()));
        let displaced = std::env::temp_dir().join(format!(
            "import-v2-parent-displaced-{}",
            uuid::Uuid::new_v4()
        ));
        let target = root.join("wiki/new.md");
        let mut transaction = FileTransaction::new_for_project(&root);
        transaction.write_new(&target, b"installed").unwrap();
        transaction.simulate_process_crash();

        let hook_root = root.clone();
        let hook_displaced = displaced.clone();
        let swapped = std::rc::Rc::new(std::cell::Cell::new(false));
        let hook_swapped = swapped.clone();
        set_before_recovery_mutation_hook(move |_| {
            if std::fs::rename(hook_root.join("wiki"), &hook_displaced).is_ok() {
                hook_swapped.set(true);
                std::fs::create_dir_all(hook_root.join("wiki")).unwrap();
                std::fs::write(hook_root.join("wiki/new.md"), b"outside replacement").unwrap();
            }
        });

        FileTransaction::reconcile_project(&root).unwrap();
        if swapped.get() {
            // Unix renameat/unlinkat remains bound to the displaced validated
            // parent and never touches the attacker replacement namespace.
            assert!(!displaced.join("new.md").exists());
            assert_eq!(std::fs::read(&target).unwrap(), b"outside replacement");
        } else {
            // Windows' retained no-FILE_SHARE_DELETE handle denies the swap;
            // rollback therefore removes the installed file in-place.
            assert!(!target.exists());
            assert!(!displaced.exists());
        }

        std::fs::remove_dir_all(root).unwrap();
        if displaced.exists() {
            std::fs::remove_dir_all(displaced).unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn recovery_rejects_symlinked_journal_directory_without_outside_deletion() {
        use std::os::unix::fs::symlink;
        let root =
            std::env::temp_dir().join(format!("import-v2-journal-link-{}", uuid::Uuid::new_v4()));
        let outside = std::env::temp_dir().join(format!(
            "import-v2-journal-outside-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(root.join(".app")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("keep.json"), b"outside").unwrap();
        symlink(&outside, root.join(".app/import-v2-journal")).unwrap();

        FileTransaction::reconcile_project(&root).unwrap_err();
        assert_eq!(
            std::fs::read(outside.join("keep.json")).unwrap(),
            b"outside"
        );
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(outside).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn recovery_rejects_symlinked_journal_file_without_read_or_delete() {
        use std::os::unix::fs::symlink;
        let root = std::env::temp_dir().join(format!(
            "import-v2-journal-file-link-{}",
            uuid::Uuid::new_v4()
        ));
        let outside = std::env::temp_dir().join(format!(
            "import-v2-journal-file-outside-{}",
            uuid::Uuid::new_v4()
        ));
        let journal = root.join(".app/import-v2-journal");
        std::fs::create_dir_all(&journal).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("keep.json"), b"outside").unwrap();
        symlink(outside.join("keep.json"), journal.join("evil.json")).unwrap();

        FileTransaction::reconcile_project(&root).unwrap_err();
        assert_eq!(
            std::fs::read(outside.join("keep.json")).unwrap(),
            b"outside"
        );
        assert!(journal.join("evil.json").exists());
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(outside).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn recovery_rejects_windows_journal_reparse_without_outside_deletion() {
        use std::os::windows::fs::symlink_dir;
        let root = std::env::temp_dir().join(format!(
            "import-v2-journal-reparse-{}",
            uuid::Uuid::new_v4()
        ));
        let outside = std::env::temp_dir().join(format!(
            "import-v2-journal-reparse-outside-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(root.join(".app")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("keep.json"), b"outside").unwrap();
        if let Err(error) = symlink_dir(&outside, root.join(".app/import-v2-journal")) {
            // Symlink creation legitimately requires Developer Mode or the
            // platform privilege; exercise this regression wherever supported.
            assert!(
                matches!(error.kind(), std::io::ErrorKind::PermissionDenied)
                    || error.raw_os_error() == Some(1314)
            );
            std::fs::remove_dir_all(root).unwrap();
            std::fs::remove_dir_all(outside).unwrap();
            return;
        }

        FileTransaction::reconcile_project(&root).unwrap_err();
        assert_eq!(
            std::fs::read(outside.join("keep.json")).unwrap(),
            b"outside"
        );
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(outside).unwrap();
    }
}
