use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::errors::BackendError;
use crate::models::layout::is_link_or_reparse;
use crate::models::paths::ProjectContext;
use crate::utils::path_utils::normalize_project_path;
use crate::utils::safe_project_dir::{BoundFileIdentity, BoundProjectMutationRoot};

#[derive(Default)]
pub struct FileStore;

static GUARDED_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[cfg(test)]
thread_local! {
    static BEFORE_CHECKED_WRITE_PUBLISH: std::cell::RefCell<Option<Box<dyn Fn(&Path)>>> =
        std::cell::RefCell::new(None);
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "kind", content = "hash")]
pub enum WriteMode {
    CreateNew,
    OverwriteIfHashMatches(String),
}

impl FileStore {
    pub fn exists(&self, context: &ProjectContext, relative_path: &str) -> bool {
        context
            .resolve_project_path(relative_path)
            .map(|path| path.exists())
            .unwrap_or(false)
    }

    pub fn ensure_dir(
        &self,
        context: &ProjectContext,
        relative_path: &str,
    ) -> Result<(), BackendError> {
        let path = context.resolve_project_write_directory(relative_path)?;
        BoundProjectMutationRoot::ensure_and_bind(
            &context.root,
            &path.join(".wiki-directory-binding-probe"),
        )
        .map(|_| ())
        .map_err(|error| {
            BackendError::new("FILE_DIR_CREATE_FAILED", error.to_string(), true, true)
                .with_details(serde_json::json!({ "path": path.to_string_lossy() }))
        })
    }

    pub fn ensure_absolute_dir(&self, root: &Path, path: &Path) -> Result<(), BackendError> {
        BoundProjectMutationRoot::ensure_and_bind(root, &path.join(".wiki-directory-binding-probe"))
            .map(|_| ())
            .map_err(|error| io_error("FILE_DIR_CREATE_FAILED", error, path))
    }

    pub fn read_markdown(
        &self,
        context: &ProjectContext,
        relative_path: &str,
    ) -> Result<String, BackendError> {
        let path = context.resolve_project_path(relative_path)?;
        fs::read_to_string(&path).map_err(|err| io_error("FILE_READ_FAILED", err, &path))
    }

    pub(crate) fn read_bytes(
        &self,
        context: &ProjectContext,
        relative_path: &str,
    ) -> Result<Vec<u8>, BackendError> {
        let path = context.resolve_project_path(relative_path)?;
        fs::read(&path).map_err(|err| io_error("FILE_READ_FAILED", err, &path))
    }

    pub(crate) fn content_hash(&self, bytes: &[u8]) -> String {
        hash_bytes(bytes)
    }

    pub fn write_markdown(
        &self,
        context: &ProjectContext,
        relative_path: &str,
        contents: &str,
    ) -> Result<(), BackendError> {
        let path = context.resolve_project_write_path(relative_path)?;
        let binding = bind_project_write(context, &path, true)?;
        write_bound_atomic(&binding, &path, contents.as_bytes(), None)
    }

