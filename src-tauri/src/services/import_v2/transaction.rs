use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::errors::{BackendError, IMPORT_V2_COMMIT_CONFLICT};

pub struct FileTransaction {
    backups: Vec<(PathBuf, Option<Vec<u8>>)>,
    created_dirs: Vec<PathBuf>,
    committed: bool,
}

impl FileTransaction {
    pub fn new() -> Self {
        Self {
            backups: Vec::new(),
            created_dirs: Vec::new(),
            committed: false,
        }
    }

    pub fn write(&mut self, path: &Path, bytes: &[u8]) -> Result<(), BackendError> {
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
        write_atomic_bytes(path, bytes)
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
        let current = std::fs::read(path).map_err(|error| io_error(error, path))?;
        if format!("{:x}", Sha256::digest(&current)) != expected_hash {
            return Err(BackendError::new(
                IMPORT_V2_COMMIT_CONFLICT,
                "Wiki changed after preview.",
                true,
                true,
            ));
        }
        self.write(path, bytes)
    }

    pub fn commit(mut self) {
        self.committed = true;
    }

    fn rollback(&mut self) {
        for (path, previous) in self.backups.iter().rev() {
            match previous {
                Some(bytes) => {
                    let _ = write_atomic_bytes(path, bytes);
                }
                None => {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
        for directory in self.created_dirs.iter().rev() {
            let _ = std::fs::remove_dir(directory);
        }
    }
}

impl Drop for FileTransaction {
    fn drop(&mut self) {
        if !self.committed {
            self.rollback();
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

fn io_error(error: std::io::Error, path: &Path) -> BackendError {
    BackendError::new("FILE_WRITE_FAILED", error.to_string(), true, false)
        .with_details(serde_json::json!({ "path": path.to_string_lossy() }))
}

#[cfg(test)]
mod tests {
    use super::FileTransaction;

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
}
