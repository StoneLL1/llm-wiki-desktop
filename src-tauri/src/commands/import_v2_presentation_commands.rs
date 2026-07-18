use std::collections::HashSet;
use std::cmp::Ordering;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::import_v2::{
    ImportBatchResult, ImportItem, ImportItemStatus, ImportRecoveryAction, ImportSession,
};
use crate::models::import_v2_file::CapabilityRequirement;
use crate::models::import_v2_migration::{LegacyHistoryView, MigrationStatus};
use crate::models::import_v2_presentation::{
    GetImportCapabilityRequirementV2Request, GetImportFrontendReadinessV2Request,
    GetImportPreviewContentV2Request, ImportCapabilityRequirement, ImportFrontendReadiness,
    ImportHistoryAction, ImportHistoryEntry, ImportHistoryPage, ImportPlatformReadiness,
    ImportPreviewContent,
    InstallImportCapabilityV2Request, ListImportHistoryV2Request, IMPORT_V2_PREVIEW_MAX_BYTES,
};
use crate::models::paths::ProjectContext;
use crate::models::task::BackendTask;
use crate::services::import_v2::activation::ImportV2ActivationService;
use crate::services::import_v2::capability_runtime::CapabilityRuntimeStatus;
use crate::services::import_v2::migration::{
    LegacyHistoryAdapter, MigrationService, REQUIRED_IMPORT_V2_CONTRACT,
};

#[tauri::command]
pub fn get_import_preview_content_v2(
    state: State<'_, AppState>,
    request: GetImportPreviewContentV2Request,
) -> Result<ImportPreviewContent, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let session = if let Some(batch_id) = request.history_batch_id.as_deref() {
        if let Some(snapshot) = crate::commands::import_v2_commands::load_history_snapshot(
            &context,
            &request.session_id,
            batch_id,
        )? {
            snapshot
        } else {
            state
                .import_v2_service
                .load_session(&context, &state.file_store, &request.session_id)?
        }
    } else {
        state
            .import_v2_service
            .load_session(&context, &state.file_store, &request.session_id)?
    };
    let item = session
        .items
        .iter()
        .find(|item| item.item_id == request.item_id)
        .ok_or_else(|| {
            presentation_error("IMPORT_V2_ITEM_NOT_FOUND", "Import item was not found.")
        })?;

    let (relative_path, title, expected_hash) =
        if let Some(candidate_id) = request.candidate_id.as_deref() {
            let (candidate, _) =
                crate::services::import_v2::agent_candidate::AgentCandidateService::new(
                    &state.import_v2_service,
                    &state.file_store,
                    &state.task_service,
                )
                .load_candidate(
                    &context,
                    &request.session_id,
                    &request.item_id,
                    candidate_id,
                )?;
            (
                candidate.markdown.relative_path,
                format!("Agent candidate: {}", item.input.display_name),
                candidate.markdown.sha256,
            )
        } else {
            if !matches!(
                item.status,
                ImportItemStatus::PreviewReady
                    | ImportItemStatus::NeedsMerge
                    | ImportItemStatus::Committing
                    | ImportItemStatus::Completed
            ) {
                return Err(presentation_error(
                    "IMPORT_V2_PREVIEW_NOT_READY",
                    "Markdown preview is not available for this import item.",
                ));
            }
            let preview = item.preview.as_ref().ok_or_else(|| {
                presentation_error(
                    "IMPORT_V2_PREVIEW_NOT_FOUND",
                    "The import item has no Markdown preview.",
                )
            })?;
            (
                preview.markdown.relative_path.clone(),
                preview.title.clone(),
                preview.markdown.sha256.clone(),
            )
        };

    if let Some(batch_id) = request.history_batch_id.as_deref() {
        validate_identifier(batch_id)?;
        validate_identifier(&request.item_id)?;
        let history_relative = format!(
            ".app/import-history-previews/{batch_id}/{}.md",
            request.item_id
        );
        if let Ok(history_path) = safe_project_path(&context.root, &history_relative) {
            if history_path.is_file() && request.candidate_id.is_none() {
                return Ok(ImportPreviewContent {
                    session_id: request.session_id,
                    item_id: request.item_id,
                    candidate_id: request.candidate_id,
                    title,
                    markdown: read_history_markdown(
                        &history_path,
                        &expected_hash,
                    )?,
                    truncated: fs::metadata(&history_path)
                        .map(|metadata| metadata.len() > IMPORT_V2_PREVIEW_MAX_BYTES)
                        .unwrap_or(false),
                    total_bytes: fs::metadata(&history_path)
                        .map(|metadata| metadata.len())
                        .unwrap_or_default(),
                    sha256: expected_hash,
                });
            }
        }
    }

    let (markdown, truncated, total_bytes) = read_staging_markdown(
        &context,
        &request.session_id,
        &request.item_id,
        &relative_path,
        &expected_hash,
    )?;
    Ok(ImportPreviewContent {
        session_id: request.session_id,
        item_id: request.item_id,
        candidate_id: request.candidate_id,
        title,
        markdown,
        truncated,
        total_bytes,
        sha256: expected_hash,
    })
}