    pub fn write_markdown_checked(
        &self,
        context: &ProjectContext,
        relative_path: &str,
        contents: &str,
        mode: WriteMode,
    ) -> Result<(), BackendError> {
        let _guard = GUARDED_WRITE_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .map_err(|_| {
                BackendError::new(
                    "FILE_WRITE_LOCK_FAILED",
                    "Another guarded file write is currently running.",
                    true,
                    false,
                )
            })?;
        let path = context.resolve_project_write_path(relative_path)?;
        let ensure_parent = matches!(mode, WriteMode::CreateNew);
        let binding = bind_project_write(context, &path, ensure_parent)?;
        let expected_identity =
            self.verify_write_mode(&binding, &path, relative_path, mode.clone())?;
        #[cfg(test)]
        BEFORE_CHECKED_WRITE_PUBLISH.with(|slot| {
            if let Some(hook) = slot.borrow_mut().take() {
                hook(&path);
            }
        });
        write_bound_atomic(&binding, &path, contents.as_bytes(), expected_identity)?;
        // The hash check and atomic rename cannot form an OS-level compare-and
        // swap for edits made by an external editor. Verify the postcondition
        // immediately so a racing replacement is reported and never enters
        // the Lint verification/rollback path as if our content were present.
        if let WriteMode::OverwriteIfHashMatches(expected_hash) = mode {
            let expected_new_hash = hash_bytes(contents.as_bytes());
            let actual_hash = hash_bound_file(&binding, &path)?;
            if actual_hash != expected_new_hash {
                return Err(BackendError::new(
                    "FILE_CHANGED_DURING_WRITE",
                    "The file changed while the guarded write was completing; no rollback was attempted.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({
                    "path": relative_path,
                    "expectedHash": expected_hash,
                    "writtenHash": expected_new_hash,
                    "actualHash": actual_hash,
                })));
            }
        }
        Ok(())
    }

    /// Validate an overwrite token before a potentially expensive or
    /// fallible safety checkpoint. This is only a preflight: callers must
    /// still use `write_markdown_checked` with the same token so the final
    /// mutation retains its identity-and-hash compare-and-swap guard.
    pub(crate) fn preflight_markdown_overwrite_hash(
        &self,
        context: &ProjectContext,
        relative_path: &str,
        expected_hash: &str,
    ) -> Result<(), BackendError> {
        let path = context.resolve_project_write_path(relative_path)?;
        let binding = BoundProjectMutationRoot::bind_read(&context.root, &path)
            .map_err(|error| io_error("FILE_READ_FAILED", error, &path))?;
        self.verify_write_mode(
            &binding,
            &path,
            relative_path,
            WriteMode::OverwriteIfHashMatches(expected_hash.to_string()),
        )?;
        Ok(())
    }

    pub fn write_markdown_create_new_atomic(
        &self,
        context: &ProjectContext,
        relative_path: &str,
        contents: &str,
    ) -> Result<(), BackendError> {
        let _guard = GUARDED_WRITE_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .map_err(|_| {
                BackendError::new(
                    "FILE_WRITE_LOCK_FAILED",
                    "Another guarded file write is currently running.",
                    true,
                    false,
                )
            })?;
        let path = context.resolve_project_write_path(relative_path)?;
        let binding = bind_project_write(context, &path, true)?;
        self.verify_write_mode(&binding, &path, relative_path, WriteMode::CreateNew)?;
        write_bound_create_new(&binding, &path, relative_path, contents.as_bytes())
    }

    pub fn write_text_absolute(
        &self,
        root: &Path,
        path: &Path,
        contents: &str,
    ) -> Result<(), BackendError> {
        let binding = bind_absolute_write(root, path)?;
        write_bound_atomic(&binding, path, contents.as_bytes(), None)
    }

    pub(crate) fn write_project_bytes_absolute(
        &self,
        context: &ProjectContext,
        path: &Path,
        bytes: &[u8],
    ) -> Result<(), BackendError> {
        let binding = bind_project_write(context, path, true)?;
        write_bound_atomic(&binding, path, bytes, None)
    }

    pub(crate) fn write_project_bytes_absolute_if_hash_matches(
        &self,
        context: &ProjectContext,
        path: &Path,
        bytes: &[u8],
        expected_hash: &str,
    ) -> Result<bool, BackendError> {
        let binding = match BoundProjectMutationRoot::bind(&context.root, path) {
            Ok(binding) => binding,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(io_error("FILE_READ_FAILED", error, path)),
        };
        let (current, identity) = match binding.read_regular_with_identity(path) {
            Ok(result) => result,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(io_error("FILE_READ_FAILED", error, path)),
        };
        if hash_bytes(&current) != expected_hash {
            return Ok(false);
        }
        write_bound_atomic(
            &binding,
            path,
            bytes,
            Some((identity, expected_hash.to_string())),
        )?;
        Ok(true)
    }

    pub(crate) fn write_bytes_create_new_absolute(
        &self,
        root: &Path,
        path: &Path,
        bytes: &[u8],
    ) -> Result<(), BackendError> {
        let binding = bind_absolute_write(root, path)?;
        binding
            .write_atomic_create_new(path, bytes)
            .map_err(|error| io_error("FILE_WRITE_FAILED", error, path))
    }

    pub fn read_json<T: serde::de::DeserializeOwned>(
        &self,
        context: &ProjectContext,
        relative_path: &str,
    ) -> Result<T, BackendError> {
        let raw = self.read_markdown(context, relative_path)?;
        serde_json::from_str(&raw).map_err(|err| {
            BackendError::new("JSON_PARSE_FAILED", err.to_string(), true, false)
                .with_details(serde_json::json!({ "path": relative_path }))
        })
    }

    pub fn read_json_file<T: serde::de::DeserializeOwned>(
        &self,
        path: &Path,
    ) -> Result<T, BackendError> {
        let raw =
            fs::read_to_string(path).map_err(|err| io_error("FILE_READ_FAILED", err, path))?;
        serde_json::from_str(&raw).map_err(|err| {
            BackendError::new("JSON_PARSE_FAILED", err.to_string(), true, false)
                .with_details(serde_json::json!({ "path": path.to_string_lossy() }))
        })
    }

    pub fn write_json_atomic<T: serde::Serialize>(
        &self,
        context: &ProjectContext,
        relative_path: &str,
        value: &T,
    ) -> Result<(), BackendError> {
        let _guard = GUARDED_WRITE_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .map_err(|_| {
                BackendError::new(
                    "FILE_WRITE_LOCK_FAILED",
                    "Another guarded file write is currently running.",
                    true,
                    false,
                )
            })?;
        let path = context.resolve_project_write_path(relative_path)?;
        let serialized = serde_json::to_string_pretty(value).map_err(|err| {
            BackendError::new("JSON_SERIALIZE_FAILED", err.to_string(), true, false)
        })?;
        let binding = bind_project_write(context, &path, true)?;
        write_bound_atomic(&binding, &path, serialized.as_bytes(), None)
    }

    pub fn write_json_atomic_checked<T: serde::Serialize>(
        &self,
        context: &ProjectContext,
        relative_path: &str,
        value: &T,
        mode: WriteMode,
    ) -> Result<(), BackendError> {
        let _guard = GUARDED_WRITE_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .map_err(|_| {
                BackendError::new(
                    "FILE_WRITE_LOCK_FAILED",
                    "Another guarded file write is currently running.",
                    true,
                    false,
                )
            })?;
        let path = context.resolve_project_write_path(relative_path)?;
        let ensure_parent = matches!(mode, WriteMode::CreateNew);
        let binding = bind_project_write(context, &path, ensure_parent)?;
        let expected_identity = self.verify_write_mode(&binding, &path, relative_path, mode)?;
        let serialized = serde_json::to_string_pretty(value).map_err(|err| {
            BackendError::new("JSON_SERIALIZE_FAILED", err.to_string(), true, false)
        })?;
        write_bound_atomic(&binding, &path, serialized.as_bytes(), expected_identity)
    }

    pub fn write_json_atomic_absolute<T: serde::Serialize>(
        &self,
        root: &Path,
        path: &Path,
        value: &T,
    ) -> Result<(), BackendError> {
        let serialized = serde_json::to_string_pretty(value).map_err(|err| {
            BackendError::new("JSON_SERIALIZE_FAILED", err.to_string(), true, false)
        })?;
        let binding = bind_absolute_write(root, path)?;
        write_bound_atomic(&binding, path, serialized.as_bytes(), None)
    }

    pub fn list_markdown_files(&self, root: &Path) -> Result<Vec<PathBuf>, BackendError> {
        let mut results = Vec::new();
        if !root.exists() {
            return Ok(results);
        }
        walk_markdown(root, &mut results)
            .map_err(|err| io_error("FILE_ENUMERATE_FAILED", err, root))?;
        results.sort();
        Ok(results)
    }

    pub fn file_hash(
        &self,
        context: &ProjectContext,
        relative_path: &str,
    ) -> Result<String, BackendError> {
        let path = context.resolve_project_path(relative_path)?;
        let binding = BoundProjectMutationRoot::bind_read(&context.root, &path)
            .map_err(|error| io_error("FILE_READ_FAILED", error, &path))?;
        hash_bound_file(&binding, &path)
    }

    pub fn file_hash_if_exists(
        &self,
        context: &ProjectContext,
        relative_path: &str,
    ) -> Result<Option<String>, BackendError> {
        let path = context.resolve_project_path(relative_path)?;
        let binding = match BoundProjectMutationRoot::bind_read(&context.root, &path) {
            Ok(binding) => binding,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error("FILE_READ_FAILED", error, &path)),
        };
        match binding.read_regular(&path) {
            Ok(bytes) => Ok(Some(hash_bytes(&bytes))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(io_error("FILE_READ_FAILED", error, &path)),
        }
    }

    pub(crate) fn remove_if_hash_matches(
        &self,
        context: &ProjectContext,
        relative_path: &str,
        expected_hash: &str,
    ) -> Result<bool, BackendError> {
        let path = context.resolve_project_write_path(relative_path)?;
        let binding = match BoundProjectMutationRoot::bind(&context.root, &path) {
            Ok(binding) => binding,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(io_error("FILE_READ_FAILED", error, &path)),
        };
        let (bytes, identity) = match binding.read_regular_with_identity(&path) {
            Ok(result) => result,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(io_error("FILE_READ_FAILED", error, &path)),
        };
        if hash_bytes(&bytes) != expected_hash {
            return Ok(false);
        }
        binding
            .remove_file_if_identity_and_hash(&path, identity, expected_hash)
            .map_err(|error| io_error("FILE_WRITE_FAILED", error, &path))?;
        Ok(true)
    }

    pub fn assert_unique_project_paths(&self, paths: &[&str]) -> Result<(), BackendError> {
        let mut seen = HashSet::new();
        for path in paths {
            let normalized = normalize_project_path(path).to_ascii_lowercase();
            if !seen.insert(normalized.clone()) {
                return Err(BackendError::new(
                    "FILE_DUPLICATE_PATH",
                    "The same project path was provided more than once.",
                    false,
                    true,
                )
                .with_details(serde_json::json!({ "path": normalized })));
            }
        }
        Ok(())
    }

    fn verify_write_mode(
        &self,
        binding: &BoundProjectMutationRoot,
        path: &Path,
        relative_path: &str,
        mode: WriteMode,
    ) -> Result<Option<(BoundFileIdentity, String)>, BackendError> {
        match mode {
            WriteMode::CreateNew => match binding.file_identity(path) {
                Ok(_) => Err(BackendError::new(
                    "FILE_ALREADY_EXISTS",
                    "File already exists and cannot be overwritten without an explicit hash match.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({ "path": relative_path }))),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(io_error("FILE_READ_FAILED", error, path)),
            },
            WriteMode::OverwriteIfHashMatches(expected_hash) => {
                let (bytes, identity) = match binding.read_regular_with_identity(path) {
                    Ok(result) => result,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        return Err(BackendError::new(
                            "FILE_NOT_FOUND",
                            "Cannot overwrite a missing file.",
                            true,
                            true,
                        )
                        .with_details(serde_json::json!({ "path": relative_path })))
                    }
                    Err(error) => return Err(io_error("FILE_READ_FAILED", error, path)),
                };
                let current_hash = hash_bytes(&bytes);
                if current_hash != expected_hash {
                    // Surface the on-disk baseline text so the frontend can run
                    // a 3-way diff (baseline disk + editor buffer + generated)
                    // instead of forcing a blind reload. Both the wiki save and
                    // compile conflict paths flow through here, so this one
                    // change covers PRD-READ-004 (external edits) and
                    // PRD-WIKI-004 (compile merge conflicts). Only include the
                    // baseline when the file is valid UTF-8: from_utf8_lossy
                    // would otherwise produce U+FFFD garbage for binary files,
                    // mislead the diff, and possibly crash the editor.
                    let baseline_content = String::from_utf8(bytes).ok();
                    let mut details = serde_json::json!({
                        "path": relative_path,
                        "expectedHash": expected_hash,
                        "currentHash": current_hash,
                    });
                    if let Some(baseline) = baseline_content {
                        details["baselineContent"] = serde_json::Value::String(baseline);
                    }
                    return Err(BackendError::new(
                        "FILE_HASH_MISMATCH",
                        "File changed since it was last read. Reload before overwriting.",
                        true,
                        true,
                    )
                    .with_details(details));
                }
                Ok(Some((identity, current_hash)))
            }
        }
    }
}

