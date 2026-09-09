//! Operation history over application-owned Git snapshots. Call mutations under
//! the project's write permit; a list query never reads content or runs Git.
use std::collections::BTreeMap;
use std::fs;

use crate::errors::BackendError;
use crate::models::paths::ProjectContext;
use crate::models::version_history::*;
use crate::services::{CompileService, FileStore, GitService};

const SCHEMA_VERSION: u32 = 1;
const MAX_CAPTURE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_CAPTURE_PATHS: usize = 10_000;
const MAX_DIFF_TEXT_BYTES: usize = 256 * 1024;

#[derive(Debug, Default, Clone, Copy)]
pub struct VersionHistoryService;

fn error(code: &str, message: impl Into<String>) -> BackendError {
    BackendError::new(code, message, true, true)
}

#[cfg(test)]
mod tests;

fn validate_id(id: &str) -> Result<(), BackendError> {
    let (time, uuid) = id
        .split_once('-')
        .ok_or_else(|| error("VERSION_ID_INVALID", "Invalid operation id."))?;
    if time.len() != 16
        || !time.bytes().all(|b| b.is_ascii_digit())
        || uuid::Uuid::parse_str(uuid).is_err()
    {
        return Err(error("VERSION_ID_INVALID", "Invalid operation id."));
    }
    Ok(())
}

fn history_root(context: &ProjectContext) -> Result<String, BackendError> {
    context
        .layout
        .app_state_root
        .as_ref()
        .map(|root| format!("{root}/version-history"))
        .ok_or_else(|| {
            error(
                "VERSION_LAYOUT_UNAVAILABLE",
                "Enable this knowledge base before saving versions.",
            )
        })
}

fn record_path(context: &ProjectContext, id: &str) -> Result<String, BackendError> {
    validate_id(id)?;
    Ok(format!("{}/operations/{id}.json", history_root(context)?))
}

fn hashes(files: &BTreeMap<String, Option<Vec<u8>>>) -> BTreeMap<String, Option<String>> {
    files
        .iter()
        .map(|(path, bytes)| {
            (
                path.clone(),
                bytes.as_ref().map(|bytes| FileStore.content_hash(bytes)),
            )
        })
        .collect()
}

impl VersionHistoryService {
    /// One small transaction for plain Wiki mutations. Domains supply the
    /// exact candidate and baseline; there is no project-wide staging.
    pub fn apply_wiki_files(
        &self,
        context: &ProjectContext,
        kind: VersionOperationKind,
        expected: &BTreeMap<String, Option<String>>,
        outputs: &BTreeMap<String, Option<Vec<u8>>>,
    ) -> Result<VersionOperation, BackendError> {
        if expected.keys().ne(outputs.keys()) {
            return Err(error(
                "VERSION_SCOPE_CHANGED",
                "The candidate and baseline have different file scopes.",
            ));
        }
        if outputs
            .values()
            .flatten()
            .try_fold(0u64, |total, bytes| total.checked_add(bytes.len() as u64))
            .is_none_or(|total| total > MAX_CAPTURE_BYTES)
        {
            return Err(error(
                "VERSION_SIZE_LIMIT",
                "The candidate exceeds the supported recovery budget.",
            ));
        }
        for path in outputs.keys() {
            context.resolve_wiki_write_path(path)?;
            if Some(path.as_str()) == context.layout.activity_log_path.as_deref() {
                return Err(error(
                    "VERSION_PATH_UNSAFE",
                    "Activity logs cannot be replaced by a content operation.",
                ));
            }
            if context
                .layout
                .source_paths()
                .is_ok_and(|source| source.contains_source_markdown(path))
            {
                return Err(error(
                    "VERSION_DOMAIN_RESTORE_REQUIRED",
                    "Source changes require their Source transaction.",
                ));
            }
        }
        let paths = outputs.keys().cloned().collect::<Vec<_>>();
        let mut record = self.begin(context, kind, &paths, None)?;
        if &record.before_hashes != expected {
            record.summary.state = VersionOperationState::Aborted;
            self.persist(context, &record)?;
            return Err(error(
                "FILE_HASH_MISMATCH",
                "A file changed before the recovery version was saved.",
            ));
        }
        record.expected_hashes = hashes(outputs);
        self.persist(context, &record)?;
        let result = (|| {
            let mut transaction =
                crate::services::import_v2::transaction::FileTransaction::new_for_context(context)?;
            for (path, bytes) in outputs {
                let absolute = context.resolve_wiki_write_path(path)?;
                match (bytes, expected.get(path).and_then(Option::as_ref)) {
                    (Some(bytes), Some(hash)) => {
                        transaction.write_if_hash_matches(&absolute, bytes, hash)?
                    }
                    (Some(bytes), None) => transaction.write_new(&absolute, bytes)?,
                    (None, Some(hash)) => transaction.delete_if_hash_matches(&absolute, hash)?,
                    (None, None) => {}
                }
            }
            transaction.commit()?;
            self.finish(context, &mut record)?;
            Ok::<(), BackendError>(())
        })();
        if let Err(failure) = result {
            let preserved = self.rollback_failed(context, &record.summary.operation_id)?;
            return Err(error("VERSION_APPLY_FAILED", "The operation could not finish; review its recovery record.")
                .with_details(serde_json::json!({ "operationId":record.summary.operation_id, "originalCode":failure.code, "preservedPaths":preserved })));
        }
        Ok(record)
    }

