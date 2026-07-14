use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::errors::BackendError;
use crate::models::import_v2_migration::{
    LegacyHistoryEntry, LegacyHistoryView, LegacyHistoryWarning,
};
use crate::services::import_v2::transaction::{
    is_project_reparse_point, read_project_file_nofollow,
};

#[derive(Debug, Clone, Copy)]
pub struct LegacyHistoryLimits {
    pub max_files: usize,
    pub max_bytes: u64,
}

impl Default for LegacyHistoryLimits {
    fn default() -> Self {
        Self {
            max_files: 10_000,
            max_bytes: 32 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LegacyHistoryAdapter {
    limits: LegacyHistoryLimits,
}

impl Default for LegacyHistoryAdapter {
    fn default() -> Self {
        Self {
            limits: LegacyHistoryLimits::default(),
        }
    }
}

impl LegacyHistoryAdapter {
    pub fn new(limits: LegacyHistoryLimits) -> Self {
        Self { limits }
    }

    pub fn list(&self, project_root: &Path) -> Result<LegacyHistoryView, BackendError> {
        let metadata = fs::symlink_metadata(project_root).map_err(|error| {
            BackendError::new("PROJECT_NOT_FOUND", error.to_string(), true, true)
        })?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || is_project_reparse_point(&metadata)
        {
            return Err(BackendError::new(
                "PROJECT_PATH_INVALID",
                "Legacy history requires a regular project directory.",
                false,
                true,
            ));
        }
        let mut paths = Vec::new();
        for directory in [".app/tasks", ".app/import-history"] {
            collect_json_paths(project_root, directory, &mut paths);
        }
        paths.sort();

        let mut view = LegacyHistoryView::default();
        let mut bytes_read = 0_u64;
        for (index, path) in paths.iter().enumerate() {
            if index >= self.limits.max_files {
                view.warnings.push(limit_warning("file count"));
                break;
            }
            let relative = relative_path(project_root, path);
            let metadata = match fs::symlink_metadata(path) {
                Ok(metadata) if metadata.is_file() => metadata,
                _ => continue,
            };
            if bytes_read.saturating_add(metadata.len()) > self.limits.max_bytes {
                view.warnings.push(LegacyHistoryWarning {
                    code: "LEGACY_HISTORY_LIMIT".into(),
                    message: "Legacy history byte limit reached; remaining entries were not read.".into(),
                    evidence_path: relative,
                });
                break;
            }
            bytes_read = bytes_read.saturating_add(metadata.len());
            let bytes = match read_project_file_nofollow(project_root, path) {
                Ok(bytes) => bytes,
                Err(_) => {
                    view.warnings.push(corrupt_warning(&relative, "read failed"));
                    continue;
                }
            };
            let value: Value = match serde_json::from_slice(&bytes) {
                Ok(value) => value,
                Err(_) => {
                    view.warnings.push(corrupt_warning(&relative, "malformed JSON"));
                    continue;
                }
            };
            let Some(object) = value.as_object() else {
                view.warnings.push(corrupt_warning(&relative, "unsupported shape"));
                continue;
            };
            let id = string_field(object, &["id", "taskId", "batchId"])
                .map(str::to_string)
                .or_else(|| path.file_stem().and_then(|value| value.to_str()).map(str::to_string))
                .unwrap_or_else(|| "legacy-entry".into());
            let title = string_field(object, &["title", "name"])
                .unwrap_or("Legacy import history")
                .chars()
                .take(200)
                .collect();
            view.entries.push(LegacyHistoryEntry {
                id,
                title,
                status: string_field(object, &["status"]).unwrap_or("unknown").into(),
                started_at: string_field(object, &["startedAt", "createdAt"]).map(str::to_string),
                updated_at: string_field(object, &["updatedAt"]).map(str::to_string),
                completed_at: string_field(object, &["completedAt"]).map(str::to_string),
                evidence_path: relative,
                legacy_read_only: true,
                available_actions: Vec::new(),
                can_retry: false,
                can_delete: false,
                can_replace_source: false,
            });
        }
        view.entries.sort_by(|left, right| left.id.cmp(&right.id));
        view.warnings.sort_by(|left, right| left.evidence_path.cmp(&right.evidence_path));
        Ok(view)
    }
}

fn collect_json_paths(root: &Path, relative: &str, output: &mut Vec<PathBuf>) {
    let path = root.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR));
    let Ok(metadata) = fs::symlink_metadata(&path) else { return };
    if metadata.file_type().is_symlink() || is_project_reparse_point(&metadata) {
        return;
    }
    if metadata.is_file() {
        if path.extension().and_then(|value| value.to_str()) == Some("json") {
            output.push(path);
        }
        return;
    }
    if !metadata.is_dir() { return; }
    let Ok(entries) = fs::read_dir(path) else { return };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    entries.sort();
    for entry in entries {
        if let Ok(child) = entry.strip_prefix(root) {
            collect_json_paths(root, &child.to_string_lossy().replace('\\', "/"), output);
        }
    }
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn string_field<'a>(object: &'a serde_json::Map<String, Value>, names: &[&str]) -> Option<&'a str> {
    names.iter().find_map(|name| object.get(*name).and_then(Value::as_str))
}

fn corrupt_warning(path: &str, reason: &str) -> LegacyHistoryWarning {
    LegacyHistoryWarning {
        code: "LEGACY_HISTORY_CORRUPT".into(),
        message: format!("Legacy history entry was skipped ({reason}); sensitive fields were omitted."),
        evidence_path: path.into(),
    }
}

fn limit_warning(reason: &str) -> LegacyHistoryWarning {
    LegacyHistoryWarning {
        code: "LEGACY_HISTORY_LIMIT".into(),
        message: format!("Legacy history {reason} limit reached; remaining entries were not read."),
        evidence_path: ".app/".into(),
    }
}
