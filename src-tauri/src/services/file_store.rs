use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::errors::BackendError;
use crate::models::paths::ProjectContext;

#[derive(Default)]
pub struct FileStore;

impl FileStore {
    pub fn exists(&self, context: &ProjectContext, relative_path: &str) -> bool {
        context.resolve_project_path(relative_path)
            .map(|path| path.exists())
            .unwrap_or(false)
    }

    pub fn ensure_dir(&self, context: &ProjectContext, relative_path: &str) -> Result<(), BackendError> {
        let path = context.resolve_project_path(relative_path)?;
        fs::create_dir_all(&path).map_err(|err| io_error("FILE_DIR_CREATE_FAILED", err, &path))
    }

    pub fn ensure_absolute_dir(&self, path: &Path) -> Result<(), BackendError> {
        fs::create_dir_all(path).map_err(|err| io_error("FILE_DIR_CREATE_FAILED", err, path))
    }

    pub fn read_markdown(&self, context: &ProjectContext, relative_path: &str) -> Result<String, BackendError> {
        let path = context.resolve_project_path(relative_path)?;
        fs::read_to_string(&path).map_err(|err| io_error("FILE_READ_FAILED", err, &path))
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

    pub fn write_text_absolute(&self, path: &Path, contents: &str) -> Result<(), BackendError> {
        write_text(path, contents)
    }

    pub fn read_json<T: serde::de::DeserializeOwned>(
        &self,
        context: &ProjectContext,
        relative_path: &str,
    ) -> Result<T, BackendError> {
        let raw = self.read_markdown(context, relative_path)?;
        serde_json::from_str(&raw)
            .map_err(|err| BackendError::new("JSON_PARSE_FAILED", err.to_string(), true, false)
                .with_details(serde_json::json!({ "path": relative_path })))
    }

    pub fn read_json_file<T: serde::de::DeserializeOwned>(&self, path: &Path) -> Result<T, BackendError> {
        let raw = fs::read_to_string(path).map_err(|err| io_error("FILE_READ_FAILED", err, path))?;
        serde_json::from_str(&raw)
            .map_err(|err| BackendError::new("JSON_PARSE_FAILED", err.to_string(), true, false)
                .with_details(serde_json::json!({ "path": path.to_string_lossy() })))
    }

    pub fn write_json_atomic<T: serde::Serialize>(
        &self,
        context: &ProjectContext,
        relative_path: &str,
        value: &T,
    ) -> Result<(), BackendError> {
        let path = context.resolve_project_path(relative_path)?;
        let serialized = serde_json::to_string_pretty(value)
            .map_err(|err| BackendError::new("JSON_SERIALIZE_FAILED", err.to_string(), true, false))?;
        write_atomic(&path, serialized.as_bytes())
    }

    pub fn write_json_atomic_absolute<T: serde::Serialize>(
        &self,
        path: &Path,
        value: &T,
    ) -> Result<(), BackendError> {
        let serialized = serde_json::to_string_pretty(value)
            .map_err(|err| BackendError::new("JSON_SERIALIZE_FAILED", err.to_string(), true, false))?;
        write_atomic(path, serialized.as_bytes())
    }

    pub fn list_markdown_files(&self, root: &Path) -> Result<Vec<PathBuf>, BackendError> {
        let mut results = Vec::new();
        if !root.exists() {
            return Ok(results);
        }
        walk_markdown(root, root, &mut results)
            .map_err(|err| io_error("FILE_ENUMERATE_FAILED", err, root))?;
        results.sort();
        Ok(results)
    }
}

fn write_text(path: &Path, contents: &str) -> Result<(), BackendError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| io_error("FILE_DIR_CREATE_FAILED", err, parent))?;
    }
    write_atomic(path, contents.as_bytes())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), BackendError> {
    let parent = path.parent().ok_or_else(|| {
        BackendError::new("PATH_INVALID", "Cannot determine parent directory.", false, true)
    })?;
    fs::create_dir_all(parent).map_err(|err| io_error("FILE_DIR_CREATE_FAILED", err, parent))?;

    let tmp_name = match path.file_name() {
        Some(name) => format!(".{}.tmp", name.to_string_lossy()),
        None => ".tmp".to_string(),
    };
    let tmp_path = parent.join(tmp_name);

    {
        let mut file = fs::File::create(&tmp_path)
            .map_err(|err| io_error("FILE_WRITE_FAILED", err, &tmp_path))?;
        file.write_all(bytes)
            .map_err(|err| io_error("FILE_WRITE_FAILED", err, &tmp_path))?;
        file.sync_all()
            .map_err(|err| io_error("FILE_WRITE_FAILED", err, &tmp_path))?;
    }

    fs::rename(&tmp_path, path).map_err(|err| {
        let _ = fs::remove_file(&tmp_path);
        io_error("FILE_WRITE_FAILED", err, path)
    })
}

fn walk_markdown(base: &Path, current: &Path, results: &mut Vec<PathBuf>) -> std::io::Result<()> {
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
            walk_markdown(base, &path, results)?;
        } else if file_type.is_file() {
            if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
                results.push(path);
            }
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
    use super::FileStore;
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
            .write_json_atomic(&context, ".app/settings.json", &Sample { name: "zh".into(), count: 7 })
            .unwrap();

        let back: Sample = store.read_json(&context, ".app/settings.json").unwrap();
        assert_eq!(back, Sample { name: "zh".into(), count: 7 });
        // No leftover temp files.
        let entries = std::fs::read_dir(context.app_dir.join("settings.json").parent().unwrap()).unwrap();
        for entry in entries {
            let name = entry.unwrap().file_name();
            assert!(!name.to_string_lossy().contains(".tmp"), "temp file leaked: {name:?}");
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
    fn enumerates_markdown_skipping_obsidian_and_app_state() {
        let (context, root) = tmp_context("enum");
        let store = FileStore;

        store.write_markdown(&context, "wiki/concepts/agent.md", "a").unwrap();
        store.write_markdown(&context, "wiki/index.md", "i").unwrap();
        store.write_markdown(&context, "wiki/.obsidian/app.md", "obsidian").unwrap();
        std::fs::create_dir_all(root.join("wiki/.obsidian")).ok();
        std::fs::write(root.join("wiki/.obsidian/app.md"), "obsidian").unwrap();
        std::fs::create_dir_all(context.app_dir.join("tasks")).unwrap();
        std::fs::write(context.app_dir.join("tasks").join("task-1.md"), "log").unwrap();

        let files = store.list_markdown_files(&context.wiki_dir).unwrap();
        let relative: Vec<String> = files
            .iter()
            .map(|p| p.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/"))
            .collect();

        assert!(relative.contains(&"wiki/concepts/agent.md".to_string()));
        assert!(relative.contains(&"wiki/index.md".to_string()));
        assert!(!relative.iter().any(|p| p.contains(".obsidian")));
        assert!(!relative.iter().any(|p| p.contains(".app")));

        std::fs::remove_dir_all(root).unwrap();
    }
}