fn read_history_markdown(path: &Path, expected_hash: &str) -> Result<String, BackendError> {
    let bytes = fs::read(path).map_err(|_| {
        presentation_error(
            "IMPORT_V2_HISTORY_PREVIEW_READ_FAILED",
            "Historical Markdown preview could not be read.",
        )
    })?;
    let actual = sha256(&bytes);
    if !actual.eq_ignore_ascii_case(expected_hash) {
        return Err(presentation_error(
            "IMPORT_V2_HISTORY_PREVIEW_CHANGED",
            "Historical Markdown preview changed after import.",
        ));
    }
    let mut preview = bytes;
    preview.truncate(IMPORT_V2_PREVIEW_MAX_BYTES as usize);
    String::from_utf8(preview).map_err(|_| {
        presentation_error(
            "IMPORT_V2_PREVIEW_INVALID",
            "Markdown preview is not valid UTF-8.",
        )
    })
}

#[tauri::command]
pub fn get_import_frontend_readiness_v2(
    state: State<'_, AppState>,
    request: GetImportFrontendReadinessV2Request,
) -> Result<ImportFrontendReadiness, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let migration = MigrationService::default().status(&context)?;
    let legacy_history = LegacyHistoryAdapter::default().list(&context.root)?;
    let activation = ImportV2ActivationService::default().read(&context)?;
    let active = activation.as_ref().is_some_and(|record| {
        record.legacy_mutations_disabled && record.rollback_mode == "release_based"
    }) && migration.status == MigrationStatus::Applied;
    let platforms = platform_readiness(
        &state.import_v2_service.registered_engine_routes()?,
        &state.import_capability_runtime.statuses(),
    );
    Ok(ImportFrontendReadiness {
        backend_version: REQUIRED_IMPORT_V2_CONTRACT.into(),
        active,
        migration_status: migration.status,
        unfinished_session_id: state
            .import_v2_service
            .find_unfinished_session(&context, &state.file_store)?,
        legacy_history_available: !legacy_history.entries.is_empty(),
        platforms,
    })
}

fn platform_readiness(
    registered_routes: &[String],
    capability_statuses: &[CapabilityRuntimeStatus],
) -> Vec<ImportPlatformReadiness> {
    let route_status = |id: &str, routes: &[&str], phase_two: bool| {
        if phase_two {
            return ImportPlatformReadiness {
                id: id.into(),
                available: false,
                reason_code: Some("phase_two".into()),
            };
        }
        let available = routes.iter().any(|route| {
            registered_routes.iter().any(|registered| registered == route)
                || capability_statuses
                    .iter()
                    .any(|status| status.route == *route && status.available)
        });
        let reason_code = (!available).then(|| {
            if capability_statuses
                .iter()
                .any(|status| routes.iter().any(|route| status.route == *route))
            {
                "capability_missing".into()
            } else {
                "route_unavailable".into()
            }
        });
        ImportPlatformReadiness {
            id: id.into(),
            available,
            reason_code,
        }
    };
    let http = route_status("http", &["web.generic.readability"], false);
    let wechat = route_status("wechat", &["web.wechat.article"], false);
    let zhihu = route_status("zhihu", &["web.zhihu.content"], false);
    let bilibili = route_status(
        "bilibili",
        &["web.bilibili.metadata", "web.bilibili.video"],
        false,
    );
    let xiaohongshu = route_status("xiaohongshu", &[], true);
    let x = route_status("x", &[], true);
    vec![http, wechat, zhihu, bilibili, xiaohongshu, x]
}

