use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::errors::BackendError;
use crate::models::paths::ProjectContext;
use crate::utils::path_utils::normalize_project_path;

#[derive(Default)]
pub struct FileStore;

static GUARDED_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

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
        let path = context.resolve_project_path(relative_path)?;
        fs::create_dir_all(&path).map_err(|err| io_error("FILE_DIR_CREATE_FAILED", err, &path))
    }

    pub fn ensure_absolute_dir(&self, path: &Path) -> Result<(), BackendError> {
        fs::create_dir_all(path).map_err(|err| io_error("FILE_DIR_CREATE_FAILED", err, path))
    }

    pub fn read_markdown(
        &self,
        context: &ProjectContext,
        relative_path: &str,
    ) -> Result<String, BackendError> {
        let path = context.resolve_project_path(relative_path)?;
        fs::read_to_string(&path).map_err(|err| io_error("FILE_READ_FAILED", err, &path))
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
        let path = context.resolve_project_path(relative_path)?;
        write_text(&path, contents)
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
        let path = context.resolve_project_path(relative_path)?;
        self.verify_write_mode(&path, relative_path, mode.clone())?;
        write_text(&path, contents)?;
        // The hash check and atomic rename cannot form an OS-level compare-and
        // swap for edits made by an external editor. Verify the postcondition
        // immediately so a racing replacement is reported and never enters
        // the Lint verification/rollback path as if our content were present.
        if let WriteMode::OverwriteIfHashMatches(expected_hash) = mode {
            let expected_new_hash = hash_bytes(contents.as_bytes());
            let actual_hash = hash_file(&path)?;
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
        let path = context.resolve_project_path(relative_path)?;
        self.verify_write_mode(&path, relative_path, WriteMode::CreateNew)?;
        write_atomic_create_new(&path, relative_path, contents.as_bytes())
    }

    pub fn write_text_absolute(&self, path: &Path, contents: &str) -> Result<(), BackendError> {
        write_text(path, contents)
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
        let path = context.resolve_project_path(relative_path)?;
        let serialized = serde_json::to_string_pretty(value).map_err(|err| {
            BackendError::new("JSON_SERIALIZE_FAILED", err.to_string(), true, false)
        })?;
        write_atomic(&path, serialized.as_bytes())
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
        let path = context.resolve_project_path(relative_path)?;
        self.verify_write_mode(&path, relative_path, mode)?;
        let serialized = serde_json::to_string_pretty(value).map_err(|err| {
            BackendError::new("JSON_SERIALIZE_FAILED", err.to_string(), true, false)
        })?;
        write_atomic(&path, serialized.as_bytes())
    }

    pub fn write_json_atomic_absolute<T: serde::Serialize>(
        &self,
        path: &Path,
        value: &T,
    ) -> Result<(), BackendError> {
        let serialized = serde_json::to_string_pretty(value).map_err(|err| {
            BackendError::new("JSON_SERIALIZE_FAILED", err.to_string(), true, false)
        })?;
        write_atomic(path, serialized.as_bytes())
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
        hash_file(&path)
    }

    pub fn file_hash_if_exists(
        &self,
        context: &ProjectContext,
        relative_path: &str,
    ) -> Result<Option<String>, BackendError> {
        let path = context.resolve_project_path(relative_path)?;
        if !path.exists() {
            return Ok(None);
        }
        hash_file(&path).map(Some)
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
        path: &Path,
        relative_path: &str,
        mode: WriteMode,
    ) -> Result<(), BackendError> {
        match mode {
            WriteMode::CreateNew if path.exists() => Err(BackendError::new(
                "FILE_ALREADY_EXISTS",
                "File already exists and cannot be overwritten without an explicit hash match.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "path": relative_path }))),
            WriteMode::CreateNew => Ok(()),
            WriteMode::OverwriteIfHashMatches(expected_hash) => {
                if !path.exists() {
                    return Err(BackendError::new(
                        "FILE_NOT_FOUND",
                        "Cannot overwrite a missing file.",
                        true,
                        true,
                    )
                    .with_details(serde_json::json!({ "path": relative_path })));
                }
                let current_hash = hash_file(path)?;
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
                    let baseline_content = fs::read(path)
                        .ok()
                        .and_then(|bytes| String::from_utf8(bytes).ok());
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
                Ok(())
            }
        }
    }
}

fn hash_file(path: &Path) -> Result<String, BackendError> {
    let bytes = fs::read(path).map_err(|err| io_error("FILE_READ_FAILED", err, path))?;
    Ok(hash_bytes(&bytes))
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn write_text(path: &Path, contents: &str) -> Result<(), BackendError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| io_error("FILE_DIR_CREATE_FAILED", err, parent))?;
    }
    write_atomic(path, contents.as_bytes())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), BackendError> {
    let parent = path.parent().ok_or_else(|| {
        BackendError::new(
            "PATH_INVALID",
            "Cannot determine parent directory.",
            false,
            true,
        )
    })?;
    fs::create_dir_all(parent).map_err(|err| io_error("FILE_DIR_CREATE_FAILED", err, parent))?;

    let file_name = path.file_name().ok_or_else(|| {
        BackendError::new("PATH_INVALID", "Cannot determine file name.", false, true)
    })?;
    let (tmp_path, mut file) = (0..16)
        .find_map(|_| {
            let candidate = parent.join(format!(
                ".{}.{}.tmp",
                file_name.to_string_lossy(),
                uuid::Uuid::new_v4()
            ));
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(file) => Some(Ok((candidate, file))),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                Err(error) => Some(Err(io_error("FILE_WRITE_FAILED", error, &candidate))),
            }
        })
        .unwrap_or_else(|| {
            Err(BackendError::new(
                "FILE_WRITE_FAILED",
                "Could not reserve a unique atomic-write temporary file.",
                true,
                false,
            ))
        })?;

    // `create_new` atomically rejects every pre-existing filesystem object at
    // the candidate path, including links/reparse points, before any bytes are
    // written. The random same-directory name also prevents prediction-based
    // replacement between project-path validation and this write.
    let write_result = file
        .write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|err| io_error("FILE_WRITE_FAILED", err, &tmp_path));
    drop(file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&tmp_path);
        return Err(error);
    }

    fs::rename(&tmp_path, path).map_err(|err| {
        let _ = fs::remove_file(&tmp_path);
        io_error("FILE_WRITE_FAILED", err, path)
    })
}

