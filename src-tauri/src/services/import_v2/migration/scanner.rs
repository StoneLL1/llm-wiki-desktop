use std::collections::BTreeSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::errors::BackendError;
use crate::models::import_v2_migration::{
    validate_project_relative, LegacyFileEvidence, LegacyInventory, LegacyRecord, MigrationWarning,
    IMPORT_V2_MIGRATION_SCHEMA_VERSION,
};

use super::super::transaction::{is_project_reparse_point, read_project_file_nofollow};

const LEGACY_INDEX: &str = ".app/source-index.json";
const IMPORT_HISTORY_DIR: &str = ".app/import-history";
const TASKS_DIR: &str = ".app/tasks";
const RAW_DIR: &str = "raw";
const WIKI_DIR: &str = "wiki";

pub trait LegacyScanner: Send + Sync {
    fn scan(&self, project_root: &Path) -> Result<LegacyInventory, BackendError>;
}

#[derive(Debug, Clone, Copy)]
pub struct ScannerLimits {
    pub max_files: usize,
    pub max_metadata_bytes: u64,
}

impl Default for ScannerLimits {
    fn default() -> Self {
        Self {
            max_files: 50_000,
            max_metadata_bytes: 16 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DefaultLegacyScanner {
    limits: ScannerLimits,
}

impl DefaultLegacyScanner {
    pub fn new(limits: ScannerLimits) -> Self {
        Self { limits }
    }
}

impl LegacyScanner for DefaultLegacyScanner {
    fn scan(&self, project_root: &Path) -> Result<LegacyInventory, BackendError> {
        let root_metadata = fs::symlink_metadata(project_root).map_err(|error| {
            BackendError::new(
                "PROJECT_NOT_FOUND",
                format!("Cannot inspect the migration project: {error}"),
                true,
                true,
            )
        })?;
        if !root_metadata.is_dir()
            || root_metadata.file_type().is_symlink()
            || is_project_reparse_point(&root_metadata)
        {
            return Err(BackendError::new(
                "PROJECT_PATH_INVALID",
                "The migration project must be a regular directory.",
                false,
                true,
            ));
        }

        let mut state = ScanState {
            root: project_root,
            limits: self.limits,
            file_count: 0,
            metadata_bytes: 0,
            records: Vec::new(),
            warnings: Vec::new(),
            files: Vec::new(),
            referenced_paths: BTreeSet::new(),
        };

        if let Some(bytes) = state.read_metadata_file(LEGACY_INDEX) {
            parse_index(&mut state, &bytes);
        }
        state.walk_tree(IMPORT_HISTORY_DIR, true);
        state.walk_tree(TASKS_DIR, false);
        state.walk_tree(RAW_DIR, false);
        state.walk_tree(WIKI_DIR, false);
        for path in state.referenced_paths.clone() {
            if !state.files.iter().any(|file| file.relative_path == path) {
                state.read_content_file(&path);
            }
        }

        state
            .records
            .sort_by(|left, right| left.record_id.cmp(&right.record_id));
        state
            .files
            .sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        state.warnings.sort_by(|left, right| {
            (
                left.code.as_str(),
                left.relative_path.as_deref().unwrap_or_default(),
            )
                .cmp(&(
                    right.code.as_str(),
                    right.relative_path.as_deref().unwrap_or_default(),
                ))
        });
        let project_identity = project_identity(project_root, &root_metadata);
        let fingerprint = fingerprint(
            &project_identity,
            &state.records,
            &state.files,
            &state.warnings,
        );
        Ok(LegacyInventory {
            schema_version: IMPORT_V2_MIGRATION_SCHEMA_VERSION,
            project_identity,
            fingerprint,
            records: state.records,
            warnings: state.warnings,
            scanned_files: state.files,
        })
    }
}

struct ScanState<'a> {
    root: &'a Path,
    limits: ScannerLimits,
    file_count: usize,
    metadata_bytes: u64,
    records: Vec<LegacyRecord>,
    warnings: Vec<MigrationWarning>,
    files: Vec<LegacyFileEvidence>,
    referenced_paths: BTreeSet<String>,
}

impl<'a> ScanState<'a> {
    fn read_metadata_file(&mut self, relative: &str) -> Option<Vec<u8>> {
        if !self.take_file_slot(relative) {
            return None;
        }
        let path = self
            .root
            .join(relative.replace('/', std::path::MAIN_SEPARATOR_STR));
        let metadata = match safe_metadata(&path) {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => {
                self.warn("MIGRATION_NON_REGULAR_FILE", relative, false);
                return None;
            }
            Err(error) if error.kind() == ErrorKind::NotFound => return None,
            Err(_) => {
                self.warn("MIGRATION_METADATA_UNREADABLE", relative, false);
                return None;
            }
        };
        if self.metadata_bytes.saturating_add(metadata.len()) > self.limits.max_metadata_bytes {
            self.warn("MIGRATION_SCAN_LIMIT", relative, false);
            return None;
        }
        self.metadata_bytes = self.metadata_bytes.saturating_add(metadata.len());
        let bytes = match read_project_file_nofollow(self.root, &path) {
            Ok(bytes) => bytes,
            Err(_) => {
                self.warn("MIGRATION_METADATA_UNREADABLE", relative, false);
                return None;
            }
        };
        self.record_file(relative, &metadata, &bytes);
        Some(bytes)
    }

    fn read_content_file(&mut self, relative: &str) {
        if !self.take_file_slot(relative) {
            return;
        }
        let path = self
            .root
            .join(relative.replace('/', std::path::MAIN_SEPARATOR_STR));
        let metadata = match safe_metadata(&path) {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => {
                self.warn("MIGRATION_NON_REGULAR_FILE", relative, false);
                return;
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                self.warn("MIGRATION_REFERENCED_FILE_MISSING", relative, false);
                return;
            }
            Err(_) => {
                self.warn("MIGRATION_REFERENCED_FILE_UNREADABLE", relative, false);
                return;
            }
        };
        let bytes = match read_project_file_nofollow(self.root, &path) {
            Ok(bytes) => bytes,
            Err(_) => {
                self.warn("MIGRATION_REFERENCED_FILE_UNREADABLE", relative, false);
                return;
            }
        };
        self.record_file(relative, &metadata, &bytes);
    }

    fn walk_tree(&mut self, relative: &str, parse_json: bool) {
        let path = self
            .root
            .join(relative.replace('/', std::path::MAIN_SEPARATOR_STR));
        // Inspect the entry itself so links can be classified distinctly.
        // `safe_metadata` intentionally converts links into PermissionDenied,
        // which would collapse this security signal into a generic unreadable
        // directory warning before the no-follow branch below can run.
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => return,
            Err(_) => {
                self.warn("MIGRATION_DIRECTORY_UNREADABLE", relative, false);
                return;
            }
        };
        if metadata.file_type().is_symlink() || is_project_reparse_point(&metadata) {
            self.warn("MIGRATION_SYMLINK_SKIPPED", relative, false);
            return;
        }
        if metadata.is_file() {
            let bytes = if parse_json {
                self.read_metadata_file(relative)
            } else {
                self.read_content_bytes(relative)
            };
            if parse_json {
                if let Some(bytes) = bytes {
                    self.parse_history_record(relative, &bytes);
                }
            }
            return;
        }
        if !metadata.is_dir() {
            self.warn("MIGRATION_NON_REGULAR_FILE", relative, false);
            return;
        }
        let mut entries: Vec<PathBuf> = match fs::read_dir(&path) {
            Ok(entries) => entries.flatten().map(|entry| entry.path()).collect(),
            Err(_) => {
                self.warn("MIGRATION_DIRECTORY_UNREADABLE", relative, false);
                return;
            }
        };
        entries.sort();
        for entry in entries {
            let Ok(child) = entry.strip_prefix(self.root) else {
                continue;
            };
            let child = child.to_string_lossy().replace('\\', "/");
            self.walk_tree(&child, parse_json);
            if self.file_count >= self.limits.max_files {
                break;
            }
        }
    }

    fn read_content_bytes(&mut self, relative: &str) -> Option<Vec<u8>> {
        if !self.take_file_slot(relative) {
            return None;
        }
        let path = self
            .root
            .join(relative.replace('/', std::path::MAIN_SEPARATOR_STR));
        let metadata = match safe_metadata(&path) {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => {
                self.warn("MIGRATION_NON_REGULAR_FILE", relative, false);
                return None;
            }
            Err(_) => {
                self.warn("MIGRATION_REFERENCED_FILE_UNREADABLE", relative, false);
                return None;
            }
        };
        let bytes = match read_project_file_nofollow(self.root, &path) {
            Ok(bytes) => bytes,
            Err(_) => {
                self.warn("MIGRATION_REFERENCED_FILE_UNREADABLE", relative, false);
                return None;
            }
        };
        self.record_file(relative, &metadata, &bytes);
        Some(bytes)
    }

    fn parse_history_record(&mut self, relative: &str, bytes: &[u8]) {
        let value: Value = match serde_json::from_slice(bytes) {
            Ok(value) => value,
            Err(_) => {
                self.warn("MIGRATION_LEGACY_METADATA_CORRUPT", relative, true);
                return;
            }
        };
        if let Some(record) = record_from_value(&value, relative, &mut self.warnings) {
            self.retain_record_paths(&record);
            self.records.push(record);
        } else {
            self.warn("MIGRATION_LEGACY_SHAPE_UNKNOWN", relative, true);
        }
    }

    fn retain_record_paths(&mut self, record: &LegacyRecord) {
        for path in [
            record.original_path.as_deref(),
            record.destination_path.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            self.referenced_paths.insert(path.to_string());
        }
    }

    fn record_file(&mut self, relative: &str, metadata: &fs::Metadata, bytes: &[u8]) {
        let modified_nanos = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
            .map(|value| value.as_nanos());
        self.files.push(LegacyFileEvidence {
            relative_path: relative.to_string(),
            sha256: sha256(bytes),
            size_bytes: metadata.len(),
            modified_nanos,
        });
    }

    fn take_file_slot(&mut self, relative: &str) -> bool {
        if self.file_count >= self.limits.max_files {
            self.warn("MIGRATION_SCAN_LIMIT", relative, false);
            return false;
        }
        self.file_count += 1;
        true
    }

    fn warn(&mut self, code: &str, relative: &str, redacted: bool) {
        self.warnings.push(MigrationWarning {
            code: code.into(),
            message: if redacted {
                "Legacy metadata was malformed or contained an unsupported shape; sensitive fields were omitted.".into()
            } else {
                "Legacy evidence could not be used automatically.".into()
            },
            relative_path: Some(relative.replace('\\', "/")),
            redacted,
        });
    }
}

fn parse_index(state: &mut ScanState<'_>, bytes: &[u8]) {
    let value: Value = match serde_json::from_slice(bytes) {
        Ok(value) => value,
        Err(_) => {
            state.warn("MIGRATION_LEGACY_METADATA_CORRUPT", LEGACY_INDEX, true);
            return;
        }
    };
    if let Some(records) = value.get("records").and_then(Value::as_array) {
        for record_value in records {
            if let Some(record) = record_from_value(record_value, LEGACY_INDEX, &mut state.warnings)
            {
                state.retain_record_paths(&record);
                state.records.push(record);
            } else {
                state.warn("MIGRATION_LEGACY_SHAPE_UNKNOWN", LEGACY_INDEX, true);
            }
        }
        return;
    }
    if let Some(sources) = value.get("sources").and_then(Value::as_object) {
        for (source_path, artifacts) in sources {
            let destination_path = artifacts
                .as_array()
                .and_then(|values| values.iter().find_map(Value::as_str))
                .map(str::to_string);
            let record = LegacyRecord {
                record_id: stable_record_id(LEGACY_INDEX, source_path),
                stable_source_id: None,
                original_path: sanitize_path(source_path, LEGACY_INDEX, &mut state.warnings),
                destination_path: destination_path
                    .and_then(|path| sanitize_path(&path, LEGACY_INDEX, &mut state.warnings)),
                original_sha256: None,
                normalized_url: None,
                recorded_content_sha256: None,
                metadata_path: LEGACY_INDEX.into(),
            };
            state.retain_record_paths(&record);
            state.records.push(record);
        }
        return;
    }
    state.warn("MIGRATION_LEGACY_SHAPE_UNKNOWN", LEGACY_INDEX, true);
}

fn record_from_value(
    value: &Value,
    metadata_path: &str,
    warnings: &mut Vec<MigrationWarning>,
) -> Option<LegacyRecord> {
    let object = value.as_object()?;
    let record_id = string_field(object, &["recordId", "id", "batchId"])
        .map(str::to_string)
        .unwrap_or_else(|| stable_record_id(metadata_path, "record"));
    let original_path = path_field(
        object,
        &["originalPath", "rawPath", "sourcePath"],
        metadata_path,
        warnings,
    );
    let destination_path = path_field(
        object,
        &["destinationPath", "wikiPath", "outputPath"],
        metadata_path,
        warnings,
    );
    Some(LegacyRecord {
        record_id,
        stable_source_id: string_field(object, &["sourceId", "stableSourceId"]).map(str::to_string),
        original_path,
        destination_path,
        original_sha256: string_field(object, &["originalSha256", "sha256", "hash"])
            .map(str::to_string),
        normalized_url: string_field(object, &["normalizedUrl", "url"]).map(str::to_string),
        recorded_content_sha256: string_field(object, &["recordedContentSha256", "contentSha256"])
            .map(str::to_string),
        metadata_path: metadata_path.into(),
    })
}

fn path_field(
    object: &serde_json::Map<String, Value>,
    names: &[&str],
    metadata_path: &str,
    warnings: &mut Vec<MigrationWarning>,
) -> Option<String> {
    string_field(object, names).and_then(|value| sanitize_path(value, metadata_path, warnings))
}

fn string_field<'a>(object: &'a serde_json::Map<String, Value>, names: &[&str]) -> Option<&'a str> {
    names
        .iter()
        .find_map(|name| object.get(*name).and_then(Value::as_str))
}