fn hash_bound_file(
    binding: &BoundProjectMutationRoot,
    path: &Path,
) -> Result<String, BackendError> {
    let bytes = binding
        .read_regular(path)
        .map_err(|err| io_error("FILE_READ_FAILED", err, path))?;
    Ok(hash_bytes(&bytes))
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn bind_project_write(
    context: &ProjectContext,
    path: &Path,
    ensure_parent: bool,
) -> Result<BoundProjectMutationRoot, BackendError> {
    let result = if ensure_parent {
        BoundProjectMutationRoot::ensure_and_bind(&context.root, path).map(|(binding, _)| binding)
    } else {
        BoundProjectMutationRoot::bind(&context.root, path)
    };
    result.map_err(|error| io_error("FILE_WRITE_FAILED", error, path))
}

fn bind_absolute_write(root: &Path, path: &Path) -> Result<BoundProjectMutationRoot, BackendError> {
    BoundProjectMutationRoot::ensure_and_bind(root, path)
        .map(|(binding, _)| binding)
        .map_err(|error| io_error("FILE_WRITE_FAILED", error, path))
}

fn write_bound_atomic(
    binding: &BoundProjectMutationRoot,
    path: &Path,
    bytes: &[u8],
    expected: Option<(BoundFileIdentity, String)>,
) -> Result<(), BackendError> {
    if let Some((expected_identity, expected_hash)) = expected {
        let temporary = binding
            .write_synced_temp(path, bytes)
            .map_err(|error| io_error("FILE_WRITE_FAILED", error, path))?;
        let publish = binding.replace_existing_if_identity_and_hash(
            &temporary,
            path,
            expected_identity,
            &expected_hash,
        );
        match publish {
            Ok(()) => binding
                .sync()
                .map_err(|error| io_error("FILE_WRITE_FAILED", error, path)),
            Err(error) => {
                let _ = binding.remove_file(&temporary);
                if error.kind() == std::io::ErrorKind::WouldBlock {
                    Err(final_hash_mismatch(binding, path, &expected_hash))
                } else {
                    Err(io_error("FILE_WRITE_FAILED", error, path))
                }
            }
        }
    } else {
        binding
            .write_atomic_replace(path, bytes)
            .map_err(|error| io_error("FILE_WRITE_FAILED", error, path))
    }
}

fn final_hash_mismatch(
    binding: &BoundProjectMutationRoot,
    path: &Path,
    expected_hash: &str,
) -> BackendError {
    let current = binding.read_regular(path).ok();
    let current_hash = current.as_deref().map(hash_bytes);
    let mut details = serde_json::json!({
        "path": path.to_string_lossy(),
        "expectedHash": expected_hash,
        "currentHash": current_hash,
    });
    if let Some(baseline) = current.and_then(|bytes| String::from_utf8(bytes).ok()) {
        details["baselineContent"] = serde_json::Value::String(baseline);
    }
    BackendError::new(
        "FILE_HASH_MISMATCH",
        "File changed during the final guarded publish. Reload before overwriting.",
        true,
        true,
    )
    .with_details(details)
}

fn write_bound_create_new(
    binding: &BoundProjectMutationRoot,
    path: &Path,
    relative_path: &str,
    bytes: &[u8],
) -> Result<(), BackendError> {
    binding
        .write_atomic_create_new(path, bytes)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                BackendError::new(
                    "FILE_ALREADY_EXISTS",
                    "File already exists and cannot be overwritten without an explicit hash match.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({ "path": relative_path }))
            } else {
                io_error("FILE_WRITE_FAILED", error, path)
            }
        })
}