    /// Refuse oversized inputs before materializing any file. This bounded
    /// text path must not be reused as an unbounded binary archive reader.
    pub fn capture(
        &self,
        context: &ProjectContext,
        paths: &[String],
    ) -> Result<BTreeMap<String, Option<Vec<u8>>>, BackendError> {
        if paths.len() > MAX_CAPTURE_PATHS {
            return Err(error(
                "VERSION_SIZE_LIMIT",
                "Too many files in this recovery version.",
            ));
        }
        let mut total = 0u64;
        for path in paths {
            let absolute = context.resolve_project_path(path)?;
            match fs::symlink_metadata(&absolute) {
                Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                    total = total.checked_add(metadata.len()).ok_or_else(|| {
                        error("VERSION_SIZE_LIMIT", "Recovery version is too large.")
                    })?;
                    if total > MAX_CAPTURE_BYTES {
                        return Err(error(
                            "VERSION_SIZE_LIMIT",
                            "This recovery version exceeds the supported content budget.",
                        ));
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Ok(_) => {
                    return Err(error(
                        "VERSION_PATH_UNSAFE",
                        "Recovery versions require regular project files.",
                    ))
                }
                Err(e) => return Err(error("VERSION_READ_FAILED", e.to_string())),
            }
        }
        let mut captured = BTreeMap::new();
        let mut remaining = MAX_CAPTURE_BYTES;
        for path in paths {
            let absolute = context.resolve_project_path(path)?;
            let bytes = match fs::symlink_metadata(&absolute) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                _ => Some(FileStore.read_bytes_bounded(context, path, remaining)?),
            };
            remaining -= bytes.as_ref().map_or(0, |bytes| bytes.len() as u64);
            captured.insert(path.clone(), bytes);
        }
        Ok(captured)
    }

    pub fn begin(
        &self,
        context: &ProjectContext,
        kind: VersionOperationKind,
        paths: &[String],
        task_id: Option<String>,
    ) -> Result<VersionOperation, BackendError> {
        if paths.is_empty() {
            return Err(error(
                "VERSION_EMPTY",
                "No files were selected for this version.",
            ));
        }
        GitService.require_local_history(context)?;
        let identity = super::project_identity(&context.root)
            .map_err(|message| error("PROJECT_IDENTITY_FAILED", message))?;
        let files = self.capture(context, paths)?;
        let git_id = uuid::Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now();
        let id = format!("{:016}-{git_id}", created_at.timestamp_micros());
        let before = GitService
            .create_history_snapshot(
                context,
                &git_id,
                "before",
                "Before application operation",
                None,
                &files,
            )?
            .commit_hash
            .ok_or_else(|| {
                error(
                    "VERSION_SNAPSHOT_MISSING",
                    "The recovery snapshot could not be saved.",
                )
            })?;
        let before_hashes = hashes(&files);
        let record = VersionOperation {
            schema_version: SCHEMA_VERSION,
            restoration_id: None,
            restored_from: None,
            source_deletion: None,
            summary: VersionOperationSummary {
                operation_id: id,
                kind,
                created_at: created_at.to_rfc3339(),
                state: VersionOperationState::Prepared,
                file_count: files.len(),
                task_id,
            },
            project_identity: identity.canonical_identity_key,
            identity_revision: identity.identity_revision,
            git_id,
            before,
            after: None,
            expected_hashes: before_hashes.clone(),
            before_hashes,
        };
        self.persist(context, &record)?;
        Ok(record)
    }

