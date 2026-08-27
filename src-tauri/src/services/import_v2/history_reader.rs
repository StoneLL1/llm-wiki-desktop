use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use crate::errors::BackendError;
use crate::models::import_v2::{ImportBatchResult, ImportSession};
use crate::models::import_v2_migration::LegacyHistoryWarning;
use crate::models::paths::ProjectContext;
use crate::services::FileStore;

use super::ImportV2Service;

pub(crate) struct V2HistoryRecord {
    pub batch: ImportBatchResult,
    pub session: Option<ImportSession>,
    pub modified_millis: u64,
    pub updated_at: Option<String>,
}

pub(crate) struct V2HistoryScan {
    pub records: Vec<V2HistoryRecord>,
    pub claimed_paths: HashSet<String>,
    pub warnings: Vec<LegacyHistoryWarning>,
}

pub(crate) fn read_v2_history_records(
    context: &ProjectContext,
    files: &FileStore,
    imports: &ImportV2Service,
) -> Result<V2HistoryScan, BackendError> {
    let history_dir = context.resolve_project_path(".app/import-history")?;
    let metadata = match fs::symlink_metadata(&history_dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(V2HistoryScan {
                records: Vec::new(),
                claimed_paths: HashSet::new(),
                warnings: Vec::new(),
            });
        }
        Err(error) => {
            return Err(history_error(format!(
                "Import history could not be read: {error}"
            )))
        }
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
        return Err(history_error("Import history directory is invalid."));
    }

    let mut paths = fs::read_dir(&history_dir)
        .map_err(|_| history_error("Import history could not be read."))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().and_then(|value| value.to_str()) == Some("json")
                && fs::symlink_metadata(path)
                    .is_ok_and(|value| value.is_file() && !value.file_type().is_symlink())
        })
        .collect::<Vec<PathBuf>>();
    paths.sort();

    let mut records = Vec::new();
    let mut claimed_paths = HashSet::new();
    let mut warnings = Vec::new();
    for path in paths {
        let relative_path = path
            .strip_prefix(&context.root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        #[cfg(feature = "performance-observers")]
        let read_result = files.read_project_bytes_absolute(context, &path);
        #[cfg(not(feature = "performance-observers"))]
        let read_result = fs::read(&path).map_err(|error| {
            BackendError::new(
                "IMPORT_V2_HISTORY_READ_FAILED",
                error.to_string(),
                true,
                true,
            )
        });
        let bytes = match read_result {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let value = match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if value.get("sessionId").is_none() || value.get("items").is_none() {
            continue;
        }
        claimed_paths.insert(relative_path.clone());
        let batch: ImportBatchResult = match serde_json::from_value(value) {
            Ok(batch) => batch,
            Err(_) => {
                warnings.push(LegacyHistoryWarning {
                    code: "IMPORT_V2_HISTORY_CORRUPT".into(),
                    message: "A V2 import history record could not be read.".into(),
                    evidence_path: relative_path,
                });
                continue;
            }
        };
        let session = batch
            .history_snapshot
            .clone()
            .or_else(|| imports.load_session(context, files, &batch.session_id).ok());
        let modified_millis = parse_timestamp_millis(&batch.created_at)
            .unwrap_or_else(|| file_modified_millis(&path));
        let updated_at = fs::metadata(&path)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .map(|time| chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339());
        records.push(V2HistoryRecord {
            batch,
            session,
            modified_millis,
            updated_at,
        });
    }
    records.sort_by(|left, right| {
        right
            .modified_millis
            .cmp(&left.modified_millis)
            .then_with(|| right.batch.batch_id.cmp(&left.batch.batch_id))
    });
    Ok(V2HistoryScan {
        records,
        claimed_paths,
        warnings,
    })
}

fn history_error(message: impl Into<String>) -> BackendError {
    BackendError::new("IMPORT_V2_HISTORY_READ_FAILED", message, true, true)
}

fn file_modified_millis(path: &std::path::Path) -> u64 {
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or_default()
}

fn parse_timestamp_millis(value: &str) -> Option<u64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .and_then(|time| time.timestamp_millis().try_into().ok())
}

#[cfg(windows)]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_: &std::fs::Metadata) -> bool {
    false
}

#[cfg(all(test, feature = "performance-observers"))]
mod tests {
    use super::*;
    use crate::models::import_v2::{ImportResourceMode, ImportSession};

    #[test]
    fn observer_measures_the_production_history_scan_at_scale() {
        for history_count in [100usize, 1_000, 10_000] {
            let root = tempfile::tempdir().unwrap();
            let context = ProjectContext::new("perf-history", root.path().to_path_buf());
            let files = FileStore::default();
            let imports = ImportV2Service::default();
            let history_root = root.path().join(".app/import-history");
            std::fs::create_dir_all(&history_root).unwrap();
            for index in 0..history_count {
                let batch = ImportBatchResult {
                    batch_id: format!("batch-{index:05}"),
                    session_id: format!("session-{index:05}"),
                    created_at: "2026-08-27T00:00:00Z".into(),
                    batch_task_id: None,
                    committed_count: 0,
                    failed_count: 0,
                    items: Vec::new(),
                    history_snapshot: Some(ImportSession::new(
                        &format!("session-{index:05}"),
                        "perf-history",
                        ImportResourceMode::Balanced,
                    )),
                    completion: None,
                };
                std::fs::write(
                    history_root.join(format!("batch-{index:05}.json")),
                    serde_json::to_vec(&batch).unwrap(),
                )
                .unwrap();
            }

            let observation = files.observe_project(&context);
            let scan = read_v2_history_records(&context, &files, &imports).unwrap();
            let snapshot = observation.snapshot();
            assert_eq!(scan.records.len(), history_count);
            assert_eq!(scan.claimed_paths.len(), history_count);
            assert!(scan.warnings.is_empty());
            assert_eq!(snapshot.read_ops, history_count as u64);
            assert_eq!(snapshot.write_ops, 0);
            assert!(snapshot.bytes_read > 0);
            println!(
                "BATCH0_HISTORY_PRODUCTION H={history_count} {}",
                serde_json::to_string(&snapshot).unwrap()
            );
        }
    }
}
