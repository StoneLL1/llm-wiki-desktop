use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::errors::{BackendError, IMPORT_V2_COMMIT_CONFLICT, IMPORT_V2_COMMIT_FAILED};

#[cfg(test)]
thread_local! {
    static BEFORE_CHECKED_DISPLACE_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce(&Path)>>> =
        std::cell::RefCell::new(None);
    static FAIL_NEXT_CANDIDATE_INSTALL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static BEFORE_NEW_INSTALL_HOOK: std::cell::RefCell<Option<Box<dyn FnMut(&Path) -> bool>>> = std::cell::RefCell::new(None);
    static FAIL_NEXT_CLEANUP: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);
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
fn set_before_checked_displace_hook(hook: impl FnOnce(&Path) + 'static) {
    BEFORE_CHECKED_DISPLACE_HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

fn run_before_checked_displace_hook(path: &Path) {
    #[cfg(test)]
    BEFORE_CHECKED_DISPLACE_HOOK.with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook(path);
        }
    });
    #[cfg(not(test))]
    let _ = path;
}

pub struct FileTransaction {
    backups: Vec<(PathBuf, Option<Vec<u8>>)>,
    created_dirs: Vec<PathBuf>,
    recovery_artifacts: Vec<PathBuf>,
    created_hashes: std::collections::HashMap<PathBuf, String>,
    finished: bool,
}

impl FileTransaction {
    pub fn new() -> Self {
        Self {
            backups: Vec::new(),
            created_dirs: Vec::new(),
            recovery_artifacts: Vec::new(),
            created_hashes: std::collections::HashMap::new(),
            finished: false,
        }
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
        if was_absent {
            self.created_hashes
                .insert(path.to_path_buf(), hash_bytes(bytes));
        }
        Ok(())
    }

    pub fn write_new(&mut self, path: &Path, bytes: &[u8]) -> Result<(), BackendError> {
        self.ensure_parent(path)?;
        let parent = path.parent().unwrap();
        let temporary = write_synced_temp(parent, path, bytes)?;
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
                    vec![error.to_string(), cleanup.message],
                )),
            };
        }
        self.backups.push((path.to_path_buf(), None));
        self.created_hashes
            .insert(path.to_path_buf(), hash_bytes(bytes));
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
        if let Err(error) = std::fs::rename(path, &guard) {
            let _ = self.cleanup_artifact(&temporary);
            return Err(io_error(error, path));
        }
        self.recovery_artifacts.push(guard.clone());
        let previous = match std::fs::read(&guard) {
            Ok(bytes) => bytes,
            Err(error) => {
                let primary = io_error(error, &guard);
                let restore = restore_displaced(&guard, path);
                let _ = self.cleanup_artifact(&temporary);
                return match restore {
                    Ok(()) => Err(primary),
                    Err(rollback) => Err(rollback_failure(
                        "Displaced Wiki could not be read or restored.",
                        vec![primary.message, rollback.message],
                    )),
                };
            }
        };
        if format!("{:x}", Sha256::digest(&previous)) != expected_hash {
            let restore = restore_displaced(&guard, path);
            if restore.is_ok() {
                self.recovery_artifacts
                    .retain(|candidate| candidate != &guard);
            }
            let _ = self.cleanup_artifact(&temporary);
            return match restore {
                Ok(()) => Err(BackendError::new(
                    IMPORT_V2_COMMIT_CONFLICT,
                    "Wiki changed after preview.",
                    true,
                    true,
                )),
                Err(rollback) => Err(rollback_failure(
                    "Wiki changed and its displaced file could not be restored.",
                    vec![rollback.message],
                )),
            };
        }
        if let Err(error) = install_candidate(&temporary, path) {
            let _ = self.cleanup_artifact(&temporary);
            let restore = restore_displaced(&guard, path);
            if restore.is_ok() {
                self.recovery_artifacts
                    .retain(|candidate| candidate != &guard);
            }
            return match restore {
                Ok(()) => Err(io_error(error, path)),
                Err(rollback) => Err(rollback_failure(
                    "Checked Wiki replacement failed and recovery was incomplete.",
                    vec![error.to_string(), rollback.message],
                )),
            };
        }
        self.backups.push((path.to_path_buf(), Some(previous)));
        self.cleanup_artifact(&temporary)?;
        self.cleanup_artifact(&guard)
    }

    pub fn commit(mut self) {
        self.finished = true;
    }

    pub(super) fn rollback(&mut self) -> Result<(), BackendError> {
        let mut failures = Vec::new();
        for (path, previous) in self.backups.iter().rev() {
            match previous {
                Some(bytes) => {
                    if let Err(error) = write_atomic_bytes(path, bytes) {
                        failures.push(format!("{}: {}", path.display(), error.message));
                    }
                }
                None => {
                    if path.exists() {
                        let current = std::fs::read(path).ok().map(|bytes| hash_bytes(&bytes));
                        if current != self.created_hashes.get(path).cloned() {
                            failures.push(format!(
                                "{}: destination changed externally; preserved instead of deleting",
                                path.display()
                            ));
                            continue;
                        }
                    }
                    if let Err(error) = std::fs::remove_file(path) {
                        if error.kind() != std::io::ErrorKind::NotFound {
                            failures.push(format!("{}: {error}", path.display()));
                        }
                    }
                }
            }
        }
        for artifact in self.recovery_artifacts.clone().iter().rev() {
            if let Err(error) = self.cleanup_artifact(artifact) {
                failures.push(format!("{}: {}", artifact.display(), error.message));
            }
        }
        for directory in self.created_dirs.iter().rev() {
            if let Err(error) = std::fs::remove_dir(directory) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    failures.push(format!("{}: {error}", directory.display()));
                }
            }
        }
        self.finished = true;
        if failures.is_empty() {
            Ok(())
        } else {
            Err(rollback_failure(
                "Import rollback was incomplete.",
                failures,
            ))
        }
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
            std::fs::remove_file(path).map_err(|error| io_error(error, path))?;
        }
        std::fs::rename(&temporary, path).map_err(|error| io_error(error, path))
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