    pub fn load(
        &self,
        context: &ProjectContext,
        id: &str,
    ) -> Result<VersionOperation, BackendError> {
        let bytes =
            FileStore.read_bytes_bounded(context, &record_path(context, id)?, 8 * 1024 * 1024)?;
        let record: VersionOperation = serde_json::from_slice(&bytes).map_err(|_| {
            error(
                "VERSION_RECORD_INVALID",
                "The recovery record cannot be read.",
            )
        })?;
        let identity = super::project_identity(&context.root)
            .map_err(|message| error("PROJECT_IDENTITY_FAILED", message))?;
        if record.schema_version != SCHEMA_VERSION
            || record.summary.operation_id != id
            || record.project_identity != identity.canonical_identity_key
            || record.identity_revision != identity.identity_revision
            || record
                .before_hashes
                .keys()
                .ne(record.expected_hashes.keys())
        {
            return Err(error("VERSION_RECORD_INVALID", "This version does not belong to the current knowledge base or uses an unsupported format."));
        }
        uuid::Uuid::parse_str(&record.git_id)
            .map_err(|_| error("VERSION_RECORD_INVALID", "Invalid history reference."))?;
        if record.before_hashes.len() > MAX_CAPTURE_PATHS
            || record.summary.file_count != record.before_hashes.len()
        {
            return Err(error(
                "VERSION_RECORD_INVALID",
                "The recorded file scope is invalid.",
            ));
        }
        for path in record.before_hashes.keys() {
            context.resolve_project_path(path)?;
        }
        Ok(record)
    }

    pub(crate) fn validate_snapshots(
        &self,
        context: &ProjectContext,
        record: &VersionOperation,
    ) -> Result<(), BackendError> {
        if GitService
            .history_snapshot(context, &record.git_id, "before")?
            .as_deref()
            != Some(&record.before)
            || (record.after.is_some()
                && GitService.history_snapshot(context, &record.git_id, "after")? != record.after)
        {
            return Err(error(
                "VERSION_RECORD_INVALID",
                "The recovery references do not match this record.",
            ));
        }
        Ok(())
    }

    pub fn persist(
        &self,
        context: &ProjectContext,
        record: &VersionOperation,
    ) -> Result<(), BackendError> {
        FileStore.write_json_atomic(
            context,
            &record_path(context, &record.summary.operation_id)?,
            record,
        )?;
        let summary_path = format!(
            "{}/summaries/{}.json",
            history_root(context)?,
            record.summary.operation_id
        );
        // The canonical record is durable. A derived index failure must not
        // misreport a successful file mutation or trigger compensation.
        if let Err(failure) = FileStore.write_json_atomic(context, &summary_path, &record.summary) {
            eprintln!("Version summary indexing failed: {}", failure.code);
        }
        Ok(())
    }

    /// Persist the exact write intent before the domain mutates this path.
    pub fn plan_write(
        &self,
        context: &ProjectContext,
        record: &mut VersionOperation,
        path: &str,
        contents: Option<&[u8]>,
    ) -> Result<(), BackendError> {
        if record.summary.state != VersionOperationState::Prepared
            || !record.before_hashes.contains_key(path)
        {
            return Err(error(
                "VERSION_SCOPE_CHANGED",
                "The write is outside its prepared recovery version.",
            ));
        }
        // Lint supplies one candidate at a time. Account for the complete
        // current scope using metadata, without rereading all document bytes.
        let mut total = contents.map_or(0, |bytes| bytes.len() as u64);
        for other in record
            .before_hashes
            .keys()
            .filter(|other| other.as_str() != path)
        {
            match fs::symlink_metadata(context.resolve_project_path(other)?) {
                Ok(metadata) if metadata.is_file() => {
                    total = total.saturating_add(metadata.len());
                }
                Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => {}
                _ => {
                    return Err(error(
                        "VERSION_PATH_UNSAFE",
                        "The recovery scope contains an unreadable or unsafe file.",
                    ))
                }
            }
        }
        if total > MAX_CAPTURE_BYTES {
            return Err(error(
                "VERSION_SIZE_LIMIT",
                "The candidate exceeds the supported recovery budget.",
            ));
        }
        record.expected_hashes.insert(
            path.into(),
            contents.map(|bytes| FileStore.content_hash(bytes)),
        );
        self.persist(context, record)
    }

