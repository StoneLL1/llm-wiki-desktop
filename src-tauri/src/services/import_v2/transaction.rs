use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::errors::{
    BackendError, IMPORT_V2_CANCELLED, IMPORT_V2_COMMIT_CONFLICT, IMPORT_V2_COMMIT_FAILED,
};
use crate::utils::safe_project_dir::{
    BoundCreatedProjectDirectory, BoundFileIdentity as FileIdentity,
    BoundProjectMutationRoot as RecoveryParentBinding,
};

#[cfg(test)]
thread_local! {
    static BEFORE_CHECKED_DISPLACE_HOOK: std::cell::RefCell<Option<Box<dyn FnMut(&Path) -> bool>>> =
        std::cell::RefCell::new(None);
    static BEFORE_CHECKED_FINAL_MUTATION_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce(&Path)>>> =
        std::cell::RefCell::new(None);
    static BEFORE_ROLLBACK_FINAL_MUTATION_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce(&Path)>>> =
        std::cell::RefCell::new(None);
    static FAIL_NEXT_CANDIDATE_INSTALL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static BEFORE_NEW_INSTALL_HOOK: std::cell::RefCell<Option<Box<dyn FnMut(&Path) -> bool>>> = std::cell::RefCell::new(None);
    static FAIL_NEXT_CLEANUP: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);
    #[cfg(unix)]
    static FAIL_NEXT_JOURNAL_DELETE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static FAIL_NEXT_IDENTITY_QUERY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static BEFORE_RECOVERY_MUTATION_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce(&Path)>>> =
        std::cell::RefCell::new(None);
    static BEFORE_RECOVERY_FINAL_MUTATION_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce(&Path)>>> =
        std::cell::RefCell::new(None);
    static RECOVERY_PROCESS_DEATH_HOOK: std::cell::RefCell<Option<Box<dyn FnMut(&str) -> bool>>> =
        std::cell::RefCell::new(None);
    #[cfg(unix)]
    static AFTER_JOURNAL_DIRECTORY_BIND_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        std::cell::RefCell::new(None);
    static AFTER_INITIAL_JOURNAL_PUBLISH_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce(&Path)>>> =
        std::cell::RefCell::new(None);
    static SIMULATED_PROCESS_ABORT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(all(test, unix))]
fn set_fail_next_journal_delete() {
    FAIL_NEXT_JOURNAL_DELETE.with(|flag| flag.set(true));
}

#[cfg(test)]
fn set_recovery_process_death_hook(hook: impl FnMut(&str) -> bool + 'static) {
    RECOVERY_PROCESS_DEATH_HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

#[cfg(all(test, unix))]
fn set_after_journal_directory_bind_hook(hook: impl FnOnce() + 'static) {
    AFTER_JOURNAL_DIRECTORY_BIND_HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

fn run_after_journal_directory_bind_hook() {
    #[cfg(all(test, unix))]
    AFTER_JOURNAL_DIRECTORY_BIND_HOOK.with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook();
        }
    });
}

#[cfg(test)]
fn set_after_initial_journal_publish_hook(hook: impl FnOnce(&Path) + 'static) {
    AFTER_INITIAL_JOURNAL_PUBLISH_HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

fn run_after_initial_journal_publish_hook(path: &Path) {
    #[cfg(test)]
    AFTER_INITIAL_JOURNAL_PUBLISH_HOOK.with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook(path);
        }
    });
    #[cfg(not(test))]
    let _ = path;
}

fn recovery_fault_boundary(phase: &str) {
    #[cfg(test)]
    RECOVERY_PROCESS_DEATH_HOOK.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.as_mut().is_some_and(|hook| hook(phase)) {
            slot.take();
            panic!("simulated recovery process death at {phase}");
        }
    });
    #[cfg(not(test))]
    let _ = phase;
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
fn set_before_recovery_final_mutation_hook(hook: impl FnOnce(&Path) + 'static) {
    BEFORE_RECOVERY_FINAL_MUTATION_HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

fn run_before_recovery_final_mutation_hook(path: &Path) {
    #[cfg(test)]
    BEFORE_RECOVERY_FINAL_MUTATION_HOOK.with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook(path);
        }
    });
    #[cfg(not(test))]
    let _ = path;
}

#[cfg(test)]
fn set_before_checked_final_mutation_hook(hook: impl FnOnce(&Path) + 'static) {
    BEFORE_CHECKED_FINAL_MUTATION_HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

fn run_before_checked_final_mutation_hook(path: &Path) {
    #[cfg(test)]
    BEFORE_CHECKED_FINAL_MUTATION_HOOK.with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook(path);
        }
    });
    #[cfg(not(test))]
    let _ = path;
}

#[cfg(test)]
fn set_before_rollback_final_mutation_hook(hook: impl FnOnce(&Path) + 'static) {
    BEFORE_ROLLBACK_FINAL_MUTATION_HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

fn run_before_rollback_final_mutation_hook(path: &Path) {
    #[cfg(test)]
    BEFORE_ROLLBACK_FINAL_MUTATION_HOOK.with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook(path);
        }
    });
    #[cfg(not(test))]
    let _ = path;
}