fn restore_displaced(guard: &Path, path: &Path) -> Result<(), BackendError> {
    let mut target_created = false;
    let result = (|| {
        let mut source = std::fs::File::open(guard).map_err(|error| io_error(error, guard))?;
        let mut target = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| io_error(error, path))?;
        target_created = true;
        std::io::copy(&mut source, &mut target).map_err(|error| io_error(error, path))?;
        target.sync_all().map_err(|error| io_error(error, path))?;
        Ok(())
    })();
    if let Err(error) = result {
        if target_created {
            let _ = std::fs::remove_file(path);
        }
        return Err(error);
    }
    std::fs::remove_file(guard).map_err(|error| io_error(error, guard))
}

fn install_candidate(temporary: &Path, path: &Path) -> Result<(), std::io::Error> {
    #[cfg(test)]
    if FAIL_NEXT_CANDIDATE_INSTALL.with(|flag| flag.replace(false)) {
        return Err(std::io::Error::other("injected candidate install failure"));
    }
    std::fs::hard_link(temporary, path)
}

fn rollback_failure(message: &str, failures: Vec<String>) -> BackendError {
    BackendError::new(IMPORT_V2_COMMIT_FAILED, message, false, true)
        .with_details(serde_json::json!({ "rollbackFailures": failures }))
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn io_error(error: std::io::Error, path: &Path) -> BackendError {
    BackendError::new("FILE_WRITE_FAILED", error.to_string(), true, false)
        .with_details(serde_json::json!({ "path": path.to_string_lossy() }))
}

#[cfg(test)]
mod tests {
    use super::{
        set_before_checked_displace_hook, set_before_new_install_hook,
        set_fail_next_candidate_install, set_fail_next_cleanup, FileTransaction,
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
        transaction.commit();
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
        set_before_checked_displace_hook(|path| std::fs::write(path, b"external edit").unwrap());
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
        transaction.rollback().unwrap();
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
        transaction.write_new(&path, b"candidate").unwrap_err();
        transaction.rollback().unwrap();
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
}