    pub fn finish(
        &self,
        context: &ProjectContext,
        record: &mut VersionOperation,
    ) -> Result<(), BackendError> {
        let paths = record.before_hashes.keys().cloned().collect::<Vec<_>>();
        let files = self.capture(context, &paths)?;
        if hashes(&files) != record.expected_hashes {
            return Err(error(
                "VERSION_RESULT_CHANGED",
                "A file changed before the operation result could be verified.",
            ));
        }
        record.after = GitService
            .create_history_snapshot(
                context,
                &record.git_id,
                "after",
                "Application operation completed",
                Some(&record.before),
                &files,
            )?
            .commit_hash;
        record.summary.state = VersionOperationState::Applied;
        self.persist(context, record)
    }

    /// Compensate only bytes owned by the failed operation. Externally edited
    /// paths remain in the durable record for explicit recovery, never reset.
    pub(crate) fn rollback_failed(
        &self,
        context: &ProjectContext,
        id: &str,
    ) -> Result<Vec<String>, BackendError> {
        let mut record = self.load(context, id)?;
        let paths = record.before_hashes.keys().cloned().collect::<Vec<_>>();
        self.validate_snapshots(context, &record)?;
        let before = GitService.read_history_files_bounded(
            context,
            &record.before,
            &paths,
            MAX_CAPTURE_BYTES as usize,
        )?;
        if hashes(&before) != record.before_hashes {
            return Err(error(
                "VERSION_RECORD_INVALID",
                "The recovery snapshot changed.",
            ));
        }
        let current = self.capture(context, &paths)?;
        let current_hashes = hashes(&current);
        let mut preserved = Vec::new();
        let mut owned_before = BTreeMap::new();
        let mut owned_after = BTreeMap::new();
        for path in paths {
            if current_hashes.get(&path) == record.expected_hashes.get(&path)
                || current_hashes.get(&path) == record.before_hashes.get(&path)
            {
                context.resolve_wiki_write_path(&path)?;
                owned_before.insert(path.clone(), before[&path].clone());
                owned_after.insert(path.clone(), current[&path].clone());
            } else {
                preserved.push(path);
            }
        }
        let prepared =
            CompileService::prepare_history_restore(context, &owned_before, &owned_after)?;
        record.summary.state = VersionOperationState::Restoring;
        self.persist(context, &record)?;
        CompileService::restore_prepared_history_outputs(context, &prepared)?;
        if preserved.is_empty() {
            record.summary.state = VersionOperationState::Aborted;
        }
        self.persist(context, &record)?;
        Ok(preserved)
    }

