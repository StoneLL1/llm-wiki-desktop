use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::errors::BackendError;
use crate::models::import_v2::{
    ImportBatchResult, ImportItem, ImportItemCommitResult, ImportResourceMode, ImportSession,
};
use crate::models::import_v2_migration::{LegacyHistoryEntry, LegacyHistoryWarning};
use crate::models::import_v2_presentation::{
    ImportHistoryAction, ImportHistoryDetailPage, ImportHistoryEntry, ImportHistoryPage,
};
use crate::models::paths::ProjectContext;
use crate::services::import_v2::migration::LegacyHistoryAdapter;
use crate::services::import_v2::transaction::FileTransaction;
use crate::services::import_v2::ImportV2Service;
use crate::services::FileStore;

const HISTORY_SCHEMA_VERSION: u32 = 1;
const HISTORY_INDEX_PAGE_SIZE: usize = 50;
const HISTORY_DETAIL_PAGE_SIZE: usize = 50;
const HISTORY_WARNING_LIMIT: usize = 10;

#[derive(Debug, Clone, Default)]
pub struct HistoryStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryIndexManifest {
    schema_version: u32,
    revision: u64,
    #[serde(default = "live_generation")]
    generation: String,
    entry_count: u64,
    page_count: u64,
    #[serde(default)]
    warnings: Vec<LegacyHistoryWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryIndexPage {
    schema_version: u32,
    #[serde(default = "live_generation")]
    generation: String,
    page_index: u64,
    records: Vec<HistoryIndexRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryIndexLocation {
    schema_version: u32,
    generation: String,
    page_index: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum HistoryIndexRecord {
    V2 { entry: ImportHistoryEntry },
    Legacy { entry: LegacyHistoryEntry },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryWorkingManifest {
    schema_version: u32,
    revision: u64,
    entry: ImportHistoryEntry,
    detail_page_count: u64,
    resource_mode: ImportResourceMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryDetailOrderPage {
    schema_version: u32,
    batch_id: String,
    page_index: u64,
    item_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryItemSnapshot {
    schema_version: u32,
    batch_id: String,
    item: ImportItem,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryItemReceipt {
    schema_version: u32,
    batch_id: String,
    sequence: u64,
    result: ImportItemCommitResult,
    recorded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryListCursor {
    version: u8,
    revision: u64,
    before: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryDetailCursor {
    version: u8,
    batch_id: String,
    revision: u64,
    after: u64,
}

impl HistoryStore {
    pub fn begin_batch(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        batch: &ImportBatchResult,
    ) -> Result<(), BackendError> {
        self.write_working_batch(context, files, batch)?;
        self.upsert_index_entry(context, files, entry_from_batch(batch))
    }

    pub(crate) fn stage_result(
        &self,
        context: &ProjectContext,
        transaction: &mut FileTransaction,
        batch: &ImportBatchResult,
        result: &ImportItemCommitResult,
        item: &ImportItem,
        sequence: usize,
        committed_count: u32,
        failed_count: u32,
        manifest_expected_hash: &str,
        snapshot_expected_hash: &str,
    ) -> Result<(), BackendError> {
        validate_id(&batch.batch_id)?;
        validate_id(&result.item_id)?;
        let root = working_root(context, &batch.batch_id)?;
        if item.item_id != result.item_id {
            return Err(history_error("The historical item snapshot is invalid."));
        }
        let (receipt, snapshot, manifest) =
            staged_result_payloads(batch, result, item, sequence, committed_count, failed_count)?;
        transaction.write_new(
            &context.resolve_project_path(&format!(
                "{root}/results/{sequence:08}-{}.json",
                result.item_id
            ))?,
            &receipt,
        )?;
        transaction.write_if_hash_matches(
            &context.resolve_project_path(&format!("{root}/snapshots/{}.json", result.item_id))?,
            &snapshot,
            snapshot_expected_hash,
        )?;
        transaction.write_if_hash_matches(
            &context.resolve_project_path(&format!("{root}/manifest.json"))?,
            &manifest,
            manifest_expected_hash,
        )
    }

    pub(crate) fn persist_result(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        batch: &ImportBatchResult,
        result: &ImportItemCommitResult,
        item: &ImportItem,
        sequence: usize,
    ) -> Result<(), BackendError> {
        let root = working_root(context, &batch.batch_id)?;
        let manifest_path = format!("{root}/manifest.json");
        let snapshot_path = format!("{root}/snapshots/{}.json", result.item_id);
        let manifest_hash = files.file_hash(context, &manifest_path)?;
        let snapshot_hash = files.file_hash(context, &snapshot_path)?;
        let mut transaction = FileTransaction::new_for_context(context)?;
        self.stage_result(
            context,
            &mut transaction,
            batch,
            result,
            item,
            sequence,
            batch.committed_count,
            batch.failed_count,
            &manifest_hash,
            &snapshot_hash,
        )?;
        transaction.commit()
    }

    pub(crate) fn persist_result_if_unchanged(
        &self,
        context: &ProjectContext,
        batch: &ImportBatchResult,
        result: &ImportItemCommitResult,
        item: &ImportItem,
        sequence: usize,
        committed_count: u32,
        failed_count: u32,
        manifest_expected_hash: &str,
        snapshot_expected_hash: &str,
    ) -> Result<(), BackendError> {
        let mut transaction = FileTransaction::new_for_context(context)?;
        self.stage_result(
            context,
            &mut transaction,
            batch,
            result,
            item,
            sequence,
            committed_count,
            failed_count,
            manifest_expected_hash,
            snapshot_expected_hash,
        )?;
        transaction.commit()
    }

    pub(crate) fn finalize_batch(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        batch: &ImportBatchResult,
    ) -> Result<(), BackendError> {
        validate_id(&batch.batch_id)?;
        let compatibility_path = format!("{}/{}.json", history_root(context)?, batch.batch_id);
        let mut transaction = FileTransaction::new_for_context(context)?;
        transaction.write_new(
            &context.resolve_project_path(&compatibility_path)?,
            &json_bytes(batch)?,
        )?;
        transaction.commit()?;
        self.upsert_index_entry(context, files, entry_from_batch(batch))
    }

    pub(crate) fn load_compatibility_batch(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        batch_id: &str,
    ) -> Result<ImportBatchResult, BackendError> {
        validate_id(batch_id)?;
        files.read_json(
            context,
            &format!("{}/{}.json", history_root(context)?, batch_id),
        )
    }

    pub fn list_page(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<ImportHistoryPage, BackendError> {
        if limit == 0 || limit as usize > HISTORY_INDEX_PAGE_SIZE {
            return Err(history_error(
                "History page limit must be between 1 and 50.",
            ));
        }
        let manifest_path = index_manifest_path(context)?;
        if !files.exists(context, &manifest_path) {
            let mut page = fallback_history_page(context)?;
            if history_data_exists(context)? {
                page.warnings.push(rebuild_warning(context)?);
            }
            return Ok(page);
        }
        let manifest: HistoryIndexManifest = match files.read_json(context, &manifest_path) {
            Ok(manifest) if validate_index_manifest(&manifest).is_ok() => manifest,
            _ => {
                let mut page = fallback_history_page(context)?;
                page.warnings.push(rebuild_warning(context)?);
                return Ok(page);
            }
        };
        let mut before = if let Some(value) = cursor {
            let cursor: HistoryListCursor = serde_json::from_str(value)
                .map_err(|_| history_error("History cursor is invalid."))?;
            if cursor.version != 1 || cursor.revision != manifest.revision {
                return Err(BackendError::new(
                    "IMPORT_V2_HISTORY_CURSOR_STALE",
                    "Import history changed while this page was being read.",
                    true,
                    false,
                ));
            }
            cursor.before
        } else {
            manifest.entry_count
        };
        if before > manifest.entry_count {
            return Err(history_error("History cursor is invalid."));
        }

        let mut records = Vec::with_capacity(limit as usize);
        while before > 0 && records.len() < limit as usize {
            let ordinal = before - 1;
            let page_index = ordinal / HISTORY_INDEX_PAGE_SIZE as u64;
            let page: HistoryIndexPage = match files.read_json(
                context,
                &index_page_path(context, &manifest.generation, page_index)?,
            ) {
                Ok(page)
                    if validate_index_page(&page, page_index, &manifest.generation).is_ok() =>
                {
                    page
                }
                _ => {
                    let mut page = fallback_history_page(context)?;
                    page.warnings.push(rebuild_warning(context)?);
                    return Ok(page);
                }
            };
            let offset = (ordinal % HISTORY_INDEX_PAGE_SIZE as u64) as usize;
            let take = (limit as usize - records.len()).min(offset + 1);
            records.extend(page.records[..=offset].iter().rev().take(take).cloned());
            before = before.saturating_sub(take as u64);
        }
        let mut page = ImportHistoryPage {
            next_cursor: (before > 0)
                .then(|| {
                    serde_json::to_string(&HistoryListCursor {
                        version: 1,
                        revision: manifest.revision,
                        before,
                    })
                })
                .transpose()
                .map_err(|_| history_error("History cursor could not be created."))?,
            warnings: manifest.warnings,
            ..ImportHistoryPage::default()
        };
        for record in records {
            match record {
                HistoryIndexRecord::V2 { entry } => page.entries.push(entry),
                HistoryIndexRecord::Legacy { entry } => page.legacy_read_only.push(entry),
            }
        }
        Ok(page)
    }

    pub fn detail_page(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        batch_id: &str,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<ImportHistoryDetailPage, BackendError> {
        validate_id(batch_id)?;
        if limit == 0 || limit as usize > HISTORY_DETAIL_PAGE_SIZE {
            return Err(history_error(
                "History detail limit must be between 1 and 50.",
            ));
        }
        let root = working_root(context, batch_id)?;
        let manifest: HistoryWorkingManifest =
            files.read_json(context, &format!("{root}/manifest.json"))?;
        validate_working_manifest(&manifest, batch_id)?;
        let start = if let Some(value) = cursor {
            let cursor: HistoryDetailCursor = serde_json::from_str(value)
                .map_err(|_| history_error("History detail cursor is invalid."))?;
            if cursor.version != 1 || cursor.batch_id != batch_id {
                return Err(history_error("History detail cursor is invalid."));
            }
            if cursor.revision != manifest.revision {
                return Err(BackendError::new(
                    "IMPORT_V2_HISTORY_DETAIL_CURSOR_STALE",
                    "Import history detail changed while this page was being read.",
                    true,
                    false,
                ));
            }
            cursor.after
        } else {
            0
        };
        if start > manifest.entry.item_count {
            return Err(history_error("History detail cursor is invalid."));
        }
        if start == manifest.entry.item_count {
            return Ok(ImportHistoryDetailPage {
                entry: manifest.entry,
                items: Vec::new(),
                next_cursor: None,
                total: start,
            });
        }
        let page_index = start / HISTORY_DETAIL_PAGE_SIZE as u64;
        let order: HistoryDetailOrderPage =
            files.read_json(context, &format!("{root}/order/page-{page_index:06}.json"))?;
        if order.schema_version != HISTORY_SCHEMA_VERSION
            || order.batch_id != batch_id
            || order.page_index != page_index
            || order.item_ids.len() > HISTORY_DETAIL_PAGE_SIZE
        {
            return Err(history_error("History detail index is corrupt."));
        }
        let within = (start % HISTORY_DETAIL_PAGE_SIZE as u64) as usize;
        let item_ids = order
            .item_ids
            .iter()
            .skip(within)
            .take(limit as usize)
            .collect::<Vec<_>>();
        let mut items = Vec::with_capacity(item_ids.len());
        for item_id in item_ids {
            validate_id(item_id)?;
            let snapshot: HistoryItemSnapshot =
                files.read_json(context, &format!("{root}/snapshots/{item_id}.json"))?;
            if snapshot.schema_version != HISTORY_SCHEMA_VERSION
                || snapshot.batch_id != batch_id
                || snapshot.item.item_id != *item_id
            {
                return Err(history_error("Historical item snapshot is corrupt."));
            }
            items.push(snapshot.item);
        }
        let next = start.saturating_add(items.len() as u64);
        Ok(ImportHistoryDetailPage {
            entry: manifest.entry.clone(),
            items,
            next_cursor: (next < manifest.entry.item_count)
                .then(|| {
                    serde_json::to_string(&HistoryDetailCursor {
                        version: 1,
                        batch_id: batch_id.to_string(),
                        revision: manifest.revision,
                        after: next,
                    })
                })
                .transpose()
                .map_err(|_| history_error("History detail cursor could not be created."))?,
            total: manifest.entry.item_count,
        })
    }

    pub(crate) fn load_item_session_snapshot(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        batch_id: &str,
        item_id: &str,
    ) -> Result<ImportSession, BackendError> {
        validate_id(batch_id)?;
        validate_id(item_id)?;
        validate_id(session_id)?;
        let manifest: HistoryWorkingManifest = files.read_json(
            context,
            &format!("{}/manifest.json", working_root(context, batch_id)?),
        )?;
        validate_working_manifest(&manifest, batch_id)?;
        if manifest.entry.session_id.as_deref() != Some(session_id) {
            return Err(BackendError::new(
                "IMPORT_V2_HISTORY_SCOPE_MISMATCH",
                "The historical import record does not belong to this session.",
                false,
                true,
            ));
        }
        let snapshot: HistoryItemSnapshot = files.read_json(
            context,
            &format!(
                "{}/snapshots/{item_id}.json",
                working_root(context, batch_id)?
            ),
        )?;
        if snapshot.schema_version != HISTORY_SCHEMA_VERSION
            || snapshot.batch_id != batch_id
            || snapshot.item.item_id != item_id
        {
            return Err(history_error("Historical item snapshot is corrupt."));
        }
        let mut session =
            ImportSession::new(session_id, &context.project_id, manifest.resource_mode);
        session.items.push(snapshot.item);
        Ok(session)
    }

    pub fn rebuild_index(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        is_cancelled: impl FnMut() -> bool,
    ) -> Result<(), BackendError> {
        self.rebuild_index_with_progress(context, files, is_cancelled, |_| {})
    }

    pub fn rebuild_index_with_progress(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        mut is_cancelled: impl FnMut() -> bool,
        mut on_progress: impl FnMut(u64),
    ) -> Result<(), BackendError> {
        let history_relative = history_root(context)?;
        let history_root = context.resolve_project_path(&history_relative)?;
        let mut records = HashMap::<String, (u64, String, HistoryIndexRecord)>::new();
        let mut claimed_paths = HashSet::new();
        let mut warnings = Vec::new();
        let mut progress = 0_u64;

        let working = history_root.join("working");
        if working.is_dir() {
            let mut manifests = fs::read_dir(&working)
                .map_err(|error| history_error(error.to_string()))?
                .flatten()
                .map(|entry| entry.path().join("manifest.json"))
                .filter(|path| path.is_file())
                .collect::<Vec<_>>();
            manifests.sort();
            for path in manifests {
                check_rebuild_cancelled(&mut is_cancelled)?;
                progress = progress.saturating_add(1);
                on_progress(progress);
                let relative = relative_path(&context.root, &path);
                #[cfg(feature = "performance-observers")]
                let read_result = files.read_project_bytes_absolute(context, &path);
                #[cfg(not(feature = "performance-observers"))]
                let read_result = fs::read(&path).map_err(|error| history_error(error.to_string()));
                let manifest: HistoryWorkingManifest = match read_result
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<HistoryWorkingManifest>(&bytes).ok())
                {
                    Some(manifest) => manifest,
                    None => {
                        warnings.push(corrupt_warning(&relative));
                        continue;
                    }
                };
                if validate_working_manifest(&manifest, &manifest.entry.id).is_err() {
                    warnings.push(corrupt_warning(&relative));
                    continue;
                }
                let entry = manifest.entry;
                records.insert(
                    entry.id.clone(),
                    (
                        timestamp_millis(entry.started_at.as_deref()).unwrap_or_default(),
                        format!("0:{}", entry.id),
                        HistoryIndexRecord::V2 { entry },
                    ),
                );
            }
        }
        if history_root.is_dir() {
            let mut paths = fs::read_dir(&history_root)
                .map_err(|error| history_error(error.to_string()))?
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
                .collect::<Vec<_>>();
            paths.sort();
            for path in paths {
                check_rebuild_cancelled(&mut is_cancelled)?;
                progress = progress.saturating_add(1);
                on_progress(progress);
                let relative = relative_path(&context.root, &path);
                #[cfg(feature = "performance-observers")]
                let read_result = files.read_project_bytes_absolute(context, &path);
                #[cfg(not(feature = "performance-observers"))]
                let read_result = fs::read(&path).map_err(|error| history_error(error.to_string()));
                let bytes = match read_result {
                    Ok(bytes) => bytes,
                    Err(_) => continue,
                };
                let value: serde_json::Value = match serde_json::from_slice(&bytes) {
                    Ok(value) => value,
                    Err(_) => {
                        warnings.push(corrupt_warning(&relative));
                        continue;
                    }
                };
                if value.get("sessionId").is_none() || value.get("items").is_none() {
                    continue;
                }
                claimed_paths.insert(relative.clone());
                let batch: ImportBatchResult = match serde_json::from_value(value) {
                    Ok(batch) => batch,
                    Err(_) => {
                        warnings.push(corrupt_warning(&relative));
                        continue;
                    }
                };
                if batch.history_snapshot.is_some() {
                    self.write_working_batch_with_control(
                        context,
                        files,
                        &batch,
                        &mut is_cancelled,
                        &mut || {
                            progress = progress.saturating_add(1);
                            on_progress(progress);
                        },
                    )?;
                }
                let entry = entry_from_batch(&batch);
                records.insert(
                    entry.id.clone(),
                    (
                        timestamp_millis(entry.started_at.as_deref()).unwrap_or_default(),
                        format!("0:{}", entry.id),
                        HistoryIndexRecord::V2 { entry },
                    ),
                );
            }
        }
        let legacy = LegacyHistoryAdapter::default().list_with_control(
            context,
            &mut is_cancelled,
            &mut || {
                progress = progress.saturating_add(1);
                on_progress(progress);
            },
        )?;
        warnings.extend(legacy.warnings);
        for entry in legacy.entries {
            if claimed_paths.contains(&entry.evidence_path) {
                continue;
            }
            records.insert(
                format!("legacy:{}", entry.evidence_path),
                (
                    timestamp_millis(entry.started_at.as_deref().or(entry.updated_at.as_deref()))
                        .unwrap_or_default(),
                    format!("1:{}", entry.id),
                    HistoryIndexRecord::Legacy { entry },
                ),
            );
        }
        let mut records = records.into_values().collect::<Vec<_>>();
        records.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
        let pages = records
            .into_iter()
            .map(|(_, _, record)| record)
            .collect::<Vec<_>>();
        let generation = format!("rebuild-{}", uuid::Uuid::new_v4().simple());
        let generation_root = format!("{history_relative}/index/generations/{generation}");
        files.ensure_dir(context, &generation_root)?;
        files.ensure_dir(context, &format!("{generation_root}/locations"))?;
        for (page_index, records) in pages.chunks(HISTORY_INDEX_PAGE_SIZE).enumerate() {
            check_rebuild_cancelled(&mut is_cancelled)?;
            progress = progress.saturating_add(1);
            on_progress(progress);
            files.write_json_atomic(
                context,
                &format!("{generation_root}/page-{page_index:06}.json"),
                &HistoryIndexPage {
                    schema_version: HISTORY_SCHEMA_VERSION,
                    generation: generation.clone(),
                    page_index: page_index as u64,
                    records: records.to_vec(),
                },
            )?;
            for record in records {
                if let HistoryIndexRecord::V2 { entry } = record {
                    check_rebuild_cancelled(&mut is_cancelled)?;
                    files.write_json_atomic(
                        context,
                        &format!("{generation_root}/locations/{}.json", entry.id),
                        &HistoryIndexLocation {
                            schema_version: HISTORY_SCHEMA_VERSION,
                            generation: generation.clone(),
                            page_index: page_index as u64,
                        },
                    )?;
                    progress = progress.saturating_add(1);
                    on_progress(progress);
                }
            }
        }
        check_rebuild_cancelled(&mut is_cancelled)?;
        let entry_count = pages.len() as u64;
        files.write_json_atomic(
            context,
            &index_manifest_path(context)?,
            &HistoryIndexManifest {
                schema_version: HISTORY_SCHEMA_VERSION,
                revision: chrono::Utc::now().timestamp_millis().max(1) as u64,
                generation,
                entry_count,
                page_count: page_count(entry_count as usize),
                warnings: warnings.into_iter().take(HISTORY_WARNING_LIMIT).collect(),
            },
        )
    }

    fn write_working_batch(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        batch: &ImportBatchResult,
    ) -> Result<(), BackendError> {
        self.write_working_batch_with_control(context, files, batch, &mut || false, &mut || {})
    }

    fn write_working_batch_with_control(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        batch: &ImportBatchResult,
        is_cancelled: &mut impl FnMut() -> bool,
        on_progress: &mut impl FnMut(),
    ) -> Result<(), BackendError> {
        validate_id(&batch.batch_id)?;
        let snapshot = batch
            .history_snapshot
            .as_ref()
            .ok_or_else(|| history_error("The historical session snapshot is missing."))?;
        let root = working_root(context, &batch.batch_id)?;
        files.ensure_dir(context, &format!("{root}/results"))?;
        files.ensure_dir(context, &format!("{root}/snapshots"))?;
        files.ensure_dir(context, &format!("{root}/order"))?;
        for (page_index, items) in snapshot.items.chunks(HISTORY_DETAIL_PAGE_SIZE).enumerate() {
            let mut item_ids = Vec::with_capacity(items.len());
            for item in items {
                check_rebuild_cancelled(is_cancelled)?;
                validate_id(&item.item_id)?;
                item_ids.push(item.item_id.clone());
                files.write_json_atomic(
                    context,
                    &format!("{root}/snapshots/{}.json", item.item_id),
                    &HistoryItemSnapshot {
                        schema_version: HISTORY_SCHEMA_VERSION,
                        batch_id: batch.batch_id.clone(),
                        item: item.clone(),
                    },
                )?;
                on_progress();
            }
            check_rebuild_cancelled(is_cancelled)?;
            files.write_json_atomic(
                context,
                &format!("{root}/order/page-{page_index:06}.json"),
                &HistoryDetailOrderPage {
                    schema_version: HISTORY_SCHEMA_VERSION,
                    batch_id: batch.batch_id.clone(),
                    page_index: page_index as u64,
                    item_ids,
                },
            )?;
            on_progress();
        }
        check_rebuild_cancelled(is_cancelled)?;
        files.write_json_atomic(
            context,
            &format!("{root}/manifest.json"),
            &HistoryWorkingManifest {
                schema_version: HISTORY_SCHEMA_VERSION,
                revision: (batch.items.len() as u64).saturating_add(1),
                entry: entry_from_batch(batch),
                detail_page_count: detail_page_count(snapshot.items.len()),
                resource_mode: snapshot.resource_mode.clone(),
            },
        )?;
        on_progress();
        Ok(())
    }

    fn upsert_index_entry(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        entry: ImportHistoryEntry,
    ) -> Result<(), BackendError> {
        let history_root = history_root(context)?;
        files.ensure_dir(context, &format!("{history_root}/index"))?;
        let manifest_path = index_manifest_path(context)?;
        let mut manifest = if files.exists(context, &manifest_path) {
            let manifest: HistoryIndexManifest = files.read_json(context, &manifest_path)?;
            validate_index_manifest(&manifest)?;
            manifest
        } else {
            HistoryIndexManifest {
                schema_version: HISTORY_SCHEMA_VERSION,
                revision: 0,
                generation: live_generation(),
                entry_count: 0,
                page_count: 0,
                warnings: Vec::new(),
            }
        };
        files.ensure_dir(
            context,
            &format!("{history_root}/index/generations/{}", manifest.generation),
        )?;
        validate_id(&entry.id)?;
        let entry_id = entry.id.clone();
        let location_path = index_location_path(context, &manifest.generation, &entry_id)?;
        let location = if files.exists(context, &location_path) {
            files
                .read_json::<HistoryIndexLocation>(context, &location_path)
                .ok()
                .filter(|location| {
                    location.schema_version == HISTORY_SCHEMA_VERSION
                        && location.generation == manifest.generation
                        && location.page_index < manifest.page_count
                })
        } else {
            None
        };
        let page_index = location.as_ref().map_or_else(
            || manifest.entry_count.saturating_sub(1) / HISTORY_INDEX_PAGE_SIZE as u64,
            |location| location.page_index,
        );
        let page_path = index_page_path(context, &manifest.generation, page_index)?;
        let mut page = if manifest.entry_count > 0 {
            let page: HistoryIndexPage = files.read_json(context, &page_path)?;
            validate_index_page(&page, page_index, &manifest.generation)?;
            page
        } else {
            HistoryIndexPage {
                schema_version: HISTORY_SCHEMA_VERSION,
                generation: manifest.generation.clone(),
                page_index,
                records: Vec::new(),
            }
        };
        if let Some(existing) = page.records.iter_mut().find(|record| {
            matches!(record, HistoryIndexRecord::V2 { entry: current } if current.id == entry.id)
        }) {
            *existing = HistoryIndexRecord::V2 { entry };
        } else {
            if location.is_some() {
                return Err(BackendError::new(
                    "IMPORT_V2_HISTORY_INDEX_REBUILD_REQUIRED",
                    "Import history index location is corrupt and must be rebuilt.",
                    true,
                    false,
                ));
            }
            if page.records.len() == HISTORY_INDEX_PAGE_SIZE {
                page.page_index = page.page_index.saturating_add(1);
                page.records.clear();
            }
            page.records.push(HistoryIndexRecord::V2 { entry });
            manifest.entry_count = manifest.entry_count.saturating_add(1);
        }
        manifest.revision = manifest.revision.saturating_add(1);
        manifest.page_count = page_count(manifest.entry_count as usize);
        files.write_json_atomic(
            context,
            &index_page_path(context, &manifest.generation, page.page_index)?,
            &page,
        )?;
        files.ensure_dir(
            context,
            &format!(
                "{history_root}/index/generations/{}/locations",
                manifest.generation
            ),
        )?;
        files.write_json_atomic(
            context,
            &index_location_path(context, &manifest.generation, &entry_id)?,
            &HistoryIndexLocation {
                schema_version: HISTORY_SCHEMA_VERSION,
                generation: manifest.generation.clone(),
                page_index: page.page_index,
            },
        )?;
        files.write_json_atomic(context, &manifest_path, &manifest)
    }
}

impl ImportV2Service {
    pub fn rebuild_history_index(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        is_cancelled: impl FnMut() -> bool,
        mut on_progress: impl FnMut(u64),
    ) -> Result<(), BackendError> {
        let project_locks = self.project_locks(context)?;
        let mut progress_updates = Vec::new();
        let result = {
            let _guard = self.lock_project(&project_locks);
            HistoryStore::default().rebuild_index_with_progress(
                context,
                files,
                is_cancelled,
                |current| progress_updates.push(current),
            )
        };
        for current in progress_updates {
            on_progress(current);
        }
        result
    }
}

fn entry_from_batch(batch: &ImportBatchResult) -> ImportHistoryEntry {
    let snapshot = batch.history_snapshot.as_ref();
    let sample_labels = snapshot
        .into_iter()
        .flat_map(|session| session.items.iter())
        .map(|item| item.input.display_name.clone())
        .take(2)
        .collect::<Vec<_>>();
    let item_count = snapshot.map_or(batch.items.len(), |session| session.items.len()) as u64;
    let status = history_status(batch).to_string();
    let mut available_actions = Vec::new();
    if item_count > 0 && snapshot.is_some() {
        available_actions.push(ImportHistoryAction::OpenDetail);
    }
    if snapshot.is_some_and(|session| session.items.iter().any(|item| item.preview.is_some())) {
        available_actions.push(ImportHistoryAction::OpenResult);
    }
    if batch.batch_task_id.is_some()
        || snapshot.is_some_and(|session| session.items.iter().any(|item| item.task_id.is_some()))
    {
        available_actions.push(ImportHistoryAction::ViewLogs);
    }
    if batch.completion.as_ref().is_some_and(|completion| {
        !completion.new_sources.is_empty() || !completion.updated_sources.is_empty()
    }) {
        available_actions.push(ImportHistoryAction::UpdateWiki);
    }
    let title = match sample_labels.as_slice() {
        [name] => format!("Import: {name}"),
        [first, second] => format!("Import: {first}, {second}"),
        _ => format!(
            "Import batch {}",
            batch.batch_id.chars().take(8).collect::<String>()
        ),
    };
    let updated_at = snapshot.map(|session| session.updated_at.clone());
    ImportHistoryEntry {
        id: batch.batch_id.clone(),
        title,
        status: status.clone(),
        session_id: Some(batch.session_id.clone()),
        batch_id: Some(batch.batch_id.clone()),
        task_id: batch.batch_task_id.clone(),
        started_at: (!batch.created_at.is_empty()).then(|| batch.created_at.clone()),
        updated_at: updated_at.clone(),
        completed_at: (status != "processing").then_some(updated_at).flatten(),
        legacy_read_only: false,
        item_count,
        committed_count: batch.committed_count as u64,
        failed_count: batch.failed_count as u64,
        sample_labels,
        available_actions,
        snapshot_available: snapshot.is_some(),
    }
}

fn entry_from_batch_progress(
    batch: &ImportBatchResult,
    result: &ImportItemCommitResult,
    processed_count: usize,
    committed_count: u32,
    failed_count: u32,
) -> ImportHistoryEntry {
    let snapshot = batch.history_snapshot.as_ref();
    let sample_labels = snapshot
        .into_iter()
        .flat_map(|session| session.items.iter().take(2))
        .map(|item| item.input.display_name.clone())
        .collect::<Vec<_>>();
    let item_count = snapshot.map_or(processed_count, |session| session.items.len()) as u64;
    let recorded_cancelled = batch
        .items
        .iter()
        .take(processed_count)
        .filter(|item| item.error_code.as_deref() == Some(crate::errors::IMPORT_V2_CANCELLED))
        .count();
    let cancelled_count = recorded_cancelled
        + usize::from(
            batch.items.len() < processed_count
                && result.error_code.as_deref() == Some(crate::errors::IMPORT_V2_CANCELLED),
        );
    let status = if processed_count < item_count as usize {
        "processing"
    } else if failed_count == 0 && committed_count == processed_count as u32 {
        "completed"
    } else if committed_count > 0 {
        "partially_committed"
    } else if cancelled_count == processed_count {
        "cancelled"
    } else {
        "failed"
    };
    let mut available_actions = Vec::new();
    if item_count > 0 && snapshot.is_some() {
        available_actions.push(ImportHistoryAction::OpenDetail);
        available_actions.push(ImportHistoryAction::OpenResult);
    }
    if batch.batch_task_id.is_some() {
        available_actions.push(ImportHistoryAction::ViewLogs);
    }
    let title = match sample_labels.as_slice() {
        [name] => format!("Import: {name}"),
        [first, second] => format!("Import: {first}, {second}"),
        _ => format!(
            "Import batch {}",
            batch.batch_id.chars().take(8).collect::<String>()
        ),
    };
    let updated_at = snapshot.map(|session| session.updated_at.clone());
    ImportHistoryEntry {
        id: batch.batch_id.clone(),
        title,
        status: status.into(),
        session_id: Some(batch.session_id.clone()),
        batch_id: Some(batch.batch_id.clone()),
        task_id: batch.batch_task_id.clone(),
        started_at: (!batch.created_at.is_empty()).then(|| batch.created_at.clone()),
        updated_at: updated_at.clone(),
        completed_at: (status != "processing").then_some(updated_at).flatten(),
        legacy_read_only: false,
        item_count,
        committed_count: u64::from(committed_count),
        failed_count: u64::from(failed_count),
        sample_labels,
        available_actions,
        snapshot_available: snapshot.is_some(),
    }
}

fn staged_result_payloads(
    batch: &ImportBatchResult,
    result: &ImportItemCommitResult,
    item: &ImportItem,
    sequence: usize,
    committed_count: u32,
    failed_count: u32,
) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>), BackendError> {
    let manifest = HistoryWorkingManifest {
        schema_version: HISTORY_SCHEMA_VERSION,
        revision: (sequence as u64).saturating_add(2),
        entry: entry_from_batch_progress(
            batch,
            result,
            sequence.saturating_add(1),
            committed_count,
            failed_count,
        ),
        detail_page_count: detail_page_count(
            batch
                .history_snapshot
                .as_ref()
                .map_or(0, |snapshot| snapshot.items.len()),
        ),
        resource_mode: batch
            .history_snapshot
            .as_ref()
            .map(|snapshot| snapshot.resource_mode.clone())
            .unwrap_or(ImportResourceMode::Balanced),
    };
    let receipt = HistoryItemReceipt {
        schema_version: HISTORY_SCHEMA_VERSION,
        batch_id: batch.batch_id.clone(),
        sequence: sequence as u64,
        result: result.clone(),
        recorded_at: chrono::Utc::now().to_rfc3339(),
    };
    let snapshot = HistoryItemSnapshot {
        schema_version: HISTORY_SCHEMA_VERSION,
        batch_id: batch.batch_id.clone(),
        item: item.clone(),
    };
    Ok((
        json_bytes(&receipt)?,
        json_bytes(&snapshot)?,
        json_bytes(&manifest)?,
    ))
}

fn history_status(batch: &ImportBatchResult) -> &'static str {
    let expected = batch
        .history_snapshot
        .as_ref()
        .map_or(batch.items.len(), |snapshot| snapshot.items.len());
    if batch.items.len() < expected || batch.items.is_empty() {
        return "processing";
    }
    if batch.failed_count == 0 && batch.committed_count == batch.items.len() as u32 {
        return "completed";
    }
    if batch.committed_count > 0 {
        return "partially_committed";
    }
    if batch
        .items
        .iter()
        .all(|item| item.error_code.as_deref() == Some(crate::errors::IMPORT_V2_CANCELLED))
    {
        return "cancelled";
    }
    "failed"
}

fn validate_index_manifest(manifest: &HistoryIndexManifest) -> Result<(), BackendError> {
    if manifest.schema_version != HISTORY_SCHEMA_VERSION
        || manifest.generation.is_empty()
        || manifest.generation.len() > 128
        || !manifest
            .generation
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        || manifest.page_count != page_count(manifest.entry_count as usize)
        || manifest.warnings.len() > HISTORY_WARNING_LIMIT
    {
        return Err(BackendError::new(
            "IMPORT_V2_HISTORY_INDEX_REBUILD_REQUIRED",
            "Import history index is corrupt and must be rebuilt.",
            true,
            false,
        ));
    }
    Ok(())
}

fn validate_index_page(
    page: &HistoryIndexPage,
    expected: u64,
    generation: &str,
) -> Result<(), BackendError> {
    if page.schema_version != HISTORY_SCHEMA_VERSION
        || page.generation != generation
        || page.page_index != expected
        || page.records.is_empty()
        || page.records.len() > HISTORY_INDEX_PAGE_SIZE
    {
        return Err(BackendError::new(
            "IMPORT_V2_HISTORY_INDEX_REBUILD_REQUIRED",
            "Import history index is corrupt and must be rebuilt.",
            true,
            false,
        ));
    }
    Ok(())
}

fn validate_working_manifest(
    manifest: &HistoryWorkingManifest,
    batch_id: &str,
) -> Result<(), BackendError> {
    if manifest.schema_version != HISTORY_SCHEMA_VERSION
        || manifest.entry.id != batch_id
        || manifest.entry.item_count > 0 && manifest.detail_page_count == 0
    {
        return Err(history_error("Import history detail is corrupt."));
    }
    Ok(())
}

fn history_root(context: &ProjectContext) -> Result<String, BackendError> {
    Ok(context.layout.import_paths()?.history_root())
}

fn working_root(context: &ProjectContext, batch_id: &str) -> Result<String, BackendError> {
    Ok(format!("{}/working/{batch_id}", history_root(context)?))
}

fn index_manifest_path(context: &ProjectContext) -> Result<String, BackendError> {
    Ok(format!("{}/index/manifest.json", history_root(context)?))
}

fn index_page_path(
    context: &ProjectContext,
    generation: &str,
    page_index: u64,
) -> Result<String, BackendError> {
    Ok(format!(
        "{}/index/generations/{generation}/page-{page_index:06}.json",
        history_root(context)?
    ))
}

fn index_location_path(
    context: &ProjectContext,
    generation: &str,
    batch_id: &str,
) -> Result<String, BackendError> {
    Ok(format!(
        "{}/index/generations/{generation}/locations/{batch_id}.json",
        history_root(context)?
    ))
}

fn live_generation() -> String {
    "live".into()
}

fn page_count(items: usize) -> u64 {
    if items == 0 {
        0
    } else {
        items.div_ceil(HISTORY_INDEX_PAGE_SIZE) as u64
    }
}

fn detail_page_count(items: usize) -> u64 {
    if items == 0 {
        0
    } else {
        items.div_ceil(HISTORY_DETAIL_PAGE_SIZE) as u64
    }
}

fn history_data_exists(context: &ProjectContext) -> Result<bool, BackendError> {
    let history = context.resolve_project_path(&history_root(context)?)?;
    if history.is_dir()
        && fs::read_dir(&history)
            .map_err(|error| history_error(error.to_string()))?
            .flatten()
            .any(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("json"))
    {
        return Ok(true);
    }
    let task_root = context
        .layout
        .task_state_root
        .as_deref()
        .unwrap_or(".app/compat/tasks");
    Ok(context.resolve_project_path(task_root)?.is_dir())
}

fn rebuild_warning(context: &ProjectContext) -> Result<LegacyHistoryWarning, BackendError> {
    Ok(LegacyHistoryWarning {
        code: "IMPORT_V2_HISTORY_INDEX_REBUILD_REQUIRED".into(),
        message: "Import history is being prepared for paged reading.".into(),
        evidence_path: index_manifest_path(context)?,
    })
}

fn fallback_history_page(context: &ProjectContext) -> Result<ImportHistoryPage, BackendError> {
    let legacy =
        LegacyHistoryAdapter::new(crate::services::import_v2::migration::LegacyHistoryLimits {
            max_files: HISTORY_INDEX_PAGE_SIZE,
            max_bytes: 1024 * 1024,
        })
        .list(context)?;
    Ok(ImportHistoryPage {
        entries: Vec::new(),
        legacy_read_only: legacy.entries,
        warnings: legacy.warnings,
        next_cursor: None,
    })
}

fn check_rebuild_cancelled(is_cancelled: &mut impl FnMut() -> bool) -> Result<(), BackendError> {
    if is_cancelled() {
        return Err(BackendError::new(
            crate::errors::IMPORT_V2_CANCELLED,
            "Import history index rebuild was cancelled.",
            true,
            false,
        ));
    }
    Ok(())
}

fn corrupt_warning(relative: &str) -> LegacyHistoryWarning {
    LegacyHistoryWarning {
        code: "IMPORT_V2_HISTORY_CORRUPT".into(),
        message: "A canonical import history record could not be read; its bytes were preserved."
            .into(),
        evidence_path: relative.into(),
    }
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn timestamp_millis(value: Option<&str>) -> Option<u64> {
    value
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .and_then(|time| time.timestamp_millis().try_into().ok())
}

fn validate_id(value: &str) -> Result<(), BackendError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(history_error("The import history identity is invalid."));
    }
    Ok(())
}

fn json_bytes(value: &impl Serialize) -> Result<Vec<u8>, BackendError> {
    serde_json::to_vec_pretty(value)
        .map_err(|_| history_error("Import history metadata could not be serialized."))
}

fn history_error(message: impl Into<String>) -> BackendError {
    BackendError::new("IMPORT_V2_HISTORY_INVALID", message, true, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    use crate::models::import_v2::{ImportInput, ImportInputKind};

    fn queued_item(index: usize) -> ImportItem {
        ImportItem::queued(
            &format!("item-{index}"),
            ImportInput {
                kind: ImportInputKind::File,
                display_name: format!("资料-{index}.md"),
                locator: format!("fixture/{index}.md"),
                normalized_locator: None,
                source_identity: None,
                media_save_mode: Default::default(),
            },
        )
    }

    #[test]
    fn summary_never_serializes_unbounded_item_ids() {
        let batch = ImportBatchResult {
            batch_id: "batch-1".into(),
            session_id: "session-1".into(),
            created_at: "2026-08-27T00:00:00Z".into(),
            batch_task_id: None,
            committed_count: 0,
            failed_count: 0,
            items: Vec::new(),
            history_snapshot: None,
            completion: None,
        };
        let wire = serde_json::to_value(entry_from_batch(&batch)).unwrap();
        assert!(wire.get("itemIds").is_none());
        assert_eq!(wire["itemCount"], 0);
    }

    #[test]
    fn rebuild_preserves_corrupt_canonical_history_bytes_and_reports_warning() {
        let root = tempfile::tempdir().unwrap();
        let context = ProjectContext::new("history-corrupt", root.path().to_path_buf());
        let files = FileStore::default();
        let history_root = root.path().join(".app/import-history");
        std::fs::create_dir_all(&history_root).unwrap();
        let canonical = history_root.join("broken.json");
        let original = b"{ definitely not valid history";
        std::fs::write(&canonical, original).unwrap();

        HistoryStore::default()
            .rebuild_index(&context, &files, || false)
            .unwrap();

        assert_eq!(std::fs::read(canonical).unwrap(), original);
        let page = HistoryStore::default()
            .list_page(&context, &files, None, 50)
            .unwrap();
        assert!(page.entries.is_empty());
        assert!(page.warnings.iter().any(|warning| {
            warning.code == "IMPORT_V2_HISTORY_CORRUPT"
                && warning.evidence_path == ".app/import-history/broken.json"
        }));
    }

    #[test]
    fn incomplete_working_batch_rebuilds_as_processing_without_a_monolith() {
        let root = tempfile::tempdir().unwrap();
        let context = ProjectContext::new("working-rebuild", root.path().to_path_buf());
        let files = FileStore::default();
        let history = HistoryStore::default();
        let mut snapshot = ImportSession::new(
            "session-working",
            "working-rebuild",
            ImportResourceMode::Balanced,
        );
        snapshot.items = vec![queued_item(0), queued_item(1)];
        let batch = ImportBatchResult {
            batch_id: "batch-working".into(),
            session_id: snapshot.session_id.clone(),
            created_at: "2026-08-28T00:00:00Z".into(),
            batch_task_id: None,
            committed_count: 1,
            failed_count: 0,
            items: vec![ImportItemCommitResult {
                item_id: "item-0".into(),
                source_id: None,
                version_id: None,
                wiki_path: None,
                content_hash: None,
                disposition: None,
                warnings: Vec::new(),
                committed: true,
                error_code: None,
            }],
            history_snapshot: Some(snapshot),
            completion: None,
        };
        history
            .write_working_batch(&context, &files, &batch)
            .unwrap();

        history.rebuild_index(&context, &files, || false).unwrap();

        let page = history.list_page(&context, &files, None, 50).unwrap();
        assert_eq!(page.entries.len(), 1);
        assert_eq!(page.entries[0].id, "batch-working");
        assert_eq!(page.entries[0].status, "processing");
    }

    #[test]
    fn terminal_cancelled_receipt_rebuilds_as_cancelled_before_finalization() {
        let root = tempfile::tempdir().unwrap();
        let context = ProjectContext::new("cancelled-working", root.path().to_path_buf());
        let files = FileStore::default();
        let history = HistoryStore::default();
        let mut snapshot = ImportSession::new(
            "session-cancelled",
            "cancelled-working",
            ImportResourceMode::Balanced,
        );
        snapshot.items = vec![queued_item(0)];
        let result = ImportItemCommitResult {
            item_id: "item-0".into(),
            source_id: None,
            version_id: None,
            wiki_path: None,
            content_hash: None,
            disposition: None,
            warnings: Vec::new(),
            committed: false,
            error_code: Some(crate::errors::IMPORT_V2_CANCELLED.into()),
        };
        let batch = ImportBatchResult {
            batch_id: "batch-cancelled".into(),
            session_id: snapshot.session_id.clone(),
            created_at: "2026-08-28T00:00:00Z".into(),
            batch_task_id: None,
            committed_count: 0,
            failed_count: 1,
            items: Vec::new(),
            history_snapshot: Some(snapshot.clone()),
            completion: None,
        };

        history.begin_batch(&context, &files, &batch).unwrap();
        history
            .persist_result(&context, &files, &batch, &result, &snapshot.items[0], 0)
            .unwrap();
        history.rebuild_index(&context, &files, || false).unwrap();

        let page = history.list_page(&context, &files, None, 50).unwrap();
        assert_eq!(page.entries[0].status, "cancelled");
        assert!(!root
            .path()
            .join(".app/import-history/batch-cancelled.json")
            .exists());
    }

    #[test]
    fn staged_history_receipt_bytes_scale_linearly_with_selected_items() {
        let mut totals = Vec::new();
        for item_count in [100_usize, 1_000, 10_000] {
            let mut snapshot = ImportSession::new(
                "session-linear",
                "history-linear",
                ImportResourceMode::Balanced,
            );
            snapshot.items = (0..item_count).map(queued_item).collect();
            let item = snapshot.items[0].clone();
            let result = ImportItemCommitResult {
                item_id: item.item_id.clone(),
                source_id: None,
                version_id: None,
                wiki_path: None,
                content_hash: None,
                disposition: None,
                warnings: Vec::new(),
                committed: false,
                error_code: Some(crate::errors::IMPORT_V2_CANCELLED.into()),
            };
            let batch = ImportBatchResult {
                batch_id: "batch-linear".into(),
                session_id: snapshot.session_id.clone(),
                created_at: "2026-08-28T00:00:00Z".into(),
                batch_task_id: None,
                committed_count: 0,
                failed_count: 1,
                items: Vec::new(),
                history_snapshot: Some(snapshot),
                completion: None,
            };
            let payloads = staged_result_payloads(&batch, &result, &item, 0, 0, 1).unwrap();
            let bytes_per_item = payloads.0.len() + payloads.1.len() + payloads.2.len();
            let total = bytes_per_item * item_count;
            println!(
                "BATCH8_HISTORY_BYTES D={item_count} bytes_per_item={bytes_per_item} total={total}"
            );
            totals.push((item_count, bytes_per_item, total));
        }
        let min_payload = totals.iter().map(|(_, bytes, _)| *bytes).min().unwrap();
        let max_payload = totals.iter().map(|(_, bytes, _)| *bytes).max().unwrap();
        assert!(max_payload <= min_payload.saturating_mul(2), "{totals:?}");
        assert!(totals.windows(2).all(|pair| {
            let expected_ratio = pair[1].0 / pair[0].0;
            pair[1].2 <= pair[0].2.saturating_mul(expected_ratio + 1)
        }));
    }

    #[test]
    fn cancelled_rebuild_never_repoints_the_published_manifest() {
        let root = tempfile::tempdir().unwrap();
        let context = ProjectContext::new("generation-cancel", root.path().to_path_buf());
        let files = FileStore::default();
        let history = HistoryStore::default();
        let canonical_root = root.path().join(".app/import-history");
        std::fs::create_dir_all(&canonical_root).unwrap();
        for index in 0..60 {
            let batch = ImportBatchResult {
                batch_id: format!("batch-{index:03}"),
                session_id: format!("session-{index:03}"),
                created_at: format!("2026-08-28T00:00:{:02}Z", index % 60),
                batch_task_id: None,
                committed_count: 0,
                failed_count: 0,
                items: Vec::new(),
                history_snapshot: None,
                completion: None,
            };
            std::fs::write(
                canonical_root.join(format!("batch-{index:03}.json")),
                serde_json::to_vec(&batch).unwrap(),
            )
            .unwrap();
        }
        history.rebuild_index(&context, &files, || false).unwrap();
        let manifest_path = canonical_root.join("index/manifest.json");
        let published = std::fs::read(&manifest_path).unwrap();

        let progress = Cell::new(0_u64);
        let result = history.rebuild_index_with_progress(
            &context,
            &files,
            || progress.get() >= 121,
            |current| progress.set(current),
        );

        assert_eq!(result.unwrap_err().code, crate::errors::IMPORT_V2_CANCELLED);
        assert_eq!(std::fs::read(manifest_path).unwrap(), published);
        assert_eq!(
            history
                .list_page(&context, &files, None, 50)
                .unwrap()
                .entries
                .len(),
            50
        );
    }

    #[test]
    fn corrupt_index_falls_back_to_a_bounded_read_only_page() {
        let root = tempfile::tempdir().unwrap();
        let context = ProjectContext::new("corrupt-index", root.path().to_path_buf());
        let files = FileStore::default();
        std::fs::create_dir_all(root.path().join(".app/import-history/index")).unwrap();
        std::fs::create_dir_all(root.path().join(".app/tasks")).unwrap();
        std::fs::write(
            root.path().join(".app/import-history/index/manifest.json"),
            b"{ broken derived index",
        )
        .unwrap();
        std::fs::write(
            root.path().join(".app/tasks/legacy.json"),
            br#"{"id":"legacy-one","title":"Legacy one","status":"completed"}"#,
        )
        .unwrap();

        let page = HistoryStore::default()
            .list_page(&context, &files, None, 50)
            .unwrap();
        assert_eq!(page.legacy_read_only.len(), 1);
        assert!(page
            .warnings
            .iter()
            .any(|warning| warning.code == "IMPORT_V2_HISTORY_INDEX_REBUILD_REQUIRED"));
    }

    #[test]
    fn compatible_history_uses_only_compat_roots_with_cjk_paths() {
        let root = tempfile::tempdir().unwrap();
        let mut context = ProjectContext::new("compatible-history", root.path().to_path_buf());
        context.layout.app_state_root = Some(".app/compat".into());
        context.layout.task_state_root = Some(".app/compat/tasks".into());
        std::fs::create_dir_all(root.path().join(".app/compat/tasks")).unwrap();
        std::fs::create_dir_all(root.path().join(".app/tasks")).unwrap();
        std::fs::write(
            root.path().join(".app/compat/tasks/旧记录.json"),
            r#"{"id":"compat-one","title":"旧记录","status":"completed"}"#.as_bytes(),
        )
        .unwrap();
        std::fs::write(
            root.path().join(".app/tasks/unrelated.json"),
            br#"{"id":"native-one","title":"Must not leak","status":"completed"}"#,
        )
        .unwrap();

        let files = FileStore::default();
        HistoryStore::default()
            .rebuild_index(&context, &files, || false)
            .unwrap();
        let page = HistoryStore::default()
            .list_page(&context, &files, None, 50)
            .unwrap();
        assert_eq!(page.legacy_read_only.len(), 1);
        assert_eq!(page.legacy_read_only[0].id, "compat-one");
        assert!(root
            .path()
            .join(".app/compat/import-history/index/manifest.json")
            .is_file());
        assert!(!page
            .legacy_read_only
            .iter()
            .any(|entry| entry.id == "native-one"));
    }
}