fn write_atomic_create_new(
    path: &Path,
    relative_path: &str,
    bytes: &[u8],
) -> Result<(), BackendError> {
    let parent = path.parent().ok_or_else(|| {
        BackendError::new(
            "PATH_INVALID",
            "Cannot determine parent directory.",
            false,
            true,
        )
    })?;
    fs::create_dir_all(parent).map_err(|err| io_error("FILE_DIR_CREATE_FAILED", err, parent))?;
    let file_name = path.file_name().ok_or_else(|| {
        BackendError::new("PATH_INVALID", "Cannot determine file name.", false, true)
    })?;
    let (tmp_path, mut file) = (0..16)
        .find_map(|_| {
            let candidate = parent.join(format!(
                ".{}.{}.tmp",
                file_name.to_string_lossy(),
                uuid::Uuid::new_v4()
            ));
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(file) => Some(Ok((candidate, file))),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                Err(error) => Some(Err(io_error("FILE_WRITE_FAILED", error, &candidate))),
            }
        })
        .unwrap_or_else(|| {
            Err(BackendError::new(
                "FILE_WRITE_FAILED",
                "Could not reserve a unique atomic-create temporary file.",
                true,
                false,
            ))
        })?;
    let write_result = file
        .write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| io_error("FILE_WRITE_FAILED", error, &tmp_path));
    drop(file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&tmp_path);
        return Err(error);
    }

    // Linking a complete same-directory temporary file is an OS-level
    // no-replace publish: every pre-existing target (including a link created
    // after the initial check) makes hard_link fail instead of being replaced.
    let publish_result = fs::hard_link(&tmp_path, path).map_err(|error| {
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
    });
    let _ = fs::remove_file(&tmp_path);
    publish_result
}

fn walk_markdown(current: &Path, results: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            let name = entry.file_name();
            // Skip Obsidian and hidden app state when enumerating wiki content.
            if name == ".obsidian" || name == ".git" || name == ".app" {
                continue;
            }
            walk_markdown(&path, results)?;
        } else if file_type.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("md")
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
}