    pub fn list(
        &self,
        context: &ProjectContext,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<VersionHistoryPage, BackendError> {
        let identity = super::project_identity(&context.root)
            .map_err(|message| error("PROJECT_IDENTITY_FAILED", message))?;
        let scope = FileStore.content_hash(
            format!(
                "{}:{}",
                identity.canonical_identity_key, identity.identity_revision
            )
            .as_bytes(),
        );
        let cursor = cursor
            .map(|cursor| {
                let (cursor_scope, id) = cursor
                    .split_once(':')
                    .ok_or_else(|| error("VERSION_CURSOR_INVALID", "Invalid history page."))?;
                if cursor_scope != scope {
                    return Err(error(
                        "VERSION_CURSOR_INVALID",
                        "This history page belongs to another knowledge base.",
                    ));
                }
                validate_id(id)?;
                Ok(id)
            })
            .transpose()?;
        let directory =
            context.resolve_project_path(&format!("{}/operations", history_root(context)?))?;
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(VersionHistoryPage {
                    operations: vec![],
                    next_cursor: None,
                    unreadable_count: 0,
                })
            }
            Err(e) => return Err(error("VERSION_LIST_FAILED", e.to_string())),
        };
        let mut ids = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                if !entry.file_type().ok()?.is_file() {
                    return None;
                }
                let name = entry
                    .file_name()
                    .to_str()?
                    .strip_suffix(".json")?
                    .to_owned();
                (validate_id(&name).is_ok() && cursor.is_none_or(|cursor| name.as_str() < cursor))
                    .then_some(name)
            })
            .collect::<Vec<_>>();
        ids.sort_unstable_by(|a, b| b.cmp(a));
        let limit = limit.clamp(1, 100);
        let has_more = ids.len() > limit;
        ids.truncate(limit);
        let next_cursor = has_more
            .then(|| ids.last().map(|id| format!("{scope}:{id}")))
            .flatten();
        let mut operations = Vec::with_capacity(ids.len());
        let mut unreadable_count = 0;
        for id in ids {
            let path = format!("{}/summaries/{id}.json", history_root(context)?);
            let canonical = record_path(context, &id)?;
            let fresh = || -> Option<bool> {
                let summary_time = fs::metadata(context.resolve_project_path(&path).ok()?)
                    .ok()?
                    .modified()
                    .ok()?;
                let record_time = fs::metadata(context.resolve_project_path(&canonical).ok()?)
                    .ok()?
                    .modified()
                    .ok()?;
                Some(summary_time >= record_time)
            };
            let summary = if fresh() == Some(true) {
                FileStore
                    .read_bytes_bounded(context, &path, 64 * 1024)
                    .and_then(|bytes| {
                        serde_json::from_slice::<VersionOperationSummary>(&bytes).map_err(|_| {
                            error("VERSION_RECORD_INVALID", "Invalid operation summary.")
                        })
                    })
            } else {
                self.load(context, &id).map(|record| record.summary)
            };
            match summary {
                Ok(summary) if summary.operation_id == id => operations.push(summary),
                _ => unreadable_count += 1,
            }
        }
        Ok(VersionHistoryPage {
            operations,
            next_cursor,
            unreadable_count,
        })
    }

    /// Manual versions cover editable Wiki Markdown only. Source packages,
    /// generated indexes, logs and attachments retain their own lifecycles.
    pub fn manual_scope(&self, context: &ProjectContext) -> Result<Vec<String>, BackendError> {
        use crate::models::layout::ProjectMarkdownRootRole;
        let files = context.list_markdown_files_for_roles(&[
            ProjectMarkdownRootRole::Wiki,
            ProjectMarkdownRootRole::Mixed,
        ])?;
        let mut paths = Vec::new();
        for file in files {
            let path = context.to_project_relative(&file)?;
            if context.resolve_wiki_write_path(&path).is_err()
                || Some(path.as_str()) == context.layout.activity_log_path.as_deref()
                || Some(path.as_str()) == context.layout.wiki_index_path.as_deref()
                || context
                    .layout
                    .source_paths()
                    .is_ok_and(|source| source.contains_source_markdown(&path))
            {
                continue;
            }
            paths.push(path);
        }
        paths.sort();
        paths.dedup();
        if paths.is_empty() {
            return Err(error(
                "VERSION_EMPTY",
                "No editable Wiki pages are available to save.",
            ));
        }
        Ok(paths)
    }

    pub fn save_manual(
        &self,
        context: &ProjectContext,
        expected: &BTreeMap<String, Option<String>>,
    ) -> Result<VersionOperation, BackendError> {
        let paths = self.manual_scope(context)?;
        if paths.iter().ne(expected.keys()) {
            return Err(error(
                "VERSION_SCOPE_CHANGED",
                "The selection changed after preview.",
            ));
        }
        let mut record = self.begin(context, VersionOperationKind::ManualSnapshot, &paths, None)?;
        if &record.before_hashes != expected {
            record.summary.state = VersionOperationState::Aborted;
            self.persist(context, &record)?;
            return Err(error(
                "VERSION_SCOPE_CHANGED",
                "A page changed after preview.",
            ));
        }
        self.finish(context, &mut record)?;
        Ok(record)
    }

    fn recovery_record(
        &self,
        context: &ProjectContext,
        record: &VersionOperation,
    ) -> Result<Option<VersionOperation>, BackendError> {
        let recovery = record
            .restoration_id
            .as_deref()
            .map(|id| self.load(context, id))
            .transpose()?;
        if recovery.as_ref().is_some_and(|recovery| {
            recovery.restored_from.as_deref() != Some(record.summary.operation_id.as_str())
                || recovery.expected_hashes != record.before_hashes
        }) {
            return Err(error(
                "VERSION_RECORD_INVALID",
                "The restoration does not match its original operation.",
            ));
        }
        Ok(recovery)
    }

    fn accepts_restore_current(
        record: &VersionOperation,
        recovery: Option<&VersionOperation>,
        path: &str,
        current: &Option<String>,
    ) -> bool {
        if let Some(recovery) = recovery {
            return recovery.before_hashes.get(path) == Some(current)
                || recovery.expected_hashes.get(path) == Some(current);
        }
        if record.summary.kind == VersionOperationKind::ManualSnapshot {
            return true;
        }
        record.expected_hashes.get(path) == Some(current)
            || (matches!(
                record.summary.state,
                VersionOperationState::Prepared | VersionOperationState::Restoring
            ) && record.before_hashes.get(path) == Some(current))
    }

    pub fn preview_restore(
        &self,
        context: &ProjectContext,
        id: &str,
    ) -> Result<VersionRestorePreview, BackendError> {
        let record = self.load(context, id)?;
        let recovery = self.recovery_record(context, &record)?;
        let mut conflicts = Vec::new();
        let captured = self.capture(
            context,
            &record.before_hashes.keys().cloned().collect::<Vec<_>>(),
        )?;
        let actual = hashes(&captured);
        for path in record.expected_hashes.keys() {
            let current = actual[path].clone();
            if !Self::accepts_restore_current(&record, recovery.as_ref(), path, &current) {
                conflicts.push(path.clone());
            }
        }
        Ok(VersionRestorePreview {
            operation_id: id.into(),
            paths: record.before_hashes.keys().cloned().collect(),
            conflicts,
            already_restored: matches!(
                record.summary.state,
                VersionOperationState::Restored | VersionOperationState::Aborted
            ),
        })
    }

    /// Only plain Wiki content is restored here. Source/index ownership stays
    /// with the domain-specific restoration commands.
    pub fn restore(
        &self,
        context: &ProjectContext,
        id: &str,
    ) -> Result<VersionOperation, BackendError> {
        self.restore_confirmed(context, id, None)
    }

    pub fn restore_confirmed(
        &self,
        context: &ProjectContext,
        id: &str,
        expected_current: Option<&BTreeMap<String, Option<String>>>,
    ) -> Result<VersionOperation, BackendError> {
        let mut record = self.load(context, id)?;
        if matches!(
            record.summary.state,
            VersionOperationState::Restored | VersionOperationState::Aborted
        ) {
            return Ok(record);
        }
        if !matches!(
            record.summary.kind,
            VersionOperationKind::LintFix
                | VersionOperationKind::ManualSnapshot
                | VersionOperationKind::PageChange
                | VersionOperationKind::ChatEdit
                | VersionOperationKind::Restore
        ) {
            return Err(error(
                "VERSION_DOMAIN_RESTORE_REQUIRED",
                "Restore this operation from its original task.",
            ));
        }
        for path in record.before_hashes.keys() {
            context.resolve_wiki_write_path(path)?;
            if Some(path.as_str()) == context.layout.activity_log_path.as_deref() {
                return Err(error(
                    "VERSION_PATH_UNSAFE",
                    "Activity logs cannot be restored as content.",
                ));
            }
            if context
                .layout
                .source_paths()
                .is_ok_and(|paths| paths.contains_source_markdown(path))
            {
                return Err(error(
                    "VERSION_DOMAIN_RESTORE_REQUIRED",
                    "Source content requires Source version restoration.",
                ));
            }
        }
        let preview = self.preview_restore(context, id)?;
        if !preview.conflicts.is_empty() {
            return Err(error(
                "VERSION_RESTORE_CONFLICT",
                "These files have newer edits. Their current contents were preserved.",
            )
            .with_details(serde_json::json!({"paths":preview.conflicts})));
        }
        let paths = preview.paths;
        self.validate_snapshots(context, &record)?;
        let before = GitService.read_history_files_bounded(
            context,
            &record.before,
            &paths,
            MAX_CAPTURE_BYTES as usize,
        )?;
        if hashes(&before) != record.before_hashes {
            return Err(error(
                "VERSION_RECORD_INVALID",
                "The snapshot does not match its recovery record.",
            ));
        }
        let current = self.capture(context, &paths)?;
        let current_hashes = hashes(&current);
        if expected_current.is_some_and(|expected| expected != &current_hashes) {
            return Err(error(
                "VERSION_RESTORE_CONFLICT",
                "A file changed after the recovery preview.",
            ));
        }
        if record.summary.kind == VersionOperationKind::ManualSnapshot && expected_current.is_none()
        {
            return Err(error(
                "VERSION_CONFIRMATION_REQUIRED",
                "Preview current files before restoring a saved version.",
            ));
        }
        let recovery = self.recovery_record(context, &record)?;
        for path in &paths {
            if !Self::accepts_restore_current(
                &record,
                recovery.as_ref(),
                path,
                &current_hashes[path],
            ) {
                return Err(error(
                    "VERSION_RESTORE_CONFLICT",
                    "A file changed during recovery preparation.",
                )
                .with_details(serde_json::json!({"paths":[path]})));
            }
        }
        let prepared = CompileService::prepare_history_restore(context, &before, &current)?;
        let mut recovery = if let Some(recovery_id) = record.restoration_id.as_deref() {
            self.load(context, recovery_id)?
        } else {
            let mut recovery = self.begin(
                context,
                VersionOperationKind::Restore,
                &paths,
                record.summary.task_id.clone(),
            )?;
            if recovery.before_hashes != current_hashes {
                return Err(error(
                    "VERSION_RESTORE_CONFLICT",
                    "A file changed while its recovery version was being saved.",
                ));
            }
            recovery.restored_from = Some(id.into());
            recovery.expected_hashes = record.before_hashes.clone();
            self.persist(context, &recovery)?;
            record.restoration_id = Some(recovery.summary.operation_id.clone());
            record.summary.state = VersionOperationState::Restoring;
            self.persist(context, &record)?;
            recovery
        };
        CompileService::restore_prepared_history_outputs(context, &prepared)?;
        self.finish(context, &mut recovery)?;
        record.summary.state = VersionOperationState::Restored;
        self.persist(context, &record)?;
        if let Some(cache) = context.layout.graph_cache_path.as_deref() {
            if let Ok(absolute) = context.resolve_project_write_path(cache) {
                let _ =
                    crate::utils::safe_project_dir::remove_project_file(&context.root, &absolute);
            }
        }
        Ok(record)
    }

    pub fn file_diff(
        &self,
        context: &ProjectContext,
        id: &str,
        path: &str,
    ) -> Result<VersionFileDiff, BackendError> {
        let record = self.load(context, id)?;
        if !record.before_hashes.contains_key(path) {
            return Err(error(
                "VERSION_PATH_UNKNOWN",
                "This file is not part of the operation.",
            ));
        }
        self.validate_snapshots(context, &record)?;
        let paths = vec![path.to_string()];
        let saved = GitService
            .read_history_files_bounded(
                context,
                &record.before,
                &paths,
                MAX_CAPTURE_BYTES as usize,
            )?
            .remove(path)
            .flatten();
        let (before, after) = if record.summary.kind == VersionOperationKind::ManualSnapshot {
            (self.capture(context, &paths)?.remove(path).flatten(), saved)
        } else {
            (
                saved,
                match record.after {
                    Some(after) => GitService
                        .read_history_files_bounded(
                            context,
                            &after,
                            &paths,
                            MAX_CAPTURE_BYTES as usize,
                        )?
                        .remove(path)
                        .flatten(),
                    None => self.capture(context, &paths)?.remove(path).flatten(),
                },
            )
        };
        let binary = before
            .iter()
            .chain(after.iter())
            .any(|bytes| bytes.contains(&0) || std::str::from_utf8(bytes).is_err());
        let truncated = before
            .iter()
            .chain(after.iter())
            .any(|bytes| bytes.len() > MAX_DIFF_TEXT_BYTES);
        let text = |bytes: &Option<Vec<u8>>| {
            bytes.as_ref().filter(|_| !binary).map(|bytes| {
                let text = std::str::from_utf8(bytes).unwrap_or_default();
                let mut end = text.len().min(MAX_DIFF_TEXT_BYTES);
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                text[..end].to_owned()
            })
        };
        Ok(VersionFileDiff {
            path: path.into(),
            before_text: text(&before),
            after_text: text(&after),
            before_bytes: before.as_ref().map_or(0, Vec::len),
            after_bytes: after.as_ref().map_or(0, Vec::len),
            binary,
            truncated,
        })
    }
}