#[tauri::command]
pub fn list_import_history_v2(
    state: State<'_, AppState>,
    request: ListImportHistoryV2Request,
) -> Result<ImportHistoryPage, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let (v2_entries, v2_paths, mut v2_warnings) = read_v2_history(&context, &state)?;
    let LegacyHistoryView { entries, warnings } =
        LegacyHistoryAdapter::default().list(&context.root)?;
    let mut records = v2_entries;
    records.extend(
        entries
        .into_iter()
        .filter(|entry| !v2_paths.contains(&entry.evidence_path))
        .map(|entry| {
            let path = context.resolve_project_path(&entry.evidence_path).ok();
            HistoryRecord::Legacy {
                modified_millis: path
                    .as_deref()
                    .map(file_modified_millis)
                    .unwrap_or_default(),
                entry,
            }
        }),
    );
    records.sort_by(history_record_cmp);
    let cursor = parse_history_cursor(request.cursor.as_deref())?;
    let snapshot_millis = cursor
        .as_ref()
        .map(|cursor| cursor.snapshot_millis)
        .unwrap_or_else(current_unix_millis);
    let after = cursor.and_then(|cursor| cursor.after);
    let limit = request.limit.unwrap_or(50).clamp(1, 50) as usize;
    let filtered_records = records
        .into_iter()
        .filter(|record| {
            record.modified_millis() <= snapshot_millis
                && after
                    .as_ref()
                    .map_or(true, |key| {
                        history_key_cmp(&record.key(), key) == Ordering::Greater
                    })
        })
        .collect::<Vec<_>>();
    let has_more = filtered_records.len() > limit;
    let page_records = filtered_records.into_iter().take(limit).collect::<Vec<_>>();
    let mut page_entries = Vec::new();
    let mut page_legacy = Vec::new();
    for record in &page_records {
        match record {
            HistoryRecord::V2 { entry, .. } => page_entries.push(entry.clone()),
            HistoryRecord::Legacy { entry, .. } => page_legacy.push(entry.clone()),
        }
    }
    let next_cursor = if has_more {
        page_records.last().map(|record| {
            serde_json::to_string(&HistoryCursor {
                version: HISTORY_CURSOR_VERSION,
                snapshot_millis,
                after: Some(record.key()),
            })
            .expect("history cursor is serializable")
        })
    } else {
        None
    };
    v2_warnings.extend(warnings);
    Ok(ImportHistoryPage {
        entries: page_entries,
        legacy_read_only: page_legacy,
        next_cursor,
        warnings: v2_warnings,
    })
}