#[cfg(test)]
pub(super) fn set_fail_next_candidate_install() {
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
    desired_absent: bool,
    #[serde(default)]
    installed_identity: Option<FileIdentity>,
    #[serde(default)]
    recovery: Option<RecoveryRecord>,
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
enum RecoveryAction {
    DeleteInstalled,
    RestorePrevious,
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
enum RecoveryPhase {
    Planned,
    Quarantined,
    Restored,
}

#[derive(Clone, Deserialize, Serialize)]
struct RecoveryRecord {
    canonical_relative_path: String,
    guard_relative_path: String,
    expected_hash: String,
    expected_identity: FileIdentity,
    action: RecoveryAction,
    phase: RecoveryPhase,
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

/// A checked replacement prepared before the cohort journal is made durable.
/// Keeping this small record lets large import cohorts write one complete
/// recovery journal instead of repeatedly serializing a growing JSON array.
struct PendingCheckedReplacement {
    parent_binding: Arc<RecoveryParentBinding>,
    path: PathBuf,
    temporary: PathBuf,
    guard: PathBuf,
    previous: Vec<u8>,
    expected_identity: FileIdentity,
    candidate_identity: FileIdentity,
    desired: Vec<u8>,
}

struct LiveBackup {
    binding: Arc<RecoveryParentBinding>,
    path: PathBuf,
    previous: Option<Vec<u8>>,
}

struct LiveRecoveryArtifact {
    binding: Arc<RecoveryParentBinding>,
    path: PathBuf,
}

pub struct FileTransaction {
    backups: Vec<LiveBackup>,
    created_dirs: Vec<BoundCreatedProjectDirectory>,
    recovery_artifacts: Vec<LiveRecoveryArtifact>,
    retained_mutation_parents: std::collections::HashMap<PathBuf, Arc<RecoveryParentBinding>>,
    installed_ownership: std::collections::HashMap<PathBuf, InstalledOwnership>,
    unverified_installs: std::collections::HashSet<PathBuf>,
    guard_by_destination: std::collections::HashMap<PathBuf, PathBuf>,
    deleted_destinations: std::collections::HashSet<PathBuf>,
    project_root: Option<PathBuf>,
    journal_path: Option<PathBuf>,
    journal_entries: Vec<JournalEntry>,
    journal_artifacts: Vec<String>,
    #[cfg(test)]
    cohort_pin_parent_syncs: usize,
    finished: bool,
}

pub(super) fn read_project_file_nofollow(
    root: &Path,
    path: &Path,
) -> Result<Vec<u8>, BackendError> {
    let binding = bind_recovery_parent(root, path)?;
    let bytes = read_regular_nofollow(&binding, path)?;
    revalidate_recovery_parent(&binding)?;
    Ok(bytes)
}

pub(super) fn is_project_reparse_point(metadata: &std::fs::Metadata) -> bool {
    transaction_is_reparse_point(metadata)
}

impl FileTransaction {
    #[cfg(test)]
    fn simulate_process_crash(mut self) {
        // A real process death closes ownership anchors but skips Drop/rollback.
        self.installed_ownership.clear();
        self.backups.clear();
        self.created_dirs.clear();
        self.recovery_artifacts.clear();
        self.retained_mutation_parents.clear();
        std::mem::forget(self);
    }
    pub fn new() -> Self {
        Self {
            backups: Vec::new(),
            created_dirs: Vec::new(),
            recovery_artifacts: Vec::new(),
            retained_mutation_parents: std::collections::HashMap::new(),
            installed_ownership: std::collections::HashMap::new(),
            unverified_installs: std::collections::HashSet::new(),
            guard_by_destination: std::collections::HashMap::new(),
            deleted_destinations: std::collections::HashSet::new(),
            project_root: None,
            journal_path: None,
            journal_entries: Vec::new(),
            journal_artifacts: Vec::new(),
            #[cfg(test)]
            cohort_pin_parent_syncs: 0,
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
        // deleting entries. Every journal read and mutation is resolved through
        // this capability even if the lexical directory name is replaced.
        let journal_binding = bind_recovery_parent(root, &directory.join("entry"))?;
        run_after_journal_directory_bind_hook();
        let entries = journal_binding
            .read_entry_names()
            .map_err(|error| io_error(error, &directory))?
            .into_iter()
            .map(|name| directory.join(name))
            .collect::<Vec<_>>();
        for path in entries {
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let mut journal: Journal =
                serde_json::from_slice(&read_regular_nofollow(&journal_binding, &path)?)
                    .map_err(|_| staging_safe_io_error())?;
            validate_journal_recovery_artifacts(root, &journal)?;
            let indices: Vec<usize> = if journal.state == JournalState::Committed {
                (0..journal.entries.len()).collect()
            } else {
                (0..journal.entries.len()).rev().collect()
            };
            for index in indices {
                let intent = &journal.entries[index];
                let target = safe_journal_target(root, &intent.relative_path)?;
                let parent_binding = bind_recovery_parent(root, &target)?;
                let current = match parent_binding.read_regular(&target) {
                    Ok(bytes) => Some(bytes),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => return Err(io_error(error, &target)),
                };
                let current_hash = current.as_deref().map(digest_bytes);
                if journal.state == JournalState::Committed {
                    let desired_is_present = current_hash.as_deref() == Some(&intent.desired_hash)
                        && journal_identity_matches(
                            root,
                            &journal,
                            &parent_binding,
                            &target,
                            intent,
                            false,
                        )?;
                    if (intent.desired_absent && current.is_some())
                        || (!intent.desired_absent && !desired_is_present)
                    {
                        return Err(conflict_error());
                    }
                    continue;
                }
                let previous_hash = intent.previous.as_deref().map(digest_bytes);
                if intent.desired_absent {
                    if current_hash == previous_hash {
                        continue;
                    }
                    if current.is_some() {
                        return Err(conflict_error());
                    }
                    let previous = intent
                        .previous
                        .as_deref()
                        .ok_or_else(staging_safe_io_error)?;
                    let temporary = write_synced_temp(&parent_binding, &target, previous)?;
                    install_candidate(&parent_binding, &temporary, &target)
                        .map_err(|error| io_error(error, &target))?;
                    parent_binding
                        .sync()
                        .map_err(|error| io_error(error, parent_binding.parent()))?;
                    let _ = bound_remove_file(&parent_binding, &temporary);
                    continue;
                }
                if intent.recovery.is_some() || current_hash != previous_hash {
                    if intent.recovery.is_some() {
                        if !journal_candidate_pin_matches(root, &journal, intent)? {
                            return Err(conflict_error());
                        }
                        resume_recovery_entry(
                            root,
                            &journal_binding,
                            &path,
                            &mut journal,
                            index,
                            &parent_binding,
                            &target,
                        )?;
                        continue;
                    }
                    if current_hash.as_deref() != Some(&intent.desired_hash)
                        || !journal_identity_matches(
                            root,
                            &journal,
                            &parent_binding,
                            &target,
                            intent,
                            true,
                        )?
                    {
                        return Err(conflict_error());
                    }
                    plan_and_recover_entry(
                        root,
                        &journal_binding,
                        &path,
                        &mut journal,
                        index,
                        &parent_binding,
                        &target,
                    )?;
                }
            }
            for relative in &journal.recovery_artifacts {
                let artifact = safe_journal_target(root, relative)?;
                let parent_binding = bind_recovery_parent(root, &artifact)?;
                run_before_recovery_mutation_hook(&artifact);
                match bound_remove_file(&parent_binding, &artifact) {
                    Ok(()) => {
                        parent_binding
                            .sync()
                            .map_err(|error| io_error(error, parent_binding.parent()))?;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(io_error(error, &artifact)),
                }
            }
            bound_remove_file(&journal_binding, &path).map_err(|error| io_error(error, &path))?;
            journal_binding
                .sync()
                .map_err(|error| io_error(error, journal_binding.parent()))?;
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
        let relative = project_relative_path(root, path)
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
            desired_absent: false,
            // The same-volume install primitives below preserve the candidate's
            // native identity. Persist it before the namespace mutation so a
            // crash immediately after install is still recoverable.
            installed_identity: Some(candidate_identity),
            recovery: None,
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
        let bytes = serde_json::to_vec(&Journal {
            state: JournalState::InProgress,
            entries: self.journal_entries.clone(),
            recovery_artifacts: self.journal_artifacts.clone(),
        })
        .map_err(|_| staging_safe_io_error())?;
        write_atomic_bytes(root, &journal_path, &bytes)?;
        run_after_initial_journal_publish_hook(&journal_path);
        Ok(())
    }

    fn record_delete_intent(&mut self, path: &Path, previous: Vec<u8>) -> Result<(), BackendError> {
        let Some(root) = self.project_root.as_deref() else {
            return Ok(());
        };
        let relative = project_relative_path(root, path)
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
            relative_path: relative,
            previous: Some(previous),
            desired_hash: digest_bytes(&[]),
            desired_absent: true,
            installed_identity: None,
            recovery: None,
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
        let bytes = serde_json::to_vec(&Journal {
            state: JournalState::InProgress,
            entries: self.journal_entries.clone(),
            recovery_artifacts: self.journal_artifacts.clone(),
        })
        .map_err(|_| staging_safe_io_error())?;
        write_atomic_bytes(root, &journal_path, &bytes)?;
        run_after_initial_journal_publish_hook(&journal_path);
        Ok(())
    }

    fn stage_recovery_artifact(&mut self, path: &Path) -> Result<(), BackendError> {
        let Some(root) = self.project_root.as_deref() else {
            return Ok(());
        };
        self.journal_artifacts.push(
            project_relative_path(root, path)
                .map_err(|_| staging_safe_io_error())?
                .to_string_lossy()
                .replace('\\', "/"),
        );
        Ok(())
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
        let root = self
            .project_root
            .as_deref()
            .ok_or_else(staging_safe_io_error)?;
        write_atomic_bytes(root, path, &bytes)
    }

    fn finish_journal(&mut self) -> Result<(), BackendError> {
        if let Some(path) = self.journal_path.take() {
            #[cfg(all(test, unix))]
            if FAIL_NEXT_JOURNAL_DELETE.with(|flag| flag.replace(false)) {
                return Err(io_error(
                    std::io::Error::other("injected journal delete failure"),
                    &path,
                ));
            }
            let root = self
                .project_root
                .as_deref()
                .ok_or_else(staging_safe_io_error)?;
            let binding = bind_recovery_parent(root, &path)?;
            match bound_remove_file(&binding, &path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(io_error(error, &path)),
            }
        }
        Ok(())
    }

    pub fn write_new(&mut self, path: &Path, bytes: &[u8]) -> Result<(), BackendError> {
        let parent_binding = self.ensure_and_bind_mutation_parent(path)?;
        let temporary = write_synced_temp(&parent_binding, path, bytes)?;
        self.track_artifact(&parent_binding, &temporary)?;
        let candidate_identity = bound_file_identity(&parent_binding, &temporary)?;
        let candidate_pin = self.pin_candidate_identity(&parent_binding, &temporary)?;
        self.sync_candidate_pin_parent(&parent_binding, candidate_pin.as_deref())?;
        self.stage_recovery_artifact(&temporary)?;
        if let Some(pin) = candidate_pin.as_deref() {
            self.stage_recovery_artifact(pin)?;
        }
        self.record_intent(path, None, bytes, candidate_identity)?;
        #[cfg(test)]
        if let Some(entry) = self.journal_entries.last() {
            commit_fault_boundary("intent", Some(entry.relative_path.as_str()));
        }
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
            let mut cleanup_failures = Vec::new();
            for artifact in [Some(temporary.as_path()), candidate_pin.as_deref()]
                .into_iter()
                .flatten()
            {
                if let Err(cleanup) = self.cleanup_artifact(artifact) {
                    cleanup_failures.push(cleanup.message);
                }
            }
            if cleanup_failures.is_empty() {
                return Err(io_error(error, path));
            }
            cleanup_failures.insert(0, "candidate install failed".to_string());
            return Err(rollback_failure(
                "New-file install failed and recovery artifact cleanup failed.",
                cleanup_failures,
            ));
        }
        self.backups.push(LiveBackup {
            binding: Arc::clone(&parent_binding),
            path: path.to_path_buf(),
            previous: None,
        });
        self.capture_installed_expected(&parent_binding, path, bytes, candidate_identity)?;
        parent_binding
            .sync()
            .map_err(|error| io_error(error, parent_binding.parent()))?;
        #[cfg(test)]
        if let Some(entry) = self.journal_entries.last() {
            commit_fault_boundary("installed", Some(entry.relative_path.as_str()));
        }
        // Keep the prepared artifact tracked until commit even though a
        // successful no-replace rename consumes its name. Recovery treats the
        // now-absent artifact as complete, while an interrupted or failed
        // publish still leaves an exact path that cleanup can retry.
        Ok(())
    }

    fn ensure_and_bind_mutation_parent(
        &mut self,
        path: &Path,
    ) -> Result<Arc<RecoveryParentBinding>, BackendError> {
        let parent = path.parent().ok_or_else(staging_safe_io_error)?;
        if self.retained_mutation_parents.contains_key(parent) {
            return self.bind_mutation_parent(path);
        }
        let fallback_root;
        let root = if let Some(root) = self.project_root.as_deref() {
            root
        } else {
            fallback_root = path
                .ancestors()
                .find(|candidate| candidate.is_dir())
                .ok_or_else(staging_safe_io_error)?
                .to_path_buf();
            &fallback_root
        };
        let (binding, created) = RecoveryParentBinding::ensure_and_bind(root, path)
            .map_err(|error| io_error(error, path))?;
        self.created_dirs.extend(created);
        self.retain_mutation_parent(path, binding)
    }

    fn bind_mutation_parent(
        &mut self,
        path: &Path,
    ) -> Result<Arc<RecoveryParentBinding>, BackendError> {
        let parent = path.parent().ok_or_else(staging_safe_io_error)?;
        let binding = if let Some(root) = self.project_root.as_deref() {
            bind_recovery_parent(root, path)?
        } else {
            RecoveryParentBinding::bind(parent, path).map_err(|error| io_error(error, path))?
        };
        self.retain_mutation_parent(path, binding)
    }

    fn retain_mutation_parent(
        &mut self,
        path: &Path,
        binding: RecoveryParentBinding,
    ) -> Result<Arc<RecoveryParentBinding>, BackendError> {
        let key = binding.parent().to_path_buf();
        if let Some(retained) = self.retained_mutation_parents.get(&key) {
            if !retained
                .has_same_directory_identity(&binding)
                .map_err(|error| io_error(error, path))?
            {
                return Err(conflict_error());
            }
            return Ok(Arc::clone(retained));
        }
        let binding = Arc::new(binding);
        self.retained_mutation_parents
            .insert(key, Arc::clone(&binding));
        Ok(binding)
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
        let binding = if let Some(artifact) = self
            .recovery_artifacts
            .iter()
            .find(|artifact| artifact.path == path)
        {
            Arc::clone(&artifact.binding)
        } else {
            self.bind_mutation_parent(path)?
        };
        match bound_remove_file(&binding, path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error(error, path)),
        }
        self.recovery_artifacts
            .retain(|candidate| candidate.path != path);
        Ok(())
    }

    fn track_artifact(
        &mut self,
        binding: &Arc<RecoveryParentBinding>,
        path: &Path,
    ) -> Result<(), BackendError> {
        self.recovery_artifacts.push(LiveRecoveryArtifact {
            binding: Arc::clone(binding),
            path: path.to_path_buf(),
        });
        Ok(())
    }

    fn pin_candidate_identity(
        &mut self,
        binding: &Arc<RecoveryParentBinding>,
        temporary: &Path,
    ) -> Result<Option<PathBuf>, BackendError> {
        #[cfg(unix)]
        {
            let pin = binding
                .parent()
                .join(format!(".wiki-guard-installed-{}", uuid::Uuid::new_v4()));
            if let Err(error) = bound_hard_link(binding, temporary, &pin) {
                return Err(hard_link_identity_proof_error(error, &pin));
            }
            if let Err(error) = self.track_artifact(binding, &pin) {
                let _ = bound_remove_file(binding, &pin);
                return Err(error);
            }
            Ok(Some(pin))
        }
        #[cfg(not(unix))]
        {
            let _ = (binding, temporary);
            Ok(None)
        }
    }

    fn sync_candidate_pin_parent(
        &mut self,
        binding: &RecoveryParentBinding,
        pin: Option<&Path>,
    ) -> Result<(), BackendError> {
        if pin.is_none() {
            return Ok(());
        }
        if let Err(error) = binding.sync() {
            let primary = io_error(error, binding.parent());
            if let Some(pin) = pin {
                let _ = self.cleanup_artifact(pin);
            }
            return Err(primary);
        }
        Ok(())
    }

    pub fn write_if_hash_matches(
        &mut self,
        path: &Path,
        bytes: &[u8],
        expected_hash: &str,
    ) -> Result<(), BackendError> {
        path.parent().ok_or_else(|| {
            BackendError::new(
                "PATH_INVALID",
                "Cannot determine parent directory.",
                false,
                true,
            )
        })?;
        let parent_binding = self.bind_mutation_parent(path)?;
        let temporary = write_synced_temp(&parent_binding, path, bytes)?;
        let guard = parent_binding
            .parent()
            .join(format!(".wiki-guard-{}", uuid::Uuid::new_v4()));
        self.track_artifact(&parent_binding, &temporary)?;
        run_before_checked_displace_hook(path);
        if let Err(error) = bound_hard_link(&parent_binding, path, &guard) {
            let _ = self.cleanup_artifact(&temporary);
            return Err(hard_link_identity_proof_error(error, path));
        }
        self.track_artifact(&parent_binding, &guard)?;
        self.guard_by_destination
            .insert(path.to_path_buf(), guard.clone());
        let before_identity = bound_file_identity(&parent_binding, path)?;
        let guard_identity = bound_file_identity(&parent_binding, &guard)?;
        let previous_before = match parent_binding.read_regular(&guard) {
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
        let candidate_identity = bound_file_identity(&parent_binding, &temporary)?;
        let candidate_pin = self.pin_candidate_identity(&parent_binding, &temporary)?;
        self.sync_candidate_pin_parent(&parent_binding, candidate_pin.as_deref())?;
        self.stage_recovery_artifact(&temporary)?;
        self.stage_recovery_artifact(&guard)?;
        if let Some(pin) = candidate_pin.as_deref() {
            self.stage_recovery_artifact(pin)?;
        }
        self.record_intent(
            path,
            Some(previous_before.clone()),
            bytes,
            candidate_identity,
        )?;
        #[cfg(test)]
        if let Some(entry) = self.journal_entries.last() {
            commit_fault_boundary("intent", Some(entry.relative_path.as_str()));
        }
        run_before_checked_final_mutation_hook(path);
        #[cfg(test)]
        if FAIL_NEXT_CANDIDATE_INSTALL.with(|flag| flag.replace(false)) {
            let _ = self.cleanup_artifact(&temporary);
            let _ = self.cleanup_artifact(&guard);
            if let Some(pin) = candidate_pin.as_deref() {
                let _ = self.cleanup_artifact(pin);
            }
            return Err(io_error(
                std::io::Error::other("injected candidate install failure"),
                path,
            ));
        }
        if let Err(error) = parent_binding.replace_existing_if_identity_and_hash(
            &temporary,
            path,
            before_identity,
            expected_hash,
        ) {
            let _ = self.cleanup_artifact(&temporary);
            let _ = self.cleanup_artifact(&guard);
            if let Some(pin) = candidate_pin.as_deref() {
                let _ = self.cleanup_artifact(pin);
            }
            return Err(io_error(error, path));
        }
        self.backups.push(LiveBackup {
            binding: Arc::clone(&parent_binding),
            path: path.to_path_buf(),
            previous: Some(previous_before.clone()),
        });
        self.capture_installed_expected(&parent_binding, path, bytes, candidate_identity)?;
        #[cfg(test)]
        if let Some(entry) = self.journal_entries.last() {
            commit_fault_boundary("installed", Some(entry.relative_path.as_str()));
        }
        self.recovery_artifacts
            .retain(|candidate| candidate.path != temporary);
        let previous = read_regular_nofollow(&parent_binding, &guard)?;
        if format!("{:x}", Sha256::digest(&previous)) != expected_hash {
            bound_replace_existing(&parent_binding, &guard, path)
                .map_err(|error| io_error(error, path))?;
            self.recovery_artifacts
                .retain(|candidate| candidate.path != guard);
            parent_binding
                .sync()
                .map_err(|error| io_error(error, parent_binding.parent()))?;
            return Err(conflict_error());
        }
        parent_binding
            .sync()
            .map_err(|error| io_error(error, parent_binding.parent()))?;
        self.cleanup_artifact(&temporary)?;
        self.cleanup_artifact(&guard)
    }

    /// Apply a cohort of checked replacements with a single durable intent
    /// journal.  Every target is verified before the journal is written and
    /// checked again immediately before installation.  A crash therefore
    /// still rolls the whole cohort back, while journal serialization remains
    /// linear for 10,000-item import batches.
    pub fn write_many_if_hash_matches(
        &mut self,
        writes: &[(PathBuf, Vec<u8>, String)],
    ) -> Result<(), BackendError> {
        self.write_many_if_hash_matches_with_cancel(writes, || false)
    }

    /// Cancellable form used by large import cohort claims. Cancellation is
    /// checked only at transaction-safe boundaries; `Drop` rolls back any
    /// prepared or installed replacements before the error escapes.
    pub fn write_many_if_hash_matches_with_cancel<F>(
        &mut self,
        writes: &[(PathBuf, Vec<u8>, String)],
        mut should_cancel: F,
    ) -> Result<(), BackendError>
    where
        F: FnMut() -> bool,
    {
        if writes.is_empty() {
            return Ok(());
        }
        if self.project_root.is_none() {
            return Err(staging_safe_io_error());
        }
        let mut pending = Vec::with_capacity(writes.len());
        for (path, desired, expected_hash) in writes {
            if should_cancel() {
                return Err(transaction_cancelled_error());
            }
            let parent_binding = self.bind_mutation_parent(path)?;
            let temporary = write_synced_temp(&parent_binding, path, desired)?;
            let guard = parent_binding
                .parent()
                .join(format!(".wiki-guard-{}", uuid::Uuid::new_v4()));
            self.track_artifact(&parent_binding, &temporary)?;
            run_before_checked_displace_hook(path);
            if let Err(error) = bound_hard_link(&parent_binding, path, &guard) {
                let _ = self.cleanup_artifact(&temporary);
                return Err(hard_link_identity_proof_error(error, path));
            }
            self.track_artifact(&parent_binding, &guard)?;
            self.guard_by_destination
                .insert(path.clone(), guard.clone());
            let expected_identity = bound_file_identity(&parent_binding, path)?;
            let guard_identity = bound_file_identity(&parent_binding, &guard)?;
            let previous = read_regular_nofollow(&parent_binding, &guard)?;
            if expected_identity != guard_identity || digest_bytes(&previous) != *expected_hash {
                return Err(conflict_error());
            }
            let candidate_identity = bound_file_identity(&parent_binding, &temporary)?;
            self.pin_candidate_identity(&parent_binding, &temporary)?;
            pending.push(PendingCheckedReplacement {
                parent_binding,
                path: path.clone(),
                temporary,
                guard,
                previous,
                expected_identity,
                candidate_identity,
                desired: desired.clone(),
            });
        }
        #[cfg(unix)]
        {
            let mut synced_pin_parents = HashSet::new();
            for replacement in &pending {
                if synced_pin_parents.insert(replacement.parent_binding.parent().to_path_buf()) {
                    replacement
                        .parent_binding
                        .sync()
                        .map_err(|error| io_error(error, replacement.parent_binding.parent()))?;
                    #[cfg(test)]
                    {
                        self.cohort_pin_parent_syncs += 1;
                    }
                }
            }
        }
        if should_cancel() {
            return Err(transaction_cancelled_error());
        }
        self.install_cohort_journal(&pending)?;
        for replacement in pending {
            if should_cancel() {
                return Err(transaction_cancelled_error());
            }
            if bound_file_identity(&replacement.parent_binding, &replacement.path)?
                != replacement.expected_identity
            {
                return Err(conflict_error());
            }
            run_before_checked_final_mutation_hook(&replacement.path);
            replacement
                .parent_binding
                .replace_existing_if_identity_and_hash(
                    &replacement.temporary,
                    &replacement.path,
                    replacement.expected_identity,
                    &digest_bytes(&replacement.previous),
                )
                .map_err(|error| io_error(error, &replacement.path))?;
            self.backups.push(LiveBackup {
                binding: Arc::clone(&replacement.parent_binding),
                path: replacement.path.clone(),
                previous: Some(replacement.previous),
            });
            self.capture_cohort_install(
                &replacement.parent_binding,
                &replacement.path,
                &replacement.desired,
                replacement.candidate_identity,
            )?;
            replacement
                .parent_binding
                .sync()
                .map_err(|error| io_error(error, replacement.parent_binding.parent()))?;
            bound_remove_file(&replacement.parent_binding, &replacement.guard)
                .map_err(|error| io_error(error, &replacement.guard))?;
        }
        if should_cancel() {
            return Err(transaction_cancelled_error());
        }
        // The journal retains all artifact names until commit, but the live
        // vector need not remove them one-by-one (which is quadratic at
        // 10,000 items). If an earlier step failed, this line is not reached
        // and rollback still owns the complete artifact set.
        self.recovery_artifacts
            .retain(|artifact| is_candidate_identity_pin(&artifact.path));
        Ok(())
    }

    #[cfg(test)]
    fn cohort_pin_parent_sync_count(&self) -> usize {
        self.cohort_pin_parent_syncs
    }

    #[cfg(test)]
    fn retained_mutation_parent_count(&self) -> usize {
        self.retained_mutation_parents.len()
    }

    #[cfg(test)]
    fn retained_installed_anchor_count(&self) -> usize {
        self.installed_ownership
            .values()
            .filter(|ownership| ownership._anchor.is_some())
            .count()
    }

    pub fn delete_if_hash_matches(
        &mut self,
        path: &Path,
        expected_hash: &str,
    ) -> Result<(), BackendError> {
        path.parent().ok_or_else(|| {
            BackendError::new(
                "PATH_INVALID",
                "Cannot determine parent directory.",
                false,
                true,
            )
        })?;
        let guard = path
            .parent()
            .unwrap()
            .join(format!(".wiki-delete-guard-{}", uuid::Uuid::new_v4()));
        let parent_binding = self.bind_mutation_parent(path)?;
        run_before_checked_displace_hook(path);
        bound_hard_link(&parent_binding, path, &guard)
            .map_err(|error| hard_link_identity_proof_error(error, path))?;
        self.track_artifact(&parent_binding, &guard)?;
        let before_identity = bound_file_identity(&parent_binding, path)?;
        let guard_identity = bound_file_identity(&parent_binding, &guard)?;
        let previous = read_regular_nofollow(&parent_binding, &guard)?;
        if before_identity != guard_identity || digest_bytes(&previous) != expected_hash {
            let _ = self.cleanup_artifact(&guard);
            return Err(conflict_error());
        }
        self.stage_recovery_artifact(&guard)?;
        self.record_delete_intent(path, previous.clone())?;
        #[cfg(test)]
        if let Some(entry) = self.journal_entries.last() {
            commit_fault_boundary("intent", Some(entry.relative_path.as_str()));
        }
        if bound_file_identity(&parent_binding, path)? != before_identity {
            let _ = self.cleanup_artifact(&guard);
            return Err(conflict_error());
        }
        run_before_checked_final_mutation_hook(path);
        parent_binding
            .remove_file_if_identity_and_hash(path, before_identity, expected_hash)
            .map_err(|error| io_error(error, path))?;
        self.backups.push(LiveBackup {
            binding: Arc::clone(&parent_binding),
            path: path.to_path_buf(),
            previous: Some(previous),
        });
        self.deleted_destinations.insert(path.to_path_buf());
        parent_binding
            .sync()
            .map_err(|error| io_error(error, parent_binding.parent()))?;
        #[cfg(test)]
        if let Some(entry) = self.journal_entries.last() {
            commit_fault_boundary("installed", Some(entry.relative_path.as_str()));
        }
        self.cleanup_artifact(&guard)
    }

    pub fn commit(mut self) -> Result<(), BackendError> {
        if let Err(error) = self.verify_live_installed_ownership() {
            return Err(self.rollback_after(error));
        }
        let has_durable_journal = self.journal_path.is_some();
        let artifacts = self
            .recovery_artifacts
            .iter()
            .filter(|artifact| !has_durable_journal || !is_candidate_identity_pin(&artifact.path))
            .map(|artifact| artifact.path.clone())
            .collect::<Vec<_>>();
        for artifact in artifacts {
            if let Err(error) = self.cleanup_artifact(&artifact) {
                return Err(self.rollback_after(error));
            }
        }
        if let Err(error) = self.mark_journal_committed() {
            return Err(self.rollback_after(error));
        }
        if has_durable_journal {
            // A durable committed marker is the point of no return. Candidate
            // identity pins must survive until this marker reaches disk, and a
            // later cleanup failure must never make Drop roll back committed
            // project bytes. The committed journal remains available for the
            // next reconciliation attempt when cleanup cannot finish here.
            self.finished = true;
        }
        #[cfg(test)]
        commit_fault_boundary("committed", None);
        if has_durable_journal {
            let identity_pins = self
                .recovery_artifacts
                .iter()
                .filter(|artifact| is_candidate_identity_pin(&artifact.path))
                .map(|artifact| artifact.path.clone())
                .collect::<Vec<_>>();
            for pin in identity_pins {
                if self.cleanup_artifact(&pin).is_err() {
                    // The durable committed journal remains the source of
                    // truth. Reconciliation can retry cleanup; callers must
                    // not treat already-committed project bytes as rolled back.
                    return Ok(());
                }
            }
        }
        if self.finish_journal().is_err() && has_durable_journal {
            return Ok(());
        }
        #[cfg(test)]
        commit_fault_boundary("deleted", None);
        self.finished = true;
        Ok(())
    }

    fn install_cohort_journal(
        &mut self,
        pending: &[PendingCheckedReplacement],
    ) -> Result<(), BackendError> {
        let root = self
            .project_root
            .as_deref()
            .ok_or_else(staging_safe_io_error)?;
        self.journal_entries = pending
            .iter()
            .map(|replacement| {
                let relative = project_relative_path(root, &replacement.path)
                    .map_err(|_| staging_safe_io_error())?
                    .to_string_lossy()
                    .replace('\\', "/");
                Ok(JournalEntry {
                    relative_path: relative,
                    previous: Some(replacement.previous.clone()),
                    desired_hash: digest_bytes(&replacement.desired),
                    desired_absent: false,
                    installed_identity: Some(replacement.candidate_identity),
                    recovery: None,
                })
            })
            .collect::<Result<Vec<_>, BackendError>>()?;
        self.journal_artifacts = self
            .recovery_artifacts
            .iter()
            .map(|artifact| {
                project_relative_path(root, &artifact.path)
                    .map_err(|_| staging_safe_io_error())
                    .map(|relative| relative.to_string_lossy().replace('\\', "/"))
            })
            .collect::<Result<Vec<_>, BackendError>>()?;
        let journal_path = self
            .journal_path
            .get_or_insert_with(|| {
                root.join(format!(
                    ".app/import-v2-journal/{}.json",
                    uuid::Uuid::new_v4()
                ))
            })
            .clone();
        self.write_journal_in_progress(&journal_path)
    }

    fn write_journal_in_progress(&self, journal_path: &Path) -> Result<(), BackendError> {
        let bytes = serde_json::to_vec(&Journal {
            state: JournalState::InProgress,
            entries: self.journal_entries.clone(),
            recovery_artifacts: self.journal_artifacts.clone(),
        })
        .map_err(|_| staging_safe_io_error())?;
        let root = self
            .project_root
            .as_deref()
            .ok_or_else(staging_safe_io_error)?;
        write_atomic_bytes(root, journal_path, &bytes)
    }

    fn capture_cohort_install(
        &mut self,
        binding: &Arc<RecoveryParentBinding>,
        path: &Path,
        bytes: &[u8],
        expected_identity: FileIdentity,
    ) -> Result<(), BackendError> {
        self.unverified_installs.insert(path.to_path_buf());
        #[cfg(unix)]
        let (identity, anchor) = (bound_file_identity(binding, path)?, None);
        #[cfg(not(unix))]
        let (identity, anchor) = {
            let anchor = binding
                .open_regular_pinned(path)
                .map_err(|error| io_error(error, path))?;
            let identity = file_identity_from_file(&anchor, path)?;
            (identity, Some(anchor))
        };
        if identity != expected_identity {
            return Err(conflict_error());
        }
        self.installed_ownership.insert(
            path.to_path_buf(),
            InstalledOwnership {
                identity,
                hash: digest_bytes(bytes),
                binding: Arc::clone(binding),
                _anchor: anchor,
            },
        );
        self.unverified_installs.remove(path);
        Ok(())
    }

    fn capture_installed_expected(
        &mut self,
        binding: &Arc<RecoveryParentBinding>,
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
        #[cfg(unix)]
        let (identity, anchor) = (bound_file_identity(binding, path)?, None);
        #[cfg(not(unix))]
        let (identity, anchor) = {
            let anchor = binding
                .open_regular_pinned(path)
                .map_err(|error| io_error(error, path))?;
            let identity = file_identity_from_file(&anchor, path)?;
            (identity, Some(anchor))
        };
        if identity != expected_identity {
            return Err(conflict_error());
        }
        self.installed_ownership.insert(
            path.to_path_buf(),
            InstalledOwnership {
                identity,
                hash: digest_bytes(bytes),
                binding: Arc::clone(binding),
                _anchor: anchor,
            },
        );
        if let (Some(root), Some(journal_path)) =
            (self.project_root.as_deref(), self.journal_path.as_deref())
        {
            let relative = project_relative_path(root, path)
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
            write_atomic_bytes(root, journal_path, &journal)?;
        }
        self.unverified_installs.remove(path);
        Ok(())
    }

    fn verify_live_installed_ownership(&self) -> Result<(), BackendError> {
        for (path, ownership) in &self.installed_ownership {
            let target_still_owned = bound_file_identity(&ownership.binding, path).ok()
                == Some(ownership.identity)
                && read_regular_nofollow(&ownership.binding, path)
                    .ok()
                    .map(|bytes| digest_bytes(&bytes))
                    .as_deref()
                    == Some(ownership.hash.as_str());
            if !target_still_owned || !self.live_candidate_pin_matches(ownership.identity) {
                return Err(conflict_error());
            }
        }
        Ok(())
    }

    fn live_candidate_pin_matches(&self, expected: FileIdentity) -> bool {
        #[cfg(unix)]
        {
            self.recovery_artifacts
                .iter()
                .filter(|artifact| is_candidate_identity_pin(&artifact.path))
                .any(|artifact| {
                    bound_file_identity(&artifact.binding, &artifact.path).ok() == Some(expected)
                })
        }
        #[cfg(not(unix))]
        {
            let _ = expected;
            true
        }
    }

    pub(super) fn rollback(&mut self) -> Result<(), BackendError> {
        let mut failures = Vec::new();
        let mut ownership_valid = std::collections::HashMap::new();
        for (path, ownership) in &self.installed_ownership {
            let valid = bound_file_identity(&ownership.binding, path).ok().as_ref()
                == Some(&ownership.identity)
                && read_regular_nofollow(&ownership.binding, path)
                    .ok()
                    .map(|bytes| digest_bytes(&bytes))
                    .as_ref()
                    == Some(&ownership.hash)
                && self.live_candidate_pin_matches(ownership.identity);
            ownership_valid.insert(
                path.clone(),
                valid.then(|| (ownership.identity, ownership.hash.clone())),
            );
        }
        // Windows ownership anchors deny writes and replacement. Close them only
        // after recording the exact identity/hash required again by the final
        // handle-relative rollback operation.
        self.installed_ownership.clear();
        for backup in self.backups.iter().rev() {
            let path = &backup.path;
            if self.unverified_installs.contains(path) {
                failures.push(format!("{}: installed ownership could not be verified; preserved instead of rolling back", self.actionable_path(path)));
                continue;
            }
            let installed_expected = ownership_valid.get(path);
            if installed_expected.is_some_and(Option::is_none) {
                failures.push(format!(
                    "{}: destination changed externally; preserved instead of rolling back",
                    self.actionable_path(path)
                ));
                continue;
            }
            run_before_rollback_final_mutation_hook(path);
            match &backup.previous {
                Some(bytes) => {
                    let result = if self.deleted_destinations.contains(path) {
                        backup.binding.write_atomic_create_new(path, bytes)
                    } else if let Some(Some((identity, hash))) = installed_expected {
                        match backup.binding.write_synced_temp(path, bytes) {
                            Ok(temporary) => {
                                let result = backup.binding.replace_existing_if_identity_and_hash(
                                    &temporary, path, *identity, hash,
                                );
                                if result.is_err() {
                                    let _ = backup.binding.remove_file(&temporary);
                                }
                                result
                            }
                            Err(error) => Err(error),
                        }
                    } else {
                        Err(std::io::Error::new(
                            std::io::ErrorKind::WouldBlock,
                            "rollback ownership proof is missing",
                        ))
                    };
                    if let Err(_error) = result {
                        failures.push(format!(
                            "{}: rollback write conflict or failure; preserved instead of overwriting",
                            self.actionable_path(path)
                        ));
                    }
                }
                None => {
                    let result = if let Some(Some((identity, hash))) = installed_expected {
                        backup
                            .binding
                            .remove_file_if_identity_and_hash(path, *identity, hash)
                    } else {
                        Err(std::io::Error::new(
                            std::io::ErrorKind::WouldBlock,
                            "rollback ownership proof is missing",
                        ))
                    };
                    if let Err(error) = result {
                        if error.kind() != std::io::ErrorKind::NotFound {
                            failures.push(format!(
                                "{}: rollback removal conflict or failure; preserved instead of deleting",
                                self.actionable_path(path)
                            ));
                        }
                    }
                }
            }
        }
        let artifacts = self
            .recovery_artifacts
            .iter()
            .map(|artifact| artifact.path.clone())
            .collect::<Vec<_>>();
        for artifact in artifacts.iter().rev() {
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
        // Rollback is terminal. Release retained file and parent handles after
        // preserving any recovery artifacts on disk; a later explicit retry
        // can safely rebind those recorded names.
        self.backups.clear();
        self.recovery_artifacts.clear();
        // Shared parent capabilities deliberately outlive every individual
        // mutation, but Windows keeps their directory handles open. Release
        // them before removing transaction-created directories so an otherwise
        // complete rollback is not mislabeled as incomplete.
        self.retained_mutation_parents.clear();
        if failures.is_empty() {
            while let Some(directory) = self.created_dirs.pop() {
                if let Err(error) = directory.remove_if_empty() {
                    if error.kind() != std::io::ErrorKind::NotFound {
                        failures.push(format!(
                            "{}: directory cleanup failed",
                            self.actionable_path(directory.path())
                        ));
                    }
                }
            }
        } else {
            self.created_dirs.clear();
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

fn transaction_cancelled_error() -> BackendError {
    BackendError::new(IMPORT_V2_CANCELLED, "Import was cancelled.", true, false)
}

fn write_atomic_bytes(root: &Path, path: &Path, bytes: &[u8]) -> Result<(), BackendError> {
    let (binding, _) = RecoveryParentBinding::ensure_and_bind(root, path)
        .map_err(|error| io_error(error, path))?;
    binding
        .write_atomic_replace(path, bytes)
        .map_err(|error| io_error(error, path))
}

fn write_bound_atomic_bytes(
    binding: &RecoveryParentBinding,
    path: &Path,
    bytes: &[u8],
) -> Result<(), BackendError> {
    let temporary = write_synced_temp(binding, path, bytes)?;
    let result = bound_replace_existing(binding, &temporary, path)
        .map_err(|error| io_error(error, path))
        .and_then(|()| {
            binding
                .sync()
                .map_err(|error| io_error(error, binding.parent()))
        });
    if result.is_err() {
        let _ = bound_remove_file(binding, &temporary);
    }
    result
}

fn write_synced_temp(
    binding: &RecoveryParentBinding,
    path: &Path,
    bytes: &[u8],
) -> Result<PathBuf, BackendError> {
    binding
        .write_synced_temp(path, bytes)
        .map_err(|error| io_error(error, path))
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
    binding.publish_new(temporary, path)
}

fn conflict_error() -> BackendError {
    BackendError::new(
        IMPORT_V2_COMMIT_CONFLICT,
        "A commit target changed concurrently.",
        true,
        true,
    )
}

fn candidate_identity_pin_error(error: std::io::Error, path: &Path) -> BackendError {
    BackendError::new(
        "IMPORT_V2_IDENTITY_PIN_UNAVAILABLE",
        "The project filesystem cannot provide the hard-link identity proof required for a fail-closed import write.",
        false,
        true,
    )
    .with_details(serde_json::json!({
        "path": path.to_string_lossy(),
        "error": error.to_string(),
    }))
}

fn hard_link_identity_proof_error(error: std::io::Error, path: &Path) -> BackendError {
    if error.kind() == std::io::ErrorKind::NotFound {
        return conflict_error();
    }
    if hard_link_capability_unavailable(&error) {
        return candidate_identity_pin_error(error, path);
    }
    io_error(error, path)
}

fn hard_link_capability_unavailable(error: &std::io::Error) -> bool {
    if error.kind() == std::io::ErrorKind::Unsupported {
        return true;
    }
    #[cfg(unix)]
    {
        matches!(
            error.raw_os_error(),
            Some(libc::EXDEV) | Some(libc::EPERM) | Some(libc::EOPNOTSUPP)
        )
    }
    #[cfg(windows)]
    {
        // ERROR_INVALID_FUNCTION, ERROR_NOT_SAME_DEVICE,
        // ERROR_NOT_SUPPORTED.
        matches!(error.raw_os_error(), Some(1) | Some(17) | Some(50))
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

/// Derive a journal path from the transaction's explicit trusted root.
///
/// Existing write resolvers return a canonical parent on Windows, which may
/// carry the extended-path prefix even when the registered root is lexical.
/// Accept that spelling only when it is below the canonical form of the same
/// explicit root; never infer a root from the target itself.
fn project_relative_path(root: &Path, path: &Path) -> Result<PathBuf, ()> {
    if let Ok(relative) = path.strip_prefix(root) {
        return Ok(relative.to_path_buf());
    }
    let canonical_root = root.canonicalize().map_err(|_| ())?;
    path.strip_prefix(canonical_root)
        .map(Path::to_path_buf)
        .map_err(|_| ())
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

#[derive(Clone, Copy)]
enum JournalArtifactKind {
    Candidate,
    OriginalGuard,
    DeleteGuard,
}

fn validate_journal_recovery_artifacts(root: &Path, journal: &Journal) -> Result<(), BackendError> {
    if journal.entries.is_empty() && !journal.recovery_artifacts.is_empty() {
        return Err(staging_safe_io_error());
    }
    let mut unique = HashSet::new();
    for relative in &journal.recovery_artifacts {
        if !unique.insert(relative.as_str()) {
            return Err(staging_safe_io_error());
        }
        let artifact = safe_journal_target(root, relative)?;
        let artifact_parent = artifact.parent().ok_or_else(staging_safe_io_error)?;
        let artifact_name = artifact
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(staging_safe_io_error)?;
        let associations = journal
            .entries
            .iter()
            .filter_map(|entry| {
                let target = safe_journal_target(root, &entry.relative_path).ok()?;
                if target.parent() != Some(artifact_parent) {
                    return None;
                }
                journal_artifact_kind(artifact_name, &target, entry).map(|kind| (entry, kind))
            })
            .collect::<Vec<_>>();
        if associations.is_empty() {
            return Err(staging_safe_io_error());
        }
        let binding = bind_recovery_parent(root, &artifact)?;
        let existing = match binding.read_regular_with_identity(&artifact) {
            Ok((bytes, identity)) => Some((digest_bytes(&bytes), identity)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err(staging_safe_io_error()),
        };

        let matches_entry = associations
            .into_iter()
            .any(|(entry, kind)| match (&existing, kind) {
                (None, _) => true,
                (Some((hash, identity)), JournalArtifactKind::Candidate) => {
                    !entry.desired_absent
                        && hash == &entry.desired_hash
                        && entry.installed_identity.as_ref() == Some(identity)
                }
                (Some((hash, _)), JournalArtifactKind::OriginalGuard) => {
                    !entry.desired_absent
                        && entry.previous.as_deref().map(digest_bytes).as_ref() == Some(hash)
                }
                (Some((hash, _)), JournalArtifactKind::DeleteGuard) => {
                    entry.desired_absent
                        && entry.previous.as_deref().map(digest_bytes).as_ref() == Some(hash)
                }
            });
        if !matches_entry {
            // The journal still owns a structurally valid artifact name, but
            // the namespace object no longer proves the recorded transaction
            // identity. Treat that as external drift and stop fail-closed;
            // malformed or unassociated artifact names are rejected above.
            return Err(conflict_error());
        }
    }
    Ok(())
}

fn journal_artifact_kind(
    artifact_name: &str,
    target: &Path,
    entry: &JournalEntry,
) -> Option<JournalArtifactKind> {
    let target_name = target.file_name()?.to_string_lossy();
    let temporary_prefix = format!(".{target_name}.");
    if generated_uuid_name(artifact_name, &temporary_prefix, ".tmp") {
        return Some(JournalArtifactKind::Candidate);
    }
    if generated_uuid_name(artifact_name, ".wiki-guard-installed-", "") {
        return Some(JournalArtifactKind::Candidate);
    }
    if entry.previous.is_some() && generated_uuid_name(artifact_name, ".wiki-guard-", "") {
        return Some(JournalArtifactKind::OriginalGuard);
    }
    if entry.desired_absent && generated_uuid_name(artifact_name, ".wiki-delete-guard-", "") {
        return Some(JournalArtifactKind::DeleteGuard);
    }
    None
}

fn generated_uuid_name(name: &str, prefix: &str, suffix: &str) -> bool {
    let Some(rest) = name.strip_prefix(prefix) else {
        return false;
    };
    let Some(id) = rest.strip_suffix(suffix) else {
        return false;
    };
    uuid::Uuid::parse_str(id).is_ok()
}

fn bind_recovery_parent(root: &Path, target: &Path) -> Result<RecoveryParentBinding, BackendError> {
    RecoveryParentBinding::bind(root, target).map_err(|error| io_error(error, target))
}

fn bound_remove_file(binding: &RecoveryParentBinding, path: &Path) -> Result<(), std::io::Error> {
    binding.remove_file(path)
}

fn bound_hard_link(
    binding: &RecoveryParentBinding,
    existing: &Path,
    new_path: &Path,
) -> Result<(), std::io::Error> {
    binding.hard_link(existing, new_path)
}

fn is_candidate_identity_pin(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with(".wiki-guard-installed-"))
}

fn persist_recovery_journal(
    binding: &RecoveryParentBinding,
    path: &Path,
    journal: &Journal,
) -> Result<(), BackendError> {
    let bytes = serde_json::to_vec(journal).map_err(|_| staging_safe_io_error())?;
    write_bound_atomic_bytes(binding, path, &bytes)
}

fn plan_and_recover_entry(
    root: &Path,
    journal_binding: &RecoveryParentBinding,
    journal_path: &Path,
    journal: &mut Journal,
    index: usize,
    binding: &RecoveryParentBinding,
    path: &Path,
) -> Result<(), BackendError> {
    let entry = &journal.entries[index];
    let expected_identity = entry.installed_identity.ok_or_else(conflict_error)?;
    let guard = binding.parent().join(format!(
        ".import-v2-recovery-guard-{}",
        uuid::Uuid::new_v4()
    ));
    let guard_relative = guard
        .strip_prefix(root)
        .map_err(|_| staging_safe_io_error())?
        .to_string_lossy()
        .replace('\\', "/");
    journal.entries[index].recovery = Some(RecoveryRecord {
        canonical_relative_path: journal.entries[index].relative_path.clone(),
        guard_relative_path: guard_relative,
        expected_hash: journal.entries[index].desired_hash.clone(),
        expected_identity,
        action: if journal.entries[index].previous.is_some() {
            RecoveryAction::RestorePrevious
        } else {
            RecoveryAction::DeleteInstalled
        },
        phase: RecoveryPhase::Planned,
    });
    // The guard name and ownership proof must reach stable storage before the
    // canonical child can leave its name.
    persist_recovery_journal(journal_binding, journal_path, journal)?;
    resume_recovery_entry(
        root,
        journal_binding,
        journal_path,
        journal,
        index,
        binding,
        path,
    )
}

fn resume_recovery_entry(
    root: &Path,
    journal_binding: &RecoveryParentBinding,
    journal_path: &Path,
    journal: &mut Journal,
    index: usize,
    binding: &RecoveryParentBinding,
    path: &Path,
) -> Result<(), BackendError> {
    let record = journal.entries[index]
        .recovery
        .clone()
        .ok_or_else(staging_safe_io_error)?;
    let expected_action = if journal.entries[index].previous.is_some() {
        RecoveryAction::RestorePrevious
    } else {
        RecoveryAction::DeleteInstalled
    };
    if record.canonical_relative_path != journal.entries[index].relative_path
        || record.expected_hash != journal.entries[index].desired_hash
        || Some(record.expected_identity) != journal.entries[index].installed_identity
        || record.action != expected_action
    {
        return Err(staging_safe_io_error());
    }
    let guard = safe_journal_target(root, &record.guard_relative_path)?;
    if guard.parent() != path.parent()
        || !guard.file_name().is_some_and(|name| {
            name.to_string_lossy()
                .starts_with(".import-v2-recovery-guard-")
        })
    {
        return Err(staging_safe_io_error());
    }
    let previous = journal.entries[index].previous.clone();
    let previous_hash = previous.as_deref().map(digest_bytes);
    let guard_exists = bound_file_identity(binding, &guard).is_ok();

    if !guard_exists {
        let canonical = read_regular_nofollow(binding, path).ok();
        let canonical_hash = canonical.as_deref().map(digest_bytes);
        let canonical_is_installed = bound_file_identity(binding, path).ok()
            == Some(record.expected_identity)
            && canonical_hash.as_deref() == Some(&record.expected_hash);
        if record.phase == RecoveryPhase::Planned && canonical_is_installed {
            run_before_recovery_mutation_hook(path);
            bound_quarantine(binding, path, &guard).map_err(|error| io_error(error, path))?;
            binding
                .sync()
                .map_err(|error| io_error(error, binding.parent()))?;
            recovery_fault_boundary("after_quarantine");
            journal.entries[index].recovery.as_mut().unwrap().phase = RecoveryPhase::Quarantined;
            persist_recovery_journal(journal_binding, journal_path, journal)?;
        } else {
            let complete = match record.action {
                RecoveryAction::DeleteInstalled => true,
                RecoveryAction::RestorePrevious => canonical_hash == previous_hash,
            };
            if complete {
                journal.entries[index].recovery = None;
                persist_recovery_journal(journal_binding, journal_path, journal)?;
                return Ok(());
            }
            return Err(conflict_error());
        }
    }

    let verified = bound_file_identity(binding, &guard).ok() == Some(record.expected_identity)
        && read_regular_nofollow(binding, &guard)
            .ok()
            .is_some_and(|bytes| digest_bytes(&bytes) == record.expected_hash);
    if !verified {
        let restore = bound_restore_guard_if_absent(binding, &guard, path);
        let guard_relative = guard
            .strip_prefix(root)
            .unwrap_or(&guard)
            .to_string_lossy()
            .replace('\\', "/");
        return Err(rollback_failure(
            "Recovery could not verify the quarantined installed file.",
            vec![match restore {
                Ok(()) => format!(
                    "{}: unverified child was restored without replacing another file",
                    guard_relative
                ),
                Err(error) => format!(
                    "{}: unverified child was preserved; canonical-name restore failed: {}",
                    guard_relative, error
                ),
            }],
        ));
    }

    if record.action == RecoveryAction::RestorePrevious {
        let canonical_hash = read_regular_nofollow(binding, path)
            .ok()
            .as_deref()
            .map(digest_bytes);
        if canonical_hash != previous_hash {
            run_before_recovery_final_mutation_hook(path);
            let temporary = write_synced_temp(binding, path, previous.as_deref().unwrap())?;
            if let Err(error) = install_candidate(binding, &temporary, path) {
                let _ = bound_remove_file(binding, &temporary);
                return Err(rollback_failure(
                    "Recovery preserved an external child replacement.",
                    vec![format!(
                        "{}: verified installed recovery guard was retained; canonical destination was not overwritten ({error})",
                        record.guard_relative_path
                    )],
                ));
            }
            binding
                .sync()
                .map_err(|error| io_error(error, binding.parent()))?;
            let _ = bound_remove_file(binding, &temporary);
            recovery_fault_boundary("after_restore");
        }
        journal.entries[index].recovery.as_mut().unwrap().phase = RecoveryPhase::Restored;
        persist_recovery_journal(journal_binding, journal_path, journal)?;
    } else {
        // New-file rollback's no-clobber action is intentionally leaving the
        // canonical name absent (or preserving a replacement that won it).
        run_before_recovery_final_mutation_hook(path);
        recovery_fault_boundary("after_restore");
    }
    recovery_fault_boundary("before_guard_remove");
    bound_remove_file(binding, &guard).map_err(|error| io_error(error, &guard))?;
    binding
        .sync()
        .map_err(|error| io_error(error, binding.parent()))?;
    recovery_fault_boundary("after_guard_remove");
    journal.entries[index].recovery = None;
    persist_recovery_journal(journal_binding, journal_path, journal)
}

fn bound_replace_existing(
    binding: &RecoveryParentBinding,
    temporary: &Path,
    path: &Path,
) -> Result<(), std::io::Error> {
    #[cfg(test)]
    if FAIL_NEXT_CANDIDATE_INSTALL.with(|flag| flag.replace(false)) {
        return Err(std::io::Error::other("injected candidate install failure"));
    }
    binding.replace_existing(temporary, path)
}

fn bound_quarantine(
    binding: &RecoveryParentBinding,
    path: &Path,
    guard: &Path,
) -> Result<(), std::io::Error> {
    binding.rename(path, guard, false)
}

fn bound_restore_guard_if_absent(
    binding: &RecoveryParentBinding,
    guard: &Path,
    path: &Path,
) -> Result<(), std::io::Error> {
    bound_hard_link(binding, guard, path)?;
    bound_remove_file(binding, guard)
}

fn read_regular_nofollow(
    binding: &RecoveryParentBinding,
    path: &Path,
) -> Result<Vec<u8>, BackendError> {
    binding
        .read_regular(path)
        .map_err(|error| io_error(error, path))
}

fn bound_file_identity(
    binding: &RecoveryParentBinding,
    path: &Path,
) -> Result<FileIdentity, BackendError> {
    binding
        .file_identity(path)
        .map_err(|error| io_error(error, path))
}

fn revalidate_recovery_parent(_binding: &RecoveryParentBinding) -> Result<(), BackendError> {
    // The open parent handle/descriptor is the capability. Namespace changes
    // cannot redirect subsequent child operations away from that retained root.
    Ok(())
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

fn rollback_failure(message: &str, failures: Vec<String>) -> BackendError {
    BackendError::new(IMPORT_V2_COMMIT_FAILED, message, false, true)
        .with_details(serde_json::json!({ "rollbackFailures": failures }))
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn journal_identity_matches(
    root: &Path,
    journal: &Journal,
    binding: &RecoveryParentBinding,
    path: &Path,
    entry: &JournalEntry,
    require_candidate_pin: bool,
) -> Result<bool, BackendError> {
    let Some(expected) = entry.installed_identity else {
        return Ok(false);
    };
    let target_matches = bound_file_identity(binding, path).map(|actual| actual == expected)?;
    if !target_matches {
        return Ok(false);
    }
    if require_candidate_pin {
        journal_candidate_pin_matches(root, journal, entry)
    } else {
        Ok(true)
    }
}

fn journal_candidate_pin_matches(
    root: &Path,
    journal: &Journal,
    entry: &JournalEntry,
) -> Result<bool, BackendError> {
    let Some(expected) = entry.installed_identity else {
        return Ok(false);
    };
    #[cfg(unix)]
    {
        for relative in &journal.recovery_artifacts {
            let artifact = safe_journal_target(root, relative)?;
            if !is_candidate_identity_pin(&artifact) {
                continue;
            }
            let binding = bind_recovery_parent(root, &artifact)?;
            match binding.file_identity(&artifact) {
                Ok(actual) if actual == expected => return Ok(true),
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(io_error(error, &artifact)),
            }
        }
        Ok(false)
    }
    #[cfg(not(unix))]
    {
        let _ = (root, journal, expected);
        Ok(true)
    }
}

struct InstalledOwnership {
    identity: FileIdentity,
    hash: String,
    binding: Arc<RecoveryParentBinding>,
    _anchor: Option<std::fs::File>,
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
fn file_identity_from_file(
    _file: &std::fs::File,
    path: &Path,
) -> Result<FileIdentity, BackendError> {
    Err(BackendError::new(
        IMPORT_V2_COMMIT_FAILED,
        "Stable file identity is unavailable on this platform.",
        false,
        true,
    )
    .with_details(serde_json::json!({ "path": path.to_string_lossy() })))
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
    #[cfg(unix)]
    use super::set_after_journal_directory_bind_hook;
    use super::{
        digest_bytes, hard_link_identity_proof_error, set_after_initial_journal_publish_hook,
        set_before_checked_displace_hook, set_before_checked_final_mutation_hook,
        set_before_new_install_hook, set_before_recovery_final_mutation_hook,
        set_before_recovery_mutation_hook, set_before_rollback_final_mutation_hook,
        set_fail_next_candidate_install, set_fail_next_cleanup, set_fail_next_identity_query,
        set_recovery_process_death_hook, FileTransaction, Journal, JournalState,
        IMPORT_V2_CANCELLED, IMPORT_V2_COMMIT_CONFLICT,
    };
    use sha2::Digest;
    use std::path::Path;

    #[cfg(unix)]
    fn candidate_identity_pin(root: &Path) -> std::path::PathBuf {
        std::fs::read_dir(root)
            .unwrap()
            .filter_map(Result::ok)
            .find(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".wiki-guard-installed-")
            })
            .expect("candidate identity pin")
            .path()
    }

    #[test]
    fn checked_overwrite_and_delete_preserve_last_moment_external_edits() {
        for delete in [false, true] {
            let root = std::env::temp_dir()
                .join(format!("import-v2-final-check-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&root).unwrap();
            let path = root.join("page.md");
            std::fs::write(&path, b"before").unwrap();
            let hook_path = path.clone();
            set_before_checked_final_mutation_hook(move |_| {
                std::fs::write(&hook_path, b"external").unwrap();
            });
            let mut transaction = FileTransaction::new_for_project(&root);
            let result = if delete {
                transaction.delete_if_hash_matches(&path, &digest_bytes(b"before"))
            } else {
                transaction.write_if_hash_matches(&path, b"candidate", &digest_bytes(b"before"))
            };
            assert!(result.is_err());
            assert_eq!(std::fs::read(&path).unwrap(), b"external");
            drop(transaction);
            assert_eq!(std::fs::read(&path).unwrap(), b"external");
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn hard_link_identity_proof_errors_preserve_race_classification() {
        let path = Path::new("wiki/page.md");
        let race = hard_link_identity_proof_error(
            std::io::Error::from(std::io::ErrorKind::NotFound),
            path,
        );
        assert_eq!(race.code, IMPORT_V2_COMMIT_CONFLICT);

        #[cfg(unix)]
        let unsupported = std::io::Error::from_raw_os_error(libc::EXDEV);
        #[cfg(windows)]
        let unsupported = std::io::Error::from_raw_os_error(50);
        #[cfg(not(any(unix, windows)))]
        let unsupported = std::io::Error::from(std::io::ErrorKind::Unsupported);
        let capability = hard_link_identity_proof_error(unsupported, path);
        assert_eq!(capability.code, "IMPORT_V2_IDENTITY_PIN_UNAVAILABLE");
    }

    #[test]
    fn live_rollback_preserves_replacements_after_ownership_validation() {
        for created in [false, true] {
            let root = std::env::temp_dir().join(format!(
                "import-v2-rollback-final-check-{}",
                uuid::Uuid::new_v4()
            ));
            std::fs::create_dir_all(&root).unwrap();
            let path = root.join("page.md");
            let mut transaction = FileTransaction::new_for_project(&root);
            if created {
                transaction.write_new(&path, b"candidate").unwrap();
            } else {
                std::fs::write(&path, b"before").unwrap();
                transaction
                    .write_if_hash_matches(&path, b"candidate", &digest_bytes(b"before"))
                    .unwrap();
            }
            let hook_path = path.clone();
            set_before_rollback_final_mutation_hook(move |_| {
                std::fs::remove_file(&hook_path).unwrap();
                std::fs::write(&hook_path, b"external").unwrap();
            });
            assert!(transaction.rollback().is_err());
            assert_eq!(std::fs::read(&path).unwrap(), b"external");
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn commit_rejects_external_target_replacement_even_while_candidate_pin_survives() {
        let root = std::env::temp_dir().join(format!(
            "import-v2-commit-target-replacement-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("page.md");
        let displaced = root.join("candidate-displaced.md");

        let mut transaction = FileTransaction::new_for_project(&root);
        transaction.write_new(&path, b"candidate").unwrap();
        std::fs::rename(&path, &displaced).unwrap();
        std::fs::write(&path, b"external").unwrap();

        let result = transaction.commit();

        assert!(result.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"external");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dropped_transaction_restores_overwrites_and_removes_created_tree() {
        let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let existing = root.join("existing.md");
        std::fs::write(&existing, b"before").unwrap();
        {
            let mut transaction = FileTransaction::new();
            transaction
                .write_if_hash_matches(&existing, b"after", &digest_bytes(b"before"))
                .unwrap();
            transaction
                .write_new(&root.join("new/tree/file.bin"), b"created")
                .unwrap();
        }
        assert_eq!(std::fs::read(&existing).unwrap(), b"before");
        assert!(!root.join("new").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn one_transaction_can_create_sibling_and_descendant_parent_targets() {
        let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let mut transaction = FileTransaction::new_for_project(&root);
        transaction
            .write_new(
                &root.join("raw/sources/source/version/original.md"),
                b"original",
            )
            .unwrap();
        transaction
            .write_new(
                &root.join(".app/source-artifacts/source/version/baseline.md"),
                b"baseline",
            )
            .unwrap();
        transaction
            .write_new(
                &root.join("raw/sources/source/version/derived/extracted.md"),
                b"derived",
            )
            .unwrap();
        transaction.commit().unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn first_single_file_journal_publish_contains_every_prepared_artifact() {
        let root = std::env::temp_dir().join(format!(
            "import-v2-initial-journal-artifacts-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let target = root.join("page.md");
        let observed = std::rc::Rc::new(std::cell::Cell::new(false));
        let hook_observed = observed.clone();
        set_after_initial_journal_publish_hook(move |path| {
            let journal: Journal = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            assert_eq!(journal.entries.len(), 1);
            assert!(journal
                .recovery_artifacts
                .iter()
                .any(|artifact| artifact.starts_with(".page.md.") && artifact.ends_with(".tmp")));
            #[cfg(unix)]
            assert!(journal
                .recovery_artifacts
                .iter()
                .any(|artifact| artifact.starts_with(".wiki-guard-installed-")));
            hook_observed.set(true);
        });

        let mut transaction = FileTransaction::new_for_project(&root);
        transaction.write_new(&target, b"candidate").unwrap();
        assert!(observed.get());
        transaction.commit().unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn live_rollback_uses_retained_parent_while_symlink_swap_is_active() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "import-v2-live-rollback-swap-{}",
            uuid::Uuid::new_v4()
        ));
        let live = root.join("wiki");
        let parked = root.join("wiki-owned");
        let outside = std::env::temp_dir().join(format!(
            "import-v2-live-rollback-outside-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&live).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let existing = live.join("existing.md");
        std::fs::write(&existing, b"before").unwrap();
        let sentinel = outside.join("sentinel.md");
        std::fs::write(&sentinel, b"outside").unwrap();

        let mut transaction = FileTransaction::new_for_project(&root);
        transaction
            .write_if_hash_matches(&existing, b"after", &digest_bytes(b"before"))
            .unwrap();
        transaction
            .write_new(&live.join("created.md"), b"created")
            .unwrap();
        std::fs::rename(&live, &parked).unwrap();
        symlink(&outside, &live).unwrap();

        drop(transaction);

        assert_eq!(
            std::fs::read(parked.join("existing.md")).unwrap(),
            b"before"
        );
        assert!(!parked.join("created.md").exists());
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"outside");
        std::fs::remove_file(&live).unwrap();
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(outside).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn live_rollback_pins_parent_against_junction_replacement() {
        let root = std::env::temp_dir().join(format!(
            "import-v2-live-rollback-pin-{}",
            uuid::Uuid::new_v4()
        ));
        let live = root.join("wiki");
        let parked = root.join("wiki-owned");
        std::fs::create_dir_all(&live).unwrap();
        let existing = live.join("existing.md");
        std::fs::write(&existing, b"before").unwrap();
        let mut transaction = FileTransaction::new_for_project(&root);
        transaction
            .write_if_hash_matches(&existing, b"after", &digest_bytes(b"before"))
            .unwrap();

        assert!(std::fs::rename(&live, &parked).is_err());
        drop(transaction);

        assert_eq!(std::fs::read(&existing).unwrap(), b"before");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn committed_transaction_keeps_all_writes() {
        let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        let path = root.join("nested/file.bin");
        let mut transaction = FileTransaction::new();
        transaction.write_new(&path, b"kept").unwrap();
        transaction.commit().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"kept");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn checked_cohort_reuses_one_retained_parent_capability_and_syncs_once() {
        let root = std::env::temp_dir().join(format!("import-v2-cohort-{}", uuid::Uuid::new_v4()));
        let items = root.join(".app/import-sessions/s/items");
        std::fs::create_dir_all(&items).unwrap();
        let writes = (0..128)
            .map(|index| {
                let path = items.join(format!("{index}.json"));
                std::fs::write(&path, b"before").unwrap();
                (path, b"after".to_vec(), digest_bytes(b"before"))
            })
            .collect::<Vec<_>>();
        let mut transaction = FileTransaction::new_for_project(&root);
        transaction.write_many_if_hash_matches(&writes).unwrap();
        assert_eq!(transaction.retained_mutation_parent_count(), 1);
        assert_eq!(
            transaction.retained_installed_anchor_count(),
            if cfg!(unix) { 0 } else { writes.len() }
        );
        assert_eq!(
            transaction.cohort_pin_parent_sync_count(),
            usize::from(cfg!(unix))
        );
        transaction.commit().unwrap();
        assert!(writes
            .iter()
            .all(|(path, _, _)| std::fs::read(path).unwrap() == b"after"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn checked_cohort_rejects_a_parent_directory_swap_before_reusing_its_capability() {
        let root = std::env::temp_dir().join(format!(
            "import-v2-cohort-parent-swap-{}",
            uuid::Uuid::new_v4()
        ));
        let items = root.join(".app/import-sessions/s/items");
        let parked = root.join("parked-items");
        std::fs::create_dir_all(&items).unwrap();
        let writes = ["one.json", "two.json"]
            .into_iter()
            .map(|name| {
                let path = items.join(name);
                std::fs::write(&path, b"before").unwrap();
                (path, b"after".to_vec(), digest_bytes(b"before"))
            })
            .collect::<Vec<_>>();
        let live_for_hook = items.clone();
        let parked_for_hook = parked.clone();
        set_before_checked_displace_hook(move |_| {
            std::fs::rename(&live_for_hook, &parked_for_hook).unwrap();
            std::fs::create_dir_all(&live_for_hook).unwrap();
            true
        });

        let error = {
            let mut transaction = FileTransaction::new_for_project(&root);
            transaction.write_many_if_hash_matches(&writes).unwrap_err()
        };

        assert_eq!(error.code, IMPORT_V2_COMMIT_CONFLICT);
        assert_eq!(std::fs::read(parked.join("one.json")).unwrap(), b"before");
        assert_eq!(std::fs::read(parked.join("two.json")).unwrap(), b"before");
        assert!(std::fs::read_dir(&items).unwrap().next().is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancelled_checked_cohort_rolls_back_after_the_first_install() {
        let root = std::env::temp_dir().join(format!(
            "import-v2-cohort-cancel-install-{}",
            uuid::Uuid::new_v4()
        ));
        let items = root.join(".app/import-sessions/s/items");
        std::fs::create_dir_all(&items).unwrap();
        let writes = ["one.json", "two.json"]
            .into_iter()
            .map(|name| {
                let path = items.join(name);
                std::fs::write(&path, b"before").unwrap();
                (path, b"after".to_vec(), digest_bytes(b"before"))
            })
            .collect::<Vec<_>>();
        let mut checks = 0usize;
        let error;
        {
            let mut transaction = FileTransaction::new_for_project(&root);
            error = transaction
                .write_many_if_hash_matches_with_cancel(&writes, || {
                    checks += 1;
                    // Two prepare checks + the pre-journal check + the first
                    // install check have passed. Cancel immediately before
                    // the second install, after the first file was replaced.
                    checks == 5
                })
                .unwrap_err();
            assert_eq!(std::fs::read(&writes[0].0).unwrap(), b"after");
        }

        assert_eq!(error.code, IMPORT_V2_CANCELLED);
        assert!(writes
            .iter()
            .all(|(path, _, _)| std::fs::read(path).unwrap() == b"before"));
        assert_eq!(std::fs::read_dir(&items).unwrap().count(), 2);
        let journals = root.join(".app/import-v2-journal");
        assert!(!journals.exists() || std::fs::read_dir(journals).unwrap().next().is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn checked_delete_rolls_back_or_commits_atomically() {
        let root = std::env::temp_dir().join(format!("import-v2-delete-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("wiki")).unwrap();
        let path = root.join("wiki/old.md");
        std::fs::write(&path, b"before").unwrap();
        let hash = digest_bytes(b"before");
        {
            let mut transaction = FileTransaction::new_for_project(&root);
            transaction.delete_if_hash_matches(&path, &hash).unwrap();
            assert!(!path.exists());
        }
        assert_eq!(std::fs::read(&path).unwrap(), b"before");

        let mut transaction = FileTransaction::new_for_project(&root);
        transaction.delete_if_hash_matches(&path, &hash).unwrap();
        transaction.commit().unwrap();
        assert!(!path.exists());
        FileTransaction::reconcile_project(&root).unwrap();
        assert!(!path.exists());
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
        transaction
            .write_if_hash_matches(&path, b"after", &digest_bytes(b"before"))
            .unwrap();
        let hook_path = path.clone();
        set_before_rollback_final_mutation_hook(move |_| {
            std::fs::remove_file(&hook_path).unwrap();
            std::fs::create_dir(&hook_path).unwrap();
        });
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
        transaction.write_new(&path, b"new").unwrap();
        let hook_path = path.clone();
        set_before_rollback_final_mutation_hook(move |_| {
            std::fs::remove_file(&hook_path).unwrap();
            std::fs::create_dir(&hook_path).unwrap();
        });
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
        transaction.write_new(&path, b"new").unwrap();
        let hook_path = path.clone();
        set_before_rollback_final_mutation_hook(move |_| {
            std::fs::remove_file(&hook_path).unwrap();
            std::fs::create_dir(&hook_path).unwrap();
        });
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
        let hook_path = path.clone();
        set_before_rollback_final_mutation_hook(move |_| {
            std::fs::write(&hook_path, b"external replacement").unwrap();
        });
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
        let hook_path = path.clone();
        set_before_rollback_final_mutation_hook(move |_| {
            std::fs::remove_file(&hook_path).unwrap();
            std::fs::write(&hook_path, b"same bytes").unwrap();
        });
        transaction.rollback().unwrap_err();
        assert_eq!(std::fs::read(&path).unwrap(), b"same bytes");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn candidate_identity_pin_is_journaled_until_durable_commit() {
        use std::os::unix::fs::MetadataExt;

        let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("new.bin");
        let mut transaction = FileTransaction::new_for_project(&root);
        transaction.write_new(&path, b"candidate").unwrap();

        let identity_pin = candidate_identity_pin(&root);
        let installed_metadata = std::fs::metadata(&path).unwrap();
        let pin_metadata = std::fs::metadata(&identity_pin).unwrap();
        assert_eq!(
            (installed_metadata.dev(), installed_metadata.ino()),
            (pin_metadata.dev(), pin_metadata.ino())
        );

        let journal_path = transaction.journal_path.as_ref().unwrap();
        let journal: super::Journal =
            serde_json::from_slice(&std::fs::read(journal_path).unwrap()).unwrap();
        assert!(journal.recovery_artifacts.iter().any(|artifact| {
            artifact
                .split('/')
                .next_back()
                .is_some_and(|name| name.starts_with(".wiki-guard-installed-"))
        }));

        transaction.commit().unwrap();
        assert!(!identity_pin.exists());
        assert_eq!(
            std::fs::read_dir(root.join(".app/import-v2-journal"))
                .unwrap()
                .count(),
            0
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn live_rollback_preserves_candidate_when_identity_pin_is_missing() {
        let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("new.bin");
        let mut transaction = FileTransaction::new_for_project(&root);
        transaction.write_new(&path, b"candidate").unwrap();
        std::fs::remove_file(candidate_identity_pin(&root)).unwrap();

        let error = transaction
            .rollback()
            .expect_err("a missing pin must invalidate live rollback ownership");

        assert_eq!(error.code, crate::errors::IMPORT_V2_COMMIT_FAILED);
        assert_eq!(std::fs::read(&path).unwrap(), b"candidate");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn restart_recovery_rejects_missing_or_rebound_identity_pin() {
        for rebind in [false, true] {
            let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&root).unwrap();
            let path = root.join("new.bin");
            let mut transaction = FileTransaction::new_for_project(&root);
            transaction.write_new(&path, b"candidate").unwrap();
            let pin = candidate_identity_pin(&root);
            transaction.simulate_process_crash();
            std::fs::remove_file(&pin).unwrap();
            if rebind {
                std::fs::write(&pin, b"unrelated pin replacement").unwrap();
            }

            let error = FileTransaction::reconcile_project(&root)
                .expect_err("recovery must require the journaled candidate inode anchor");

            assert_eq!(error.code, IMPORT_V2_COMMIT_CONFLICT);
            assert_eq!(std::fs::read(&path).unwrap(), b"candidate");
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn committed_pin_cleanup_failure_is_success_and_reconciles_later() {
        let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("new.bin");
        let mut transaction = FileTransaction::new_for_project(&root);
        transaction.write_new(&path, b"candidate").unwrap();
        set_fail_next_cleanup("guard");

        transaction
            .commit()
            .expect("cleanup after the durable marker cannot undo commit success");
        assert_eq!(std::fs::read(&path).unwrap(), b"candidate");
        assert!(candidate_identity_pin(&root).exists());
        FileTransaction::reconcile_project(&root).unwrap();
        assert!(!std::fs::read_dir(root.join(".app/import-v2-journal"))
            .unwrap()
            .any(|entry| entry.is_ok()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn committed_journal_without_pin_reconciles_after_delete_failure() {
        let root = std::env::temp_dir().join(format!("import-v2-tx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("new.bin");
        let mut transaction = FileTransaction::new_for_project(&root);
        transaction.write_new(&path, b"candidate").unwrap();
        let pin = candidate_identity_pin(&root);
        super::set_fail_next_journal_delete();

        transaction
            .commit()
            .expect("durably committed bytes remain successful when journal cleanup is pending");
        assert!(!pin.exists());
        assert_eq!(std::fs::read(&path).unwrap(), b"candidate");
        FileTransaction::reconcile_project(&root).unwrap();
        assert!(!std::fs::read_dir(root.join(".app/import-v2-journal"))
            .unwrap()
            .any(|entry| entry.is_ok()));
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
        transaction
            .write_if_hash_matches(&path, b"after", &digest_bytes(b"before"))
            .unwrap();
        let hook_path = path.clone();
        set_before_rollback_final_mutation_hook(move |_| {
            std::fs::remove_file(&hook_path).unwrap();
            std::fs::write(&hook_path, b"after").unwrap();
        });
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
        transaction
            .write_if_hash_matches(&path, b"after", &digest_bytes(b"before"))
            .unwrap();
        let hook_path = path.clone();
        set_before_rollback_final_mutation_hook(move |_| {
            std::fs::remove_file(&hook_path).unwrap();
        });
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
        std::fs::create_dir_all(&root).unwrap();
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
    fn recovery_new_file_quarantine_never_unlinks_a_last_moment_child_replacement() {
        for replacement in [b"installed".as_slice(), b"external-different".as_slice()] {
            let root = std::env::temp_dir().join(format!(
                "import-v2-recovery-child-new-{}",
                uuid::Uuid::new_v4()
            ));
            std::fs::create_dir_all(&root).unwrap();
            let target = root.join("wiki/new.md");
            let mut transaction = FileTransaction::new_for_project(&root);
            transaction.write_new(&target, b"installed").unwrap();
            transaction.simulate_process_crash();

            let hook_target = target.clone();
            let replacement = replacement.to_vec();
            let hook_replacement = replacement.clone();
            set_before_recovery_final_mutation_hook(move |_| {
                std::fs::write(&hook_target, hook_replacement).unwrap();
            });

            FileTransaction::reconcile_project(&root).unwrap();
            assert_eq!(std::fs::read(&target).unwrap(), replacement);
            assert!(std::fs::read_dir(target.parent().unwrap())
                .unwrap()
                .all(|entry| !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains("recovery-guard")));
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn recovery_overwrite_quarantine_preserves_last_moment_child_and_previous_guard() {
        for replacement in [b"after".as_slice(), b"external-different".as_slice()] {
            let root = std::env::temp_dir().join(format!(
                "import-v2-recovery-child-overwrite-{}",
                uuid::Uuid::new_v4()
            ));
            let target = root.join("wiki/existing.md");
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(&target, b"before").unwrap();
            let mut transaction = FileTransaction::new_for_project(&root);
            transaction
                .write_if_hash_matches(&target, b"after", &digest_bytes(b"before"))
                .unwrap();
            transaction.simulate_process_crash();

            let hook_target = target.clone();
            let replacement = replacement.to_vec();
            let hook_replacement = replacement.clone();
            set_before_recovery_final_mutation_hook(move |_| {
                std::fs::write(&hook_target, hook_replacement).unwrap();
            });

            let error = FileTransaction::reconcile_project(&root).unwrap_err();
            assert_eq!(std::fs::read(&target).unwrap(), replacement);
            let details = error.details.unwrap();
            let failures = details["rollbackFailures"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap())
                .collect::<Vec<_>>();
            assert!(failures.iter().any(|failure| {
                failure.contains("wiki/.import-v2-recovery-guard-")
                    && failure.contains("canonical destination was not overwritten")
            }));
            let guards = std::fs::read_dir(target.parent().unwrap())
                .unwrap()
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .contains("recovery-guard")
                })
                .collect::<Vec<_>>();
            assert_eq!(guards.len(), 1);
            assert_eq!(std::fs::read(&guards[0]).unwrap(), b"after");
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn restart_recovery_is_idempotent_at_every_new_file_recovery_boundary() {
        for boundary in [
            "after_quarantine",
            "after_restore",
            "before_guard_remove",
            "after_guard_remove",
        ] {
            let root = std::env::temp_dir().join(format!(
                "import-v2-recovery-death-new-{boundary}-{}",
                uuid::Uuid::new_v4()
            ));
            std::fs::create_dir_all(&root).unwrap();
            let target = root.join("wiki/new.md");
            let mut transaction = FileTransaction::new_for_project(&root);
            transaction.write_new(&target, b"installed").unwrap();
            transaction.simulate_process_crash();
            set_recovery_process_death_hook(move |phase| phase == boundary);
            assert!(
                std::panic::catch_unwind(|| FileTransaction::reconcile_project(&root)).is_err()
            );
            FileTransaction::reconcile_project(&root).unwrap();
            FileTransaction::reconcile_project(&root).unwrap();
            assert!(!target.exists());
            assert_no_recovery_residue(&root, target.parent().unwrap());
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn restart_recovery_is_idempotent_at_every_overwrite_recovery_boundary() {
        for boundary in [
            "after_quarantine",
            "after_restore",
            "before_guard_remove",
            "after_guard_remove",
        ] {
            let root = std::env::temp_dir().join(format!(
                "import-v2-recovery-death-overwrite-{boundary}-{}",
                uuid::Uuid::new_v4()
            ));
            let target = root.join("wiki/existing.md");
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(&target, b"before").unwrap();
            let mut transaction = FileTransaction::new_for_project(&root);
            transaction
                .write_if_hash_matches(&target, b"after", &digest_bytes(b"before"))
                .unwrap();
            transaction.simulate_process_crash();
            set_recovery_process_death_hook(move |phase| phase == boundary);
            assert!(
                std::panic::catch_unwind(|| FileTransaction::reconcile_project(&root)).is_err()
            );
            FileTransaction::reconcile_project(&root).unwrap();
            FileTransaction::reconcile_project(&root).unwrap();
            assert_eq!(std::fs::read(&target).unwrap(), b"before");
            assert_no_recovery_residue(&root, target.parent().unwrap());
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    fn assert_no_recovery_residue(root: &Path, target_parent: &Path) {
        assert!(std::fs::read_dir(target_parent).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("recovery-guard")));
        assert_eq!(
            std::fs::read_dir(root.join(".app/import-v2-journal"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn journals_without_recovery_phase_remain_backward_compatible() {
        let journal: super::Journal = serde_json::from_value(serde_json::json!({
            "state": "InProgress",
            "entries": [{
                "relative_path": "wiki/old.md",
                "previous": null,
                "desired_hash": digest_bytes(b"old"),
                "installed_identity": null
            }]
        }))
        .unwrap();
        assert!(journal.entries[0].recovery.is_none());
        assert!(journal.recovery_artifacts.is_empty());
    }

    #[test]
    fn resumed_overwrite_recovery_preserves_external_canonical_and_reports_durable_guard() {
        let root = std::env::temp_dir().join(format!(
            "import-v2-recovery-drift-overwrite-{}",
            uuid::Uuid::new_v4()
        ));
        let target = root.join("wiki/existing.md");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, b"before").unwrap();
        let mut transaction = FileTransaction::new_for_project(&root);
        transaction
            .write_if_hash_matches(&target, b"after", &digest_bytes(b"before"))
            .unwrap();
        transaction.simulate_process_crash();
        set_recovery_process_death_hook(|phase| phase == "after_quarantine");
        assert!(std::panic::catch_unwind(|| FileTransaction::reconcile_project(&root)).is_err());
        std::fs::write(&target, b"external").unwrap();

        let first = FileTransaction::reconcile_project(&root).unwrap_err();
        let second = FileTransaction::reconcile_project(&root).unwrap_err();
        assert_eq!(std::fs::read(&target).unwrap(), b"external");
        let first_details = first.details.unwrap().to_string();
        let second_details = second.details.unwrap().to_string();
        assert!(first_details.contains("wiki/.import-v2-recovery-guard-"));
        assert_eq!(first_details, second_details);
        let guard = std::fs::read_dir(target.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .find(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .contains("recovery-guard")
            })
            .unwrap()
            .path();
        assert_eq!(std::fs::read(guard).unwrap(), b"after");
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
        std::fs::create_dir_all(&root).unwrap();
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
    fn recovery_enumerates_the_bound_journal_directory_after_lexical_swap() {
        let root = std::env::temp_dir().join(format!(
            "import-v2-journal-bind-swap-{}",
            uuid::Uuid::new_v4()
        ));
        let displaced = std::env::temp_dir().join(format!(
            "import-v2-journal-bind-displaced-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let target = root.join("wiki/new.md");
        let mut transaction = FileTransaction::new_for_project(&root);
        transaction.write_new(&target, b"installed").unwrap();
        transaction.simulate_process_crash();

        let journal = root.join(".app/import-v2-journal");
        let hook_journal = journal.clone();
        let hook_displaced = displaced.clone();
        set_after_journal_directory_bind_hook(move || {
            std::fs::rename(&hook_journal, &hook_displaced).unwrap();
            std::fs::create_dir(&hook_journal).unwrap();
        });

        FileTransaction::reconcile_project(&root).unwrap();

        assert!(!target.exists());
        assert!(!std::fs::read_dir(&displaced)
            .unwrap()
            .any(|entry| entry.is_ok()));
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(displaced).unwrap();
    }

    #[test]
    fn recovery_rejects_unbound_artifact_before_touching_project_bytes() {
        let root = std::env::temp_dir().join(format!(
            "import-v2-malicious-artifact-{}",
            uuid::Uuid::new_v4()
        ));
        let journal_directory = root.join(".app/import-v2-journal");
        let immutable_source = root.join("raw/sources/evidence.tmp");
        std::fs::create_dir_all(&journal_directory).unwrap();
        std::fs::create_dir_all(immutable_source.parent().unwrap()).unwrap();
        std::fs::write(&immutable_source, b"immutable evidence").unwrap();
        let journal = Journal {
            state: JournalState::Committed,
            entries: Vec::new(),
            recovery_artifacts: vec!["raw/sources/evidence.tmp".to_string()],
        };
        std::fs::write(
            journal_directory.join("malicious.json"),
            serde_json::to_vec(&journal).unwrap(),
        )
        .unwrap();

        FileTransaction::reconcile_project(&root).unwrap_err();

        assert_eq!(
            std::fs::read(&immutable_source).unwrap(),
            b"immutable evidence"
        );
        std::fs::remove_dir_all(root).unwrap();
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