fn sanitize_path(
    value: &str,
    metadata_path: &str,
    warnings: &mut Vec<MigrationWarning>,
) -> Option<String> {
    let normalized = value.replace('\\', "/");
    if validate_project_relative(&normalized).is_err() {
        warnings.push(MigrationWarning {
            code: "MIGRATION_PATH_INVALID".into(),
            message: "A legacy path was invalid and was withheld from automatic migration.".into(),
            relative_path: Some(metadata_path.into()),
            redacted: false,
        });
        None
    } else {
        Some(normalized)
    }
}

fn safe_metadata(path: &Path) -> Result<fs::Metadata, std::io::Error> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || is_project_reparse_point(&metadata) {
        Err(std::io::Error::new(
            ErrorKind::PermissionDenied,
            "link is not a file",
        ))
    } else {
        Ok(metadata)
    }
}

fn stable_record_id(metadata_path: &str, key: &str) -> String {
    format!(
        "legacy-{}",
        &sha256(format!("{metadata_path}\n{key}").as_bytes())[..24]
    )
}

fn fingerprint(
    project_identity: &str,
    records: &[LegacyRecord],
    files: &[LegacyFileEvidence],
    warnings: &[MigrationWarning],
) -> String {
    let bytes =
        serde_json::to_vec(&(project_identity, records, files, warnings)).unwrap_or_default();
    sha256(&bytes)
}

fn project_identity(root: &Path, metadata: &fs::Metadata) -> String {
    let canonical = root
        .canonicalize()
        .unwrap_or_else(|_| root.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/");
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    sha256(format!("{canonical}\n{modified}").as_bytes())
}

fn sha256(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