const HISTORY_CURSOR_VERSION: u8 = 1;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryCursor {
    version: u8,
    snapshot_millis: u64,
    after: Option<HistoryCursorKey>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryCursorKey {
    modified_millis: u64,
    kind: u8,
    id: String,
}

#[derive(Debug, Clone)]
enum HistoryRecord {
    V2 {
        entry: ImportHistoryEntry,
        modified_millis: u64,
    },
    Legacy {
        entry: crate::models::import_v2_migration::LegacyHistoryEntry,
        modified_millis: u64,
    },
}

impl HistoryRecord {
    fn modified_millis(&self) -> u64 {
        match self {
            Self::V2 { modified_millis, .. } | Self::Legacy { modified_millis, .. } => {
                *modified_millis
            }
        }
    }

    fn key(&self) -> HistoryCursorKey {
        match self {
            Self::V2 {
                entry,
                modified_millis,
                ..
            } => HistoryCursorKey {
                modified_millis: *modified_millis,
                kind: 0,
                id: entry.id.clone(),
            },
            Self::Legacy {
                entry,
                modified_millis,
            } => HistoryCursorKey {
                modified_millis: *modified_millis,
                kind: 1,
                id: entry.id.clone(),
            },
        }
    }
}

fn read_v2_history(
    context: &ProjectContext,
    state: &AppState,
) -> Result<
    (
        Vec<HistoryRecord>,
        HashSet<String>,
        Vec<crate::models::import_v2_migration::LegacyHistoryWarning>,
    ),
    BackendError,
> {
    let history_dir = context.resolve_project_path(".app/import-history")?;
    let metadata = match fs::symlink_metadata(&history_dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Vec::new(), HashSet::new(), Vec::new()));
        }
        Err(error) => {
            return Err(presentation_error(
                "IMPORT_V2_HISTORY_READ_FAILED",
                format!("Import history could not be read: {error}"),
            ));
        }
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
        return Err(presentation_error(
            "IMPORT_V2_HISTORY_READ_FAILED",
            "Import history directory is invalid.",
        ));
    }

    let mut files = fs::read_dir(&history_dir)
        .map_err(|_| {
            presentation_error(
                "IMPORT_V2_HISTORY_READ_FAILED",
                "Import history could not be read.",
            )
        })?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().and_then(|value| value.to_str()) == Some("json")
                && fs::symlink_metadata(path)
                    .is_ok_and(|value| value.is_file() && !value.file_type().is_symlink())
        })
        .collect::<Vec<_>>();
    files.sort();

    let mut entries = Vec::new();
    let mut v2_paths = HashSet::new();
    let mut warnings = Vec::new();
    for path in files {
        let relative = path
            .strip_prefix(&context.root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let value = match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let looks_like_v2 = value.get("sessionId").is_some() && value.get("items").is_some();
        if !looks_like_v2 {
            continue;
        }
        // Claim every recognizable V2 record before deserialization so a
        // malformed V2 file is not reintroduced as a misleading legacy entry.
        v2_paths.insert(relative.clone());
        let batch: ImportBatchResult = match serde_json::from_value(value) {
            Ok(batch) => batch,
            Err(_) => {
                warnings.push(crate::models::import_v2_migration::LegacyHistoryWarning {
                    code: "IMPORT_V2_HISTORY_CORRUPT".into(),
                    message: "A V2 import history record could not be read.".into(),
                    evidence_path: relative,
                });
                continue;
            }
        };
        let session = batch.history_snapshot.clone().or_else(|| {
            state
                .import_v2_service
                .load_session(context, &state.file_store, &batch.session_id)
                .ok()
        });
        let item_ids = batch
            .items
            .iter()
            .map(|item| item.item_id.clone())
            .collect::<Vec<_>>();
        let committed_ids = batch
            .items
            .iter()
            .filter(|item| item.committed)
            .map(|item| item.item_id.as_str())
            .collect::<HashSet<_>>();
        let open_result = session.as_ref().is_some_and(|session| {
            session
                .items
                .iter()
                .any(|item| committed_ids.contains(item.item_id.as_str()) && item.preview.is_some())
        });
        let view_logs = batch
            .batch_task_id
            .as_deref()
            .is_some_and(|task_id| state.task_service.get_task(task_id).is_some())
            || session.as_ref().is_some_and(|session| {
                session.items.iter().any(|item| {
                    item_ids.iter().any(|id| id == &item.item_id)
                        && item
                            .task_id
                            .as_deref()
                            .is_some_and(|task_id| state.task_service.get_task(task_id).is_some())
                })
            });
        let mut available_actions = Vec::new();
        if session.is_some() {
            available_actions.push(ImportHistoryAction::OpenDetail);
        }
        if open_result {
            available_actions.push(ImportHistoryAction::OpenResult);
        }
        if view_logs {
            available_actions.push(ImportHistoryAction::ViewLogs);
        }
        let modified_millis = parse_timestamp_millis(&batch.created_at)
            .unwrap_or_else(|| file_modified_millis(&path));
        let updated_at = fs::metadata(&path)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .map(|time| chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339());
        let title = history_title(&batch, session.as_ref());
        let entry = ImportHistoryEntry {
            id: batch.batch_id.clone(),
            title,
            status: history_status(&batch).into(),
            session_id: Some(batch.session_id.clone()),
            batch_id: Some(batch.batch_id.clone()),
            task_id: batch.batch_task_id.clone(),
            started_at: (!batch.created_at.is_empty())
                .then_some(batch.created_at.clone())
                .or_else(|| updated_at.clone()),
            updated_at: updated_at.clone(),
            completed_at: (!matches!(history_status(&batch), "processing"))
                .then_some(updated_at)
                .flatten(),
            legacy_read_only: false,
            item_ids,
            available_actions,
            snapshot_available: batch.history_snapshot.is_some(),
        };
        entries.push(HistoryRecord::V2 {
            entry,
            modified_millis,
        });
    }
    entries.sort_by(history_record_cmp);
    Ok((entries, v2_paths, warnings))
}

fn history_record_cmp(left: &HistoryRecord, right: &HistoryRecord) -> Ordering {
    history_key_cmp(&left.key(), &right.key())
}