fn walk_markdown(current: &Path, results: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if is_link_or_reparse(&metadata) {
            continue;
        }
        if metadata.is_dir() {
            let name = entry.file_name();
            // Skip Obsidian and hidden app state when enumerating wiki content.
            if name == ".obsidian" || name == ".git" || name == ".app" {
                continue;
            }
            walk_markdown(&path, results)?;
        } else if metadata.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("md")
        {
            results.push(path);
        }
    }
    Ok(())
}

fn io_error(code: &str, err: std::io::Error, path: &Path) -> BackendError {
    BackendError::new(code, err.to_string(), true, false)
        .with_details(serde_json::json!({ "path": path.to_string_lossy() }))
}

#[cfg(test)]
mod tests {
    use super::{FileStore, WriteMode};
    use crate::models::paths::ProjectContext;
    use serde::{Deserialize, Serialize};
    use std::path::PathBuf;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Sample {
        name: String,
        count: u32,
    }

    fn tmp_context(suffix: &str) -> (ProjectContext, PathBuf) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("llm-wiki-filestore-{stamp}-{suffix}"));
        std::fs::create_dir_all(&root).unwrap();
        (ProjectContext::new("project-1", root.clone()), root)
    }

    #[test]
    fn writes_and_reads_markdown_preserving_unicode() {
        let (context, root) = tmp_context("md");
        let store = FileStore;

        store
            .write_markdown(&context, "wiki/概念/Agent.md", "# 智能体\n正文 [[link]]")
            .unwrap();

        let read = store.read_markdown(&context, "wiki/概念/Agent.md").unwrap();
        assert_eq!(read, "# 智能体\n正文 [[link]]");
        assert!(context.wiki_dir.join("概念").join("Agent.md").exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn writes_json_atomically_and_reads_back() {
        let (context, root) = tmp_context("json");
        let store = FileStore;

        store
            .write_json_atomic(
                &context,
                ".app/settings.json",
                &Sample {
                    name: "zh".into(),
                    count: 7,
                },
            )
            .unwrap();

        let back: Sample = store.read_json(&context, ".app/settings.json").unwrap();
        assert_eq!(
            back,
            Sample {
                name: "zh".into(),
                count: 7
            }
        );
        // No leftover temp files.
        let entries =
            std::fs::read_dir(context.app_dir.join("settings.json").parent().unwrap()).unwrap();
        for entry in entries {
            let name = entry.unwrap().file_name();
            assert!(
                !name.to_string_lossy().contains(".tmp"),
                "temp file leaked: {name:?}"
            );
        }

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn json_writes_reject_linked_state_directories() {
        let (context, root) = tmp_context("json-linked-state");
        let store = FileStore;
        let outside = tempfile::tempdir().unwrap();
        create_directory_link(outside.path(), &context.app_dir).unwrap();

        let err = store
            .write_json_atomic(
                &context,
                ".app/settings.json",
                &Sample {
                    name: "unsafe".into(),
                    count: 1,
                },
            )
            .expect_err("linked app state must not be writable");
        assert_eq!(err.code, "PATH_OUTSIDE_PROJECT");
        assert!(!outside.path().join("settings.json").exists());

        remove_directory_link(&context.app_dir);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_path_traversal_on_write() {
        let (context, root) = tmp_context("traversal");
        let store = FileStore;

        let err = store
            .write_markdown(&context, "../outside.md", "stolen")
            .expect_err("traversal must be rejected");
        assert_eq!(err.code, "PATH_TRAVERSAL");
        assert!(!root.parent().unwrap().join("outside.md").exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn safe_markdown_write_rejects_silent_overwrite_without_matching_hash() {
        let (context, root) = tmp_context("overwrite-md");
        let store = FileStore;

        store
            .write_markdown_checked(
                &context,
                "wiki/notes/agent.md",
                "# First",
                WriteMode::CreateNew,
            )
            .unwrap();

        let err = store
            .write_markdown_checked(
                &context,
                "wiki/notes/agent.md",
                "# Second",
                WriteMode::CreateNew,
            )
            .expect_err("existing files require an explicit overwrite mode");
        assert_eq!(err.code, "FILE_ALREADY_EXISTS");
        assert_eq!(
            store
                .read_markdown(&context, "wiki/notes/agent.md")
                .unwrap(),
            "# First"
        );

        let stale_hash = "stale-hash".to_string();
        let err = store
            .write_markdown_checked(
                &context,
                "wiki/notes/agent.md",
                "# Second",
                WriteMode::OverwriteIfHashMatches(stale_hash),
            )
            .expect_err("stale overwrite hashes must be rejected");
        assert_eq!(err.code, "FILE_HASH_MISMATCH");

        let current_hash = store.file_hash(&context, "wiki/notes/agent.md").unwrap();
        store
            .write_markdown_checked(
                &context,
                "wiki/notes/agent.md",
                "# Second",
                WriteMode::OverwriteIfHashMatches(current_hash),
            )
            .unwrap();
        assert_eq!(
            store
                .read_markdown(&context, "wiki/notes/agent.md")
                .unwrap(),
            "# Second"
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn final_checked_publish_race_is_reported_as_hash_mismatch() {
        let (context, root) = tmp_context("final-cas-conflict");
        let store = FileStore;
        let relative = "wiki/notes/race.md";
        store
            .write_markdown_checked(&context, relative, "before", WriteMode::CreateNew)
            .unwrap();
        let expected_hash = store.file_hash(&context, relative).unwrap();
        super::BEFORE_CHECKED_WRITE_PUBLISH.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(|path| {
                std::fs::remove_file(path).unwrap();
                std::fs::write(path, b"external replacement").unwrap();
            }));
        });

        let error = store
            .write_markdown_checked(
                &context,
                relative,
                "candidate",
                WriteMode::OverwriteIfHashMatches(expected_hash),
            )
            .expect_err("the final namespace CAS must reject an external replacement");

        assert_eq!(error.code, "FILE_HASH_MISMATCH");
        assert_eq!(
            std::fs::read_to_string(root.join(relative)).unwrap(),
            "external replacement"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn concurrent_create_new_publishes_exactly_one_complete_file() {
        let (context, root) = tmp_context("create-new-race");
        let context = std::sync::Arc::new(context);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let mut handles = Vec::new();
        for contents in ["first-complete", "second-complete"] {
            let context = context.clone();
            let barrier = barrier.clone();
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                FileStore.write_markdown_create_new_atomic(
                    &context,
                    "exports/html/race.html",
                    contents,
                )
            }));
        }
        barrier.wait();
        let results = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter_map(|result| result.as_ref().err())
                .map(|error| error.code.as_str())
                .collect::<Vec<_>>(),
            vec!["FILE_ALREADY_EXISTS"]
        );
        let published = FileStore
            .read_markdown(&context, "exports/html/race.html")
            .unwrap();
        assert!(matches!(
            published.as_str(),
            "first-complete" | "second-complete"
        ));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn file_hash_mismatch_surfaces_disk_baseline_for_three_way_diff() {
        // PRD-READ-004 / PRD-WIKI-004: on a stale-hash overwrite the error
        // must carry the on-disk baseline text so the frontend can render a
        // 3-way diff (disk baseline + editor buffer + generated). Covers both
        // the wiki save and compile conflict paths, which both flow through
        // verify_write_mode.
        let (context, root) = tmp_context("baseline");
        let store = FileStore;

        store
            .write_markdown_checked(
                &context,
                "wiki/notes/agent.md",
                "# Agent\n\nOriginal body.",
                WriteMode::CreateNew,
            )
            .unwrap();

        let err = store
            .write_markdown_checked(
                &context,
                "wiki/notes/agent.md",
                "# Agent\n\nIncoming overwrite.",
                WriteMode::OverwriteIfHashMatches("stale-hash".to_string()),
            )
            .expect_err("stale hash must surface FILE_HASH_MISMATCH");
        assert_eq!(err.code, "FILE_HASH_MISMATCH");
        let details = err.details.expect("mismatch must carry details");
        assert_eq!(details["expectedHash"], "stale-hash");
        assert_eq!(
            details["baselineContent"], "# Agent\n\nOriginal body.",
            "baselineContent must equal the on-disk text"
        );
        // currentHash must still be present alongside the baseline.
        assert!(details.get("currentHash").is_some());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn checked_cleanup_only_removes_the_expected_complete_file() {
        let (context, root) = tmp_context("checked-cleanup");
        let store = FileStore;
        store
            .write_markdown(&context, "exports/generated.md", "generated")
            .unwrap();

        assert!(!store
            .remove_if_hash_matches(&context, "exports/generated.md", "stale")
            .unwrap());
        assert_eq!(
            store
                .read_markdown(&context, "exports/generated.md")
                .unwrap(),
            "generated"
        );
        let expected = store.file_hash(&context, "exports/generated.md").unwrap();
        assert!(store
            .remove_if_hash_matches(&context, "exports/generated.md", &expected)
            .unwrap());
        assert!(!context
            .resolve_project_path("exports/generated.md")
            .unwrap()
            .exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn safe_json_write_rejects_duplicate_paths_after_normalization() {
        let (context, root) = tmp_context("duplicate-json");
        let store = FileStore;

        let err = store
            .assert_unique_project_paths(&[".app/settings.json", ".app\\settings.json"])
            .expect_err("normalized duplicate paths must be rejected");
        assert_eq!(err.code, "FILE_DUPLICATE_PATH");

        store
            .write_json_atomic_checked(
                &context,
                ".app/settings.json",
                &Sample {
                    name: "zh".into(),
                    count: 1,
                },
                WriteMode::CreateNew,
            )
            .unwrap();
        let current_hash = store.file_hash(&context, ".app/settings.json").unwrap();
        store
            .write_json_atomic_checked(
                &context,
                ".app/settings.json",
                &Sample {
                    name: "en".into(),
                    count: 2,
                },
                WriteMode::OverwriteIfHashMatches(current_hash),
            )
            .unwrap();
        let back: Sample = store.read_json(&context, ".app/settings.json").unwrap();
        assert_eq!(
            back,
            Sample {
                name: "en".into(),
                count: 2
            }
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn enumerates_markdown_skipping_obsidian_and_app_state() {
        let (context, root) = tmp_context("enum");
        let store = FileStore;

        store
            .write_markdown(&context, "wiki/concepts/agent.md", "a")
            .unwrap();
        store
            .write_markdown(&context, "wiki/index.md", "i")
            .unwrap();
        store
            .write_markdown(&context, "wiki/.obsidian/app.md", "obsidian")
            .unwrap();
        std::fs::create_dir_all(root.join("wiki/.obsidian")).ok();
        std::fs::write(root.join("wiki/.obsidian/app.md"), "obsidian").unwrap();
        std::fs::create_dir_all(context.app_dir.join("tasks")).unwrap();
        std::fs::write(context.app_dir.join("tasks").join("task-1.md"), "log").unwrap();

        let files = store.list_markdown_files(&context.wiki_dir).unwrap();
        let relative: Vec<String> = files
            .iter()
            .map(|p| {
                p.strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();

        assert!(relative.contains(&"wiki/concepts/agent.md".to_string()));
        assert!(relative.contains(&"wiki/index.md".to_string()));
        assert!(!relative.iter().any(|p| p.contains(".obsidian")));
        assert!(!relative.iter().any(|p| p.contains(".app")));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    fn create_directory_link(
        target: &std::path::Path,
        link: &std::path::Path,
    ) -> std::io::Result<()> {
        std::os::unix::fs::symlink(target, link)
    }

    #[cfg(windows)]
    fn create_directory_link(
        target: &std::path::Path,
        link: &std::path::Path,
    ) -> std::io::Result<()> {
        let output = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ))
        }
    }

    fn remove_directory_link(link: &std::path::Path) {
        let _ = std::fs::remove_dir(link);
    }
}