fn history_key_cmp(left: &HistoryCursorKey, right: &HistoryCursorKey) -> Ordering {
    right
        .modified_millis
        .cmp(&left.modified_millis)
        .then_with(|| left.kind.cmp(&right.kind))
        .then_with(|| right.id.cmp(&left.id))
}

fn parse_history_cursor(value: Option<&str>) -> Result<Option<HistoryCursor>, BackendError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let cursor = serde_json::from_str::<HistoryCursor>(value).map_err(|_| {
        presentation_error(
            "IMPORT_V2_HISTORY_CURSOR_INVALID",
            "History cursor is invalid.",
        )
    })?;
    if cursor.version != HISTORY_CURSOR_VERSION || cursor.after.is_none() {
        return Err(presentation_error(
            "IMPORT_V2_HISTORY_CURSOR_INVALID",
            "History cursor is invalid.",
        ));
    }
    Ok(Some(cursor))
}

fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or_default()
}

fn file_modified_millis(path: &Path) -> u64 {
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

fn history_title(batch: &ImportBatchResult, session: Option<&ImportSession>) -> String {
    let names = session
        .map(|session| {
            batch
                .items
                .iter()
                .filter_map(|result| {
                    session
                        .items
                        .iter()
                        .find(|item| item.item_id == result.item_id)
                })
                .map(|item| item.input.display_name.clone())
                .take(2)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    match names.as_slice() {
        [name] => format!("Import: {name}"),
        [first, second] => format!("Import: {first}, {second}"),
        _ => format!(
            "Import batch {}",
            batch.batch_id.chars().take(8).collect::<String>()
        ),
    }
}

fn history_status(batch: &ImportBatchResult) -> &'static str {
    if batch.items.is_empty() {
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

#[tauri::command]
pub fn get_import_capability_requirement_v2(
    state: State<'_, AppState>,
    request: GetImportCapabilityRequirementV2Request,
) -> Result<ImportCapabilityRequirement, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let session =
        state
            .import_v2_service
            .load_session(&context, &state.file_store, &request.session_id)?;
    let item = session
        .items
        .iter()
        .find(|item| item.item_id == request.item_id)
        .ok_or_else(|| {
            presentation_error("IMPORT_V2_ITEM_NOT_FOUND", "Import item was not found.")
        })?;
    let (capability_id, route, license) = capability_for_item(item).ok_or_else(|| {
        presentation_error(
            "IMPORT_V2_CAPABILITY_NOT_REQUIRED",
            "This import item does not currently require a capability pack.",
        )
    })?;
    let requirement = CapabilityRequirement {
        capability_id: capability_id.into(),
        minimum_version: None,
        protocol_version: "2".into(),
        target_triple: target_triple(),
        accepted_license_expressions: vec![license.into()],
    };
    let available = state
        .import_capability_runtime
        .statuses()
        .into_iter()
        .any(|status| status.capability_id == capability_id && status.available);
    Ok(ImportCapabilityRequirement {
        requirement,
        route: route.into(),
        available,
        // The current runtime only resolves signed installed packs. It does not
        // own downloads, so the UI must present a fallback instead of a dead
        // install button until the pack manager exposes an install task.
        installable: false,
        compressed_bytes: None,
        installed_bytes: None,
        model_bytes: None,
        license: Some(license.into()),
        fallback: (!available).then_some("Install the signed capability pack from a release that includes it, then retry this item.".into()),
    })
}

#[tauri::command]
pub fn install_import_capability_v2(
    state: State<'_, AppState>,
    request: InstallImportCapabilityV2Request,
) -> Result<BackendTask, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let _ = state
        .import_v2_service
        .load_session(&context, &state.file_store, &request.session_id)?
        .items
        .iter()
        .find(|item| item.item_id == request.item_id)
        .ok_or_else(|| {
            presentation_error("IMPORT_V2_ITEM_NOT_FOUND", "Import item was not found.")
        })?;
    if !request.acknowledge_install {
        return Err(presentation_error(
            "IMPORT_V2_CAPABILITY_CONFIRMATION_REQUIRED",
            "Capability installation requires explicit confirmation.",
        ));
    }
    Err(presentation_error(
        "IMPORT_V2_CAPABILITY_INSTALL_UNAVAILABLE",
        "The installed runtime does not expose a signed capability installation task.",
    ))
}

fn read_staging_markdown(
    context: &crate::models::paths::ProjectContext,
    session_id: &str,
    item_id: &str,
    relative_path: &str,
    expected_hash: &str,
) -> Result<(String, bool, u64), BackendError> {
    validate_identifier(session_id)?;
    validate_identifier(item_id)?;
    let normalized = normalize_relative(relative_path)?;
    let relative =
        format!(".app/import-sessions/{session_id}/items/{item_id}/staging/{normalized}");
    let path = safe_project_path(&context.root, &relative)?;
    let metadata = fs::metadata(&path).map_err(|_| {
        presentation_error(
            "IMPORT_V2_PREVIEW_READ_FAILED",
            "Markdown preview could not be read.",
        )
    })?;
    let total_bytes = metadata.len();
    let truncated = total_bytes > IMPORT_V2_PREVIEW_MAX_BYTES;
    let file = fs::File::open(&path).map_err(|_| {
        presentation_error(
            "IMPORT_V2_PREVIEW_READ_FAILED",
            "Markdown preview could not be read.",
        )
    })?;
    let mut bytes = Vec::new();
    file.take(IMPORT_V2_PREVIEW_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| {
            presentation_error(
                "IMPORT_V2_PREVIEW_READ_FAILED",
                "Markdown preview could not be read.",
            )
        })?;
    if !truncated {
        let actual = sha256(&bytes);
        if !actual.eq_ignore_ascii_case(expected_hash) {
            return Err(presentation_error(
                "IMPORT_V2_PREVIEW_CHANGED",
                "Markdown preview changed before it was opened.",
            ));
        }
    }
    bytes.truncate(IMPORT_V2_PREVIEW_MAX_BYTES as usize);
    let markdown = String::from_utf8(bytes).map_err(|_| {
        presentation_error(
            "IMPORT_V2_PREVIEW_INVALID",
            "Markdown preview is not valid UTF-8.",
        )
    })?;
    Ok((markdown, truncated, total_bytes))
}

fn safe_project_path(root: &Path, relative: &str) -> Result<PathBuf, BackendError> {
    let canonical_root = fs::canonicalize(root).map_err(|_| {
        presentation_error("PROJECT_NOT_FOUND", "Project root could not be resolved.")
    })?;
    let mut current = canonical_root.clone();
    for component in Path::new(relative).components() {
        let Component::Normal(part) = component else {
            return Err(presentation_error(
                "PATH_INVALID",
                "Preview path is invalid.",
            ));
        };
        current.push(part);
        let metadata = fs::symlink_metadata(&current).map_err(|_| {
            presentation_error(
                "IMPORT_V2_PREVIEW_READ_FAILED",
                "Markdown preview could not be read.",
            )
        })?;
        if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
            return Err(presentation_error(
                "PATH_SYMLINK_REJECTED",
                "Preview path contains a link.",
            ));
        }
    }
    let canonical = fs::canonicalize(&current).map_err(|_| {
        presentation_error(
            "IMPORT_V2_PREVIEW_READ_FAILED",
            "Markdown preview could not be read.",
        )
    })?;
    if !canonical.starts_with(&canonical_root) {
        return Err(presentation_error(
            "PATH_OUTSIDE_PROJECT",
            "Preview path is outside the project.",
        ));
    }
    Ok(canonical)
}

fn normalize_relative(value: &str) -> Result<String, BackendError> {
    let normalized = value.replace('\\', "/");
    let path = Path::new(&normalized);
    if normalized.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(presentation_error(
            "PATH_INVALID",
            "Preview path is invalid.",
        ));
    }
    Ok(normalized)
}

fn validate_identifier(value: &str) -> Result<(), BackendError> {
    if !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        Ok(())
    } else {
        Err(presentation_error(
            "IMPORT_V2_SESSION_INVALID",
            "Import identity is invalid.",
        ))
    }
}

fn capability_for_item(item: &ImportItem) -> Option<(&'static str, &'static str, &'static str)> {
    let actions = item.issue.as_ref()?.recovery_actions.as_slice();
    if actions.contains(&ImportRecoveryAction::InstallBrowserCapability) {
        Some(("browser-runtime", "web.generic.browser", "Apache-2.0"))
    } else if actions.contains(&ImportRecoveryAction::InstallMediaCapability) {
        Some(("media-runtime", "media.subtitle", "LGPL-2.1-or-later"))
    } else if actions.contains(&ImportRecoveryAction::InstallCapability) {
        Some(("document-standard", "pack.markitdown", "MIT"))
    } else {
        None
    }
}

fn target_triple() -> String {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "windows") => "x86_64-pc-windows-msvc",
        ("aarch64", "macos") => "aarch64-apple-darwin",
        ("x86_64", "macos") => "x86_64-apple-darwin",
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        _ => "unsupported-target",
    }
    .into()
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
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

fn presentation_error(code: &'static str, message: impl Into<String>) -> BackendError {
    BackendError::new(code, message, true, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::import_v2::ImportItemCommitResult;

    fn batch(items: Vec<ImportItemCommitResult>) -> ImportBatchResult {
        let committed_count = items.iter().filter(|item| item.committed).count() as u32;
        ImportBatchResult {
            batch_id: "batch-1".into(),
            session_id: "session-1".into(),
            created_at: "2026-07-15T00:00:00Z".into(),
            batch_task_id: None,
            committed_count,
            failed_count: items.len() as u32 - committed_count,
            items,
            history_snapshot: None,
        }
    }

    fn item(id: &str, committed: bool, error_code: Option<&str>) -> ImportItemCommitResult {
        ImportItemCommitResult {
            item_id: id.into(),
            source_id: committed.then(|| "source-1".into()),
            version_id: committed.then(|| "version-1".into()),
            wiki_path: committed.then(|| "wiki/item.md".into()),
            committed,
            error_code: error_code.map(str::to_string),
        }
    }

    #[test]
    fn history_status_describes_partial_and_cancelled_batches() {
        assert_eq!(history_status(&batch(Vec::new())), "processing");
        assert_eq!(
            history_status(&batch(vec![item("a", true, None)])),
            "completed"
        );
        assert_eq!(
            history_status(&batch(vec![
                item("a", true, None),
                item("b", false, Some("E"))
            ])),
            "partially_committed"
        );
        assert_eq!(
            history_status(&batch(vec![item(
                "a",
                false,
                Some(crate::errors::IMPORT_V2_CANCELLED)
            )])),
            "cancelled"
        );
        assert_eq!(
            history_status(&batch(vec![item("a", false, Some("E"))])),
            "failed"
        );
    }

    #[test]
    fn history_cursor_is_opaque_and_rejects_legacy_offsets() {
        let cursor = HistoryCursor {
            version: HISTORY_CURSOR_VERSION,
            snapshot_millis: 123,
            after: Some(HistoryCursorKey {
                modified_millis: 100,
                kind: 0,
                id: "batch-1".into(),
            }),
        };
        let encoded = serde_json::to_string(&cursor).unwrap();
        let decoded = parse_history_cursor(Some(&encoded)).unwrap().unwrap();
        assert_eq!(decoded.snapshot_millis, 123);
        assert_eq!(decoded.after.unwrap().id, "batch-1");
        assert!(parse_history_cursor(Some("50")).is_err());
    }

    #[test]
    fn history_cursor_sort_is_deterministic_for_equal_timestamps() {
        let v2 = HistoryCursorKey {
            modified_millis: 100,
            kind: 0,
            id: "batch-1".into(),
        };
        let legacy = HistoryCursorKey {
            modified_millis: 100,
            kind: 1,
            id: "legacy-1".into(),
        };
        assert_eq!(history_key_cmp(&v2, &legacy), Ordering::Less);
        assert_eq!(history_key_cmp(&legacy, &v2), Ordering::Greater);
    }

    #[test]
    fn platform_readiness_uses_registered_routes_and_capability_statuses() {
        let routes = vec!["web.generic.readability".into(), "web.wechat.article".into()];
        let capabilities = vec![CapabilityRuntimeStatus {
            capability_id: "browser-runtime-lite".into(),
            route: "web.zhihu.content".into(),
            available: false,
            reason: Some("not installed".into()),
        }];
        let platforms = platform_readiness(&routes, &capabilities);

        let http = platforms.iter().find(|item| item.id == "http").unwrap();
        let wechat = platforms.iter().find(|item| item.id == "wechat").unwrap();
        let zhihu = platforms.iter().find(|item| item.id == "zhihu").unwrap();
        let x = platforms.iter().find(|item| item.id == "x").unwrap();
        assert!(http.available);
        assert!(wechat.available);
        assert!(!zhihu.available);
        assert_eq!(zhihu.reason_code.as_deref(), Some("capability_missing"));
        assert_eq!(x.reason_code.as_deref(), Some("phase_two"));
    }
}
