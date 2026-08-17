use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::commands::import_v2_commands::{
    start_import_items_for_state, StartImportItemsV2Request,
};
use crate::errors::BackendError;
use crate::models::import_v2::{
    ArtifactKind, ImportAsrProfile, ImportBatchResult, ImportInputKind, ImportItem,
    ImportItemStatus, ImportRecoveryAction, ImportResolutionKind, ImportResourceMode,
    ImportSession,
};
use crate::models::import_v2_file::CapabilityRequirement;
use crate::models::import_v2_migration::{LegacyHistoryView, MigrationStatus};
use crate::models::import_v2_presentation::{
    GetImportAsrEnablementPlanV2Request, GetImportCapabilityRequirementV2Request,
    GetImportFrontendReadinessV2Request, GetImportPreviewContentV2Request, ImportAsrDependency,
    ImportAsrDependencyKind, ImportAsrEnablementPlan, ImportAsrProfilePlan,
    ImportCapabilityReadiness, ImportCapabilityRequirement, ImportFeatureReadiness,
    ImportFrontendReadiness, ImportHistoryAction, ImportHistoryEntry, ImportHistoryPage,
    ImportPlatformReadiness, ImportPreviewComparison, ImportPreviewContent, ImportPreviewResource,
    ImportPreviewTarget, ImportWorkbenchPreferences, ImportWorkbenchPreferencesRequest,
    InstallImportCapabilityV2Request, ListImportHistoryV2Request,
    SaveImportWorkbenchPreferencesRequest, IMPORT_V2_PREVIEW_MAX_BYTES,
    IMPORT_V2_WORKBENCH_PREFERENCES_PATH,
};
use crate::models::paths::ProjectContext;
use crate::models::task::{BackendTask, TaskResult, TaskStatus, TaskType};
use crate::services::import_v2::activation::ImportV2ActivationService;
use crate::services::import_v2::capability_installer::{catalog_entry, install_catalog_entry};
use crate::services::import_v2::capability_runtime::CapabilityRuntimeStatus;
use crate::services::import_v2::migration::{
    LegacyHistoryAdapter, MigrationService, REQUIRED_IMPORT_V2_CONTRACT,
};

#[tauri::command]
pub fn get_import_workbench_preferences_v2(
    state: State<'_, AppState>,
    request: ImportWorkbenchPreferencesRequest,
) -> Result<ImportWorkbenchPreferences, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    if !state
        .file_store
        .exists(&context, IMPORT_V2_WORKBENCH_PREFERENCES_PATH)
    {
        return Ok(ImportWorkbenchPreferences::default());
    }
    let preferences: ImportWorkbenchPreferences = state
        .file_store
        .read_json(&context, IMPORT_V2_WORKBENCH_PREFERENCES_PATH)?;
    validate_workbench_preferences(&preferences)?;
    Ok(preferences)
}

#[tauri::command]
pub fn save_import_workbench_preferences_v2(
    state: State<'_, AppState>,
    request: SaveImportWorkbenchPreferencesRequest,
) -> Result<ImportWorkbenchPreferences, BackendError> {
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, context| {
            validate_workbench_preferences(&request.preferences)?;
            state.file_store.write_json_atomic(
                context,
                IMPORT_V2_WORKBENCH_PREFERENCES_PATH,
                &request.preferences,
            )?;
            Ok(request.preferences.clone())
        },
    )
}

fn validate_workbench_preferences(
    preferences: &ImportWorkbenchPreferences,
) -> Result<(), BackendError> {
    const MAX_SCROLL_TOP: u32 = 10_000_000;
    if preferences.schema_version != 1
        || preferences.workbench_scroll_top > MAX_SCROLL_TOP
        || preferences.capabilities_scroll_top > MAX_SCROLL_TOP
        || preferences.history_scroll_top > MAX_SCROLL_TOP
    {
        return Err(presentation_error(
            "IMPORT_V2_WORKBENCH_PREFERENCES_INVALID",
            "Import workbench preferences are invalid.",
        ));
    }
    Ok(())
}

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
            state.import_v2_service.load_session(
                &context,
                &state.file_store,
                &request.session_id,
            )?
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
    let preview_details = item.preview.as_ref().ok_or_else(|| {
        presentation_error(
            "IMPORT_V2_PREVIEW_NOT_FOUND",
            "The import item has no Markdown preview.",
        )
    })?;
    let resources = read_preview_resources(
        &context,
        &request.session_id,
        &request.item_id,
        preview_details,
    )?;
    let target = preview_target(&context, &state.file_store, &session, item, preview_details)?;
    let quality = preview_details.quality.clone();
    let raw_label = item.input.display_name.clone();

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
                    markdown: read_history_markdown(&history_path, &expected_hash)?,
                    truncated: fs::metadata(&history_path)
                        .map(|metadata| metadata.len() > IMPORT_V2_PREVIEW_MAX_BYTES)
                        .unwrap_or(false),
                    total_bytes: fs::metadata(&history_path)
                        .map(|metadata| metadata.len())
                        .unwrap_or_default(),
                    sha256: expected_hash,
                    target,
                    quality,
                    raw_label,
                    resources,
                    comparison: None,
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
    let comparison = if request.history_batch_id.is_none() {
        preview_comparison(
            &context,
            &state.file_store,
            &request.session_id,
            &request.item_id,
            preview_details,
        )?
    } else {
        None
    };
    Ok(ImportPreviewContent {
        session_id: request.session_id,
        item_id: request.item_id,
        candidate_id: request.candidate_id,
        title,
        markdown,
        truncated,
        total_bytes,
        sha256: expected_hash,
        target,
        quality,
        raw_label,
        resources,
        comparison,
    })
}

fn preview_comparison(
    context: &ProjectContext,
    files: &crate::services::FileStore,
    session_id: &str,
    item_id: &str,
    preview: &crate::models::import_v2::ImportPreviewArtifact,
) -> Result<Option<ImportPreviewComparison>, BackendError> {
    let Some(resolution) = preview.resolution.as_ref() else {
        return Ok(None);
    };
    if !matches!(
        resolution.kind,
        ImportResolutionKind::SameSourceNewVersion | ImportResolutionKind::NeedsThreeWayMerge
    ) {
        return Ok(None);
    }
    let Some(binding) = resolution.binding.as_ref() else {
        return Ok(None);
    };
    let manifest = crate::services::import_v2::source_registry::SourceRegistry::read_manifest(
        context,
        files,
        &format!(".app/sources/{}.json", binding.source_id),
    )?;
    let current_path = safe_project_path(&context.root, &manifest.wiki_path)?;
    let current_bytes = fs::read(current_path).map_err(|_| {
        presentation_error(
            "IMPORT_V2_PREVIEW_COMPARISON_READ_FAILED",
            "The existing Source could not be read for comparison.",
        )
    })?;
    if !sha256(&current_bytes).eq_ignore_ascii_case(&binding.current_hash) {
        return Err(presentation_error(
            "IMPORT_V2_PREVIEW_COMPARISON_CHANGED",
            "The existing Source changed before the comparison was opened.",
        ));
    }
    let current_markdown = bounded_preview_markdown(current_bytes)?;
    let merged_markdown = preview
        .manual_merge
        .as_ref()
        .map(|artifact| {
            read_staging_markdown(
                context,
                session_id,
                item_id,
                &artifact.relative_path,
                &artifact.sha256,
            )
            .map(|(markdown, _, _)| markdown)
        })
        .transpose()?;
    Ok(Some(ImportPreviewComparison {
        current_markdown,
        merged_markdown,
    }))
}

fn bounded_preview_markdown(bytes: Vec<u8>) -> Result<String, BackendError> {
    let mut markdown = String::from_utf8(bytes).map_err(|_| {
        presentation_error(
            "IMPORT_V2_PREVIEW_INVALID",
            "Markdown preview is not valid UTF-8.",
        )
    })?;
    if markdown.len() > IMPORT_V2_PREVIEW_MAX_BYTES as usize {
        let mut end = IMPORT_V2_PREVIEW_MAX_BYTES as usize;
        while !markdown.is_char_boundary(end) {
            end -= 1;
        }
        markdown.truncate(end);
    }
    Ok(markdown)
}

fn preview_target(
    context: &ProjectContext,
    files: &crate::services::FileStore,
    session: &ImportSession,
    item: &ImportItem,
    preview: &crate::models::import_v2::ImportPreviewArtifact,
) -> Result<ImportPreviewTarget, BackendError> {
    let resolution = preview.resolution.as_ref();
    let binding = resolution.and_then(|value| value.binding.as_ref());
    let wiki_path = if let Some(path) = resolution.and_then(|value| value.target_wiki_path.clone())
    {
        Some(path)
    } else if let Some(binding) = binding {
        crate::services::import_v2::source_registry::SourceRegistry::read_manifest(
            context,
            files,
            &format!(".app/sources/{}.json", binding.source_id),
        )
        .ok()
        .map(|manifest| manifest.wiki_path)
    } else {
        crate::services::import_v2::commit::planned_new_source_wiki_path(
            context,
            files,
            session,
            &item.item_id,
        )?
    };
    let disposition = match resolution.map(|value| &value.kind) {
        Some(ImportResolutionKind::ExactDuplicate) => "duplicate",
        Some(ImportResolutionKind::SameSourceNewVersion) => "update",
        Some(ImportResolutionKind::NeedsThreeWayMerge) => "merge",
        _ => "new_source",
    };
    Ok(ImportPreviewTarget {
        disposition: disposition.into(),
        source_id: binding.map(|value| value.source_id.clone()),
        version_id: binding.map(|value| value.target_version_id.clone()),
        wiki_path,
    })
}

fn read_preview_resources(
    context: &ProjectContext,
    session_id: &str,
    item_id: &str,
    preview: &crate::models::import_v2::ImportPreviewArtifact,
) -> Result<Vec<ImportPreviewResource>, BackendError> {
    const MAX_INLINE_IMAGE_BYTES: u64 = 5 * 1024 * 1024;
    validate_identifier(session_id)?;
    validate_identifier(item_id)?;
    preview
        .assets
        .iter()
        .map(|artifact| {
            let source = normalize_relative(&artifact.relative_path)?;
            let name = Path::new(&source)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("resource")
                .to_string();
            let kind = match artifact.kind {
                ArtifactKind::Image => "image",
                ArtifactKind::Attachment => "attachment",
                ArtifactKind::Subtitle => "subtitle",
                ArtifactKind::Transcript => "transcript",
                ArtifactKind::Metadata => "metadata",
                ArtifactKind::SourceEvidence => "source_evidence",
                ArtifactKind::SourceSnapshot => "source_snapshot",
                ArtifactKind::Markdown => "markdown",
            }
            .to_string();
            let data_url = if artifact.kind == ArtifactKind::Image
                && artifact.size_bytes <= MAX_INLINE_IMAGE_BYTES
            {
                let relative =
                    format!(".app/import-sessions/{session_id}/items/{item_id}/staging/{source}");
                let path = safe_project_path(&context.root, &relative)?;
                let bytes = fs::read(path).map_err(|_| {
                    presentation_error(
                        "IMPORT_V2_PREVIEW_RESOURCE_READ_FAILED",
                        "A preview resource could not be read.",
                    )
                })?;
                if sha256(&bytes).eq_ignore_ascii_case(&artifact.sha256) {
                    image_mime(&name, &bytes).map(|mime| {
                        format!(
                            "data:{mime};base64,{}",
                            base64::engine::general_purpose::STANDARD.encode(bytes)
                        )
                    })
                } else {
                    None
                }
            } else {
                None
            };
            Ok(ImportPreviewResource {
                source,
                name,
                kind,
                size_bytes: artifact.size_bytes,
                data_url,
            })
        })
        .collect()
}

fn image_mime(name: &str, bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        match Path::new(name)
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("svg") => None,
            _ => None,
        }
    }
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
    let registered_routes = state.import_v2_service.registered_engine_routes()?;
    let capability_statuses = state.import_capability_runtime.statuses();
    let files = file_readiness(&registered_routes, &capability_statuses);
    let platforms = platform_readiness(&registered_routes, &capability_statuses);
    let abilities = ability_readiness(&registered_routes, &capability_statuses);
    let capabilities = capability_statuses
        .into_iter()
        .map(|status| ImportCapabilityReadiness {
            capability_id: status.capability_id,
            route: status.route,
            available: status.available,
            reason_code: status.reason,
        })
        .collect();
    Ok(ImportFrontendReadiness {
        backend_version: REQUIRED_IMPORT_V2_CONTRACT.into(),
        active,
        migration_status: migration.status,
        unfinished_session_id: state
            .import_v2_service
            .find_unfinished_session(&context, &state.file_store)?,
        legacy_history_available: !legacy_history.entries.is_empty(),
        files,
        platforms,
        abilities,
        capabilities,
    })
}

fn file_readiness(
    registered_routes: &[String],
    capability_statuses: &[CapabilityRuntimeStatus],
) -> Vec<ImportFeatureReadiness> {
    let route = |id: &str, candidates: &[&str]| {
        let available = candidates.iter().any(|candidate| {
            registered_routes.iter().any(|route| route == candidate)
                || capability_statuses
                    .iter()
                    .any(|status| status.route == *candidate && status.available)
        });
        ImportFeatureReadiness {
            id: id.into(),
            available,
            reason_code: (!available).then(|| "capability_missing".into()),
        }
    };
    vec![
        route("doc", &["pack.office-legacy", "pack.markitdown"]),
        route(
            "docx",
            &["office.modern.docx", "pack.markitdown", "pack.office-oxide"],
        ),
        route("xls", &["pack.office-legacy", "pack.markitdown"]),
        route("pdf", &["pdf.text", "pdf.layout", "pack.markitdown"]),
        route(
            "xlsx",
            &["office.modern.xlsx", "pack.markitdown", "pack.office-oxide"],
        ),
        route("pptx", &["office.modern.pptx", "pack.markitdown"]),
        route("ppt", &["pack.office-legacy", "pack.markitdown"]),
        route("md", &["file.native"]),
        route("txt", &["file.native"]),
        route("html", &["file.native"]),
        route("csv", &["file.csv-package"]),
        route("png", &["ocr.cjk-accurate", "ocr.basic"]),
        route("jpeg", &["ocr.cjk-accurate", "ocr.basic"]),
        route("webp", &["ocr.cjk-accurate", "ocr.basic"]),
        route("bmp", &["ocr.cjk-accurate", "ocr.basic"]),
        route("tiff", &["ocr.cjk-accurate", "ocr.basic"]),
        route("heic", &["ocr.cjk-accurate", "ocr.basic"]),
        route("heif", &["ocr.cjk-accurate", "ocr.basic"]),
        route("gif", &["ocr.cjk-accurate", "ocr.basic"]),
        route("mp3", &["media.companion", "media.asr"]),
        route("wav", &["media.companion", "media.asr"]),
        route("m4a", &["media.companion", "media.asr"]),
        route("aac", &["media.companion", "media.asr"]),
        route("flac", &["media.companion", "media.asr"]),
        route("ogg", &["media.companion", "media.asr"]),
        route("opus", &["media.companion", "media.asr"]),
        route("wma", &["media.companion", "media.asr"]),
        route("mp4", &["media.companion", "media.asr"]),
        route("mov", &["media.companion", "media.asr"]),
        route("mkv", &["media.companion", "media.asr"]),
        route("webm", &["media.companion", "media.asr"]),
        route("avi", &["media.companion", "media.asr"]),
        route("m4v", &["media.companion", "media.asr"]),
        route("wmv", &["media.companion", "media.asr"]),
        route("srt", &["media.subtitle"]),
        route("vtt", &["media.subtitle"]),
        route("ass", &["media.subtitle"]),
        ImportFeatureReadiness {
            id: "lrc".into(),
            available: false,
            reason_code: Some("batch_four".into()),
        },
    ]
}

fn ability_readiness(
    registered_routes: &[String],
    capability_statuses: &[CapabilityRuntimeStatus],
) -> Vec<ImportFeatureReadiness> {
    let route = |id: &str, candidates: &[&str]| {
        let available = candidates.iter().any(|candidate| {
            registered_routes.iter().any(|route| route == candidate)
                || capability_statuses
                    .iter()
                    .any(|status| status.route == *candidate && status.available)
        });
        ImportFeatureReadiness {
            id: id.into(),
            available,
            reason_code: (!available).then(|| "capability_missing".into()),
        }
    };
    vec![
        ImportFeatureReadiness {
            id: "subtitle".into(),
            available: true,
            reason_code: None,
        },
        route("local_asr", &["media.asr"]),
        route("ocr", &["ocr.cjk-accurate", "ocr.basic"]),
        ImportFeatureReadiness {
            id: "keyframes".into(),
            available: false,
            reason_code: Some("phase_two".into()),
        },
    ]
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
            registered_routes
                .iter()
                .any(|registered| registered == route)
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
    let xiaohongshu = route_status("xiaohongshu", &["web.xiaohongshu.note"], false);
    let douyin = route_status("douyin", &["web.douyin.video"], false);
    let x = route_status("x", &[], true);
    vec![http, wechat, zhihu, bilibili, xiaohongshu, douyin, x]
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
                && after.as_ref().map_or(true, |key| {
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
            Self::V2 {
                modified_millis, ..
            }
            | Self::Legacy {
                modified_millis, ..
            } => *modified_millis,
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
        if batch.completion.as_ref().is_some_and(|completion| {
            !completion.new_sources.is_empty() || !completion.updated_sources.is_empty()
        }) {
            available_actions.push(ImportHistoryAction::UpdateWiki);
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
    let catalog = catalog_entry(capability_id, &requirement.target_triple);
    Ok(ImportCapabilityRequirement {
        requirement,
        route: route.into(),
        available,
        installable: !available && catalog.is_some(),
        compressed_bytes: catalog.as_ref().map(|entry| entry.compressed_bytes),
        installed_bytes: catalog.as_ref().map(|entry| entry.installed_bytes),
        model_bytes: catalog.as_ref().and_then(|entry| entry.model_bytes),
        license: Some(license.into()),
        fallback: (!available && catalog.is_none()).then_some("This source build has no signed capability artifact for the current target. Release CI must publish the target pack and catalog entry before installation can be enabled.".into()),
    })
}

#[tauri::command]
pub fn get_import_asr_enablement_plan_v2(
    state: State<'_, AppState>,
    request: GetImportAsrEnablementPlanV2Request,
) -> Result<ImportAsrEnablementPlan, BackendError> {
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
    if !item.issue.as_ref().is_some_and(|issue| {
        issue
            .recovery_actions
            .contains(&ImportRecoveryAction::AuthorizeLocalAsr)
            || issue
                .recovery_actions
                .contains(&ImportRecoveryAction::InstallMediaCapability)
    }) {
        return Err(presentation_error(
            "IMPORT_V2_ASR_NOT_REQUIRED",
            "This import item does not currently require local speech recognition.",
        ));
    }

    let install_root = state.import_capability_runtime.install_root();
    let available_memory_bytes = available_memory_bytes();
    let disk_probe_root = install_root.as_deref().unwrap_or(&context.root);
    let available_disk_bytes = available_disk_bytes(disk_probe_root);
    let media_duration_seconds = local_wav_duration_seconds(item);
    let statuses = state.import_capability_runtime.statuses();
    let target = target_triple();
    let profiles = [
        AsrProfileSpec {
            profile: ImportAsrProfile::Fast,
            capability_id: "asr-sensevoice-small",
            engine_name: "sherpa-onnx SenseVoiceSmall",
            model_name: "SenseVoiceSmall int8",
            source: "https://github.com/k2-fsa/sherpa-onnx",
            license: "Apache-2.0 AND LGPL-3.0-or-later AND MIT",
            speed_numerator: 1,
            speed_denominator: 2,
        },
        AsrProfileSpec {
            profile: ImportAsrProfile::Balanced,
            capability_id: "asr-sensevoice-small",
            engine_name: "sherpa-onnx SenseVoiceSmall",
            model_name: "SenseVoiceSmall int8",
            source: "https://github.com/k2-fsa/sherpa-onnx",
            license: "Apache-2.0 AND LGPL-3.0-or-later AND MIT",
            speed_numerator: 3,
            speed_denominator: 4,
        },
        AsrProfileSpec {
            profile: ImportAsrProfile::Accurate,
            capability_id: "asr-whisper",
            engine_name: "whisper.cpp",
            model_name: "Whisper small",
            source: "https://github.com/ggml-org/whisper.cpp",
            license: "MIT AND LGPL-2.1-or-later",
            speed_numerator: 5,
            speed_denominator: 4,
        },
    ]
    .into_iter()
    .map(|spec| build_asr_profile_plan(spec, &statuses, &target, media_duration_seconds))
    .collect::<Vec<_>>();
    let recommended_profile = recommend_asr_profile(
        &profiles,
        &session.resource_mode,
        available_memory_bytes,
        available_disk_bytes,
    );

    Ok(ImportAsrEnablementPlan {
        recommended_profile,
        available_memory_bytes,
        available_disk_bytes,
        media_duration_seconds,
        install_location: install_root.map(|path| path.to_string_lossy().into_owned()),
        local_only: true,
        profiles,
    })
}

#[tauri::command]
pub fn install_import_capability_v2(
    app: AppHandle,
    state: State<'_, AppState>,
    request: InstallImportCapabilityV2Request,
) -> Result<BackendTask, BackendError> {
    if !request.acknowledge_install {
        return Err(presentation_error(
            "IMPORT_V2_CAPABILITY_CONFIRMATION_REQUIRED",
            "Capability installation requires explicit confirmation.",
        ));
    }
    let target = target_triple();
    let entry = catalog_entry(&request.capability_id, &target).ok_or_else(|| {
        presentation_error(
            "IMPORT_V2_CAPABILITY_INSTALL_UNAVAILABLE",
            "No signed capability release is available for this target.",
        )
    })?;
    let install_root = state.import_capability_runtime.install_root().ok_or_else(|| {
        presentation_error(
            "IMPORT_V2_CAPABILITY_INSTALL_UNAVAILABLE",
            "Capability installation is unavailable before the application data directory is initialized.",
        )
    })?;
    let (task, context) = state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, context| {
            let session = state.import_v2_service.load_session(
                context,
                &state.file_store,
                &request.session_id,
            )?;
            let item = session
                .items
                .iter()
                .find(|item| item.item_id == request.item_id)
                .ok_or_else(|| {
                    presentation_error("IMPORT_V2_ITEM_NOT_FOUND", "Import item was not found.")
                })?;
            let asr_choice_allowed = item.issue.as_ref().is_some_and(|issue| {
                issue
                    .recovery_actions
                    .contains(&ImportRecoveryAction::AuthorizeLocalAsr)
                    || issue
                        .recovery_actions
                        .contains(&ImportRecoveryAction::InstallMediaCapability)
            }) && matches!(
                request.capability_id.as_str(),
                "asr-sensevoice-small" | "asr-whisper"
            );
            let required_capability = capability_for_item(item);
            if required_capability.is_none() && !asr_choice_allowed {
                return Err(presentation_error(
                    "IMPORT_V2_CAPABILITY_NOT_REQUIRED",
                    "This import item does not currently require a capability pack.",
                ));
            }
            if required_capability.is_some_and(|(expected_capability_id, _, _)| {
                request.capability_id != expected_capability_id
            }) && !asr_choice_allowed
            {
                return Err(presentation_error(
                    "IMPORT_V2_CAPABILITY_MISMATCH",
                    "The requested capability does not match this import item.",
                ));
            }
            let task = state
                .task_service
                .create_project_task(
                    TaskType::Import,
                    request.project_id.clone(),
                    context.root.clone(),
                    format!("Install {}", request.capability_id),
                    true,
                )
                .map_err(|error| presentation_error("IMPORT_V2_TASK_FAILED", &error))?;
            Ok((task, context.clone()))
        },
    )?;
    let task_id = task.id.clone();
    let execution_lease = match state.begin_project_external_task(&context, &task_id) {
        Ok(lease) => lease,
        Err(error) => {
            let _ = state
                .task_service
                .discard_unstarted_tasks(std::slice::from_ref(&task_id));
            return Err(error);
        }
    };
    let project_id = request.project_id.clone();
    let project_root_path = request.project_root_path.clone();
    let session_id = request.session_id.clone();
    let item_id = request.item_id.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let _execution_lease = execution_lease;
        if state
            .task_service
            .transition_status(&task_id, TaskStatus::Running)
            .is_err()
        {
            return;
        }
        let Some(token) = state.task_service.get_cancellation_token(&task_id) else {
            return;
        };
        let install = install_catalog_entry(&install_root, &entry, &token, |current, total| {
            let _ = state.task_service.update_progress(
                &task_id,
                current,
                Some(total),
                Some("Downloading and verifying capability".into()),
            );
        })
        .await;
        if let Err(error) = install {
            finish_capability_install_error(&state, &task_id, error);
            return;
        }
        if token.is_cancelled() {
            let _ = state
                .task_service
                .transition_status(&task_id, TaskStatus::Cancelled);
            return;
        }
        state
            .import_capability_runtime
            .load_installed(&install_root, &state.import_v2_service);
        let available = state
            .import_capability_runtime
            .statuses()
            .into_iter()
            .any(|status| status.capability_id == entry.capability_id && status.available);
        if !available {
            finish_capability_install_error(
                &state,
                &task_id,
                presentation_error(
                    "IMPORT_V2_CAPABILITY_INSTALL_FAILED",
                    "The installed capability did not pass runtime verification.",
                ),
            );
            return;
        }
        let resume_action =
            (entry.capability_id == "ocr-cjk-accurate").then_some(ImportRecoveryAction::EnableOcr);
        let resume_result = state.with_current_project_write_access(
            &project_id,
            &project_root_path,
            |permit, context| {
                let loaded_session = state.import_v2_service.load_session(
                    context,
                    &state.file_store,
                    &session_id,
                )?;
                let asr_capability = matches!(
                    entry.capability_id.as_str(),
                    "asr-sensevoice-small" | "asr-whisper"
                );
                let mut resume_item_ids = loaded_session
                    .items
                    .iter()
                    .filter(|candidate| {
                        capability_for_item(candidate).is_some_and(|(capability_id, _, _)| {
                            capability_id == entry.capability_id.as_str()
                        }) || (asr_capability
                            && candidate.issue.as_ref().is_some_and(|issue| {
                                issue
                                    .recovery_actions
                                    .contains(&ImportRecoveryAction::AuthorizeLocalAsr)
                                    || issue
                                        .recovery_actions
                                        .contains(&ImportRecoveryAction::InstallMediaCapability)
                            }))
                    })
                    .map(|candidate| candidate.item_id.clone())
                    .collect::<Vec<_>>();
                if resume_item_ids.is_empty() {
                    resume_item_ids.push(item_id.clone());
                }
                let resume_tasks = start_import_items_for_state(
                    app.clone(),
                    &state,
                    permit,
                    StartImportItemsV2Request {
                        project_id: project_id.clone(),
                        project_root_path: project_root_path.clone(),
                        session_id: session_id.clone(),
                        item_ids: resume_item_ids,
                        recovery_action: resume_action.clone(),
                    },
                )?;
                state
                    .task_service
                    .complete_running_with_result(
                        &task_id,
                        TaskResult {
                            summary: format!(
                                "Installed {} and created {} automatic resume task(s).",
                                entry.capability_id,
                                resume_tasks.len()
                            ),
                            affected_paths: Vec::new(),
                            reference: None,
                            pending_action: None,
                        },
                    )
                    .map_err(|error| presentation_error("IMPORT_V2_TASK_FAILED", &error))?;
                Ok(())
            },
        );
        if let Err(error) = resume_result {
            finish_capability_install_error(&state, &task_id, error);
        }
    });
    Ok(task)
}

fn finish_capability_install_error(state: &AppState, task_id: &str, error: BackendError) {
    if state.task_service.is_cancelled(task_id) {
        let _ = state
            .task_service
            .transition_status(task_id, TaskStatus::Cancelled);
    } else {
        let _ = state.task_service.set_error(task_id, error);
        let _ = state
            .task_service
            .transition_status(task_id, TaskStatus::Failed);
    }
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
    const BROWSER_BUNDLE_LICENSE: &str = "Apache-2.0 AND MIT AND BSD-2-Clause AND BSD-3-Clause AND ISC AND MIT-0 AND LicenseRef-Bundled-Third-Party-Notices";
    let actions = item.issue.as_ref()?.recovery_actions.as_slice();
    if actions.contains(&ImportRecoveryAction::InstallBrowserCapability) {
        Some((
            "browser-runtime",
            "web.generic.browser",
            BROWSER_BUNDLE_LICENSE,
        ))
    } else if actions.contains(&ImportRecoveryAction::InstallMediaCapability) {
        Some((
            "asr-sensevoice-small",
            "media.asr",
            "Apache-2.0 AND LGPL-3.0-or-later AND MIT",
        ))
    } else if actions.contains(&ImportRecoveryAction::InstallOcrCapability) {
        Some((
            "ocr-cjk-accurate",
            "ocr.cjk-accurate",
            "Apache-2.0 AND MIT AND BSD-3-Clause AND HPND AND MPL-2.0 AND PSF-2.0 AND LGPL-2.1-only AND LGPL-3.0-only",
        ))
    } else if actions.contains(&ImportRecoveryAction::InstallCapability) {
        Some(("document-standard", "pack.markitdown", "MIT"))
    } else {
        None
    }
}

#[derive(Clone)]
struct AsrProfileSpec {
    profile: ImportAsrProfile,
    capability_id: &'static str,
    engine_name: &'static str,
    model_name: &'static str,
    source: &'static str,
    license: &'static str,
    speed_numerator: u64,
    speed_denominator: u64,
}

fn build_asr_profile_plan(
    spec: AsrProfileSpec,
    statuses: &[CapabilityRuntimeStatus],
    target: &str,
    media_duration_seconds: Option<u64>,
) -> ImportAsrProfilePlan {
    let runtime = statuses
        .iter()
        .find(|status| status.capability_id == spec.capability_id && status.route == "media.asr");
    let available = runtime.is_some_and(|status| status.available);
    let catalog = catalog_entry(spec.capability_id, target);
    let installable = !available && catalog.is_some();
    let component_available = available;
    let dependency = |kind, name: &str, source: &str, license: &str| ImportAsrDependency {
        kind,
        name: name.into(),
        available: component_available,
        bundled_with_capability: true,
        source: source.into(),
        license: license.into(),
    };
    let dependencies = vec![
        dependency(
            ImportAsrDependencyKind::MediaRuntime,
            "FFmpeg local media runtime",
            "https://ffmpeg.org/",
            "LGPL-2.1-or-later",
        ),
        dependency(
            ImportAsrDependencyKind::Engine,
            spec.engine_name,
            spec.source,
            spec.license,
        ),
        dependency(
            ImportAsrDependencyKind::Model,
            spec.model_name,
            spec.source,
            spec.license,
        ),
        dependency(
            ImportAsrDependencyKind::LanguageSupport,
            "Multilingual recognition support",
            spec.source,
            spec.license,
        ),
    ];
    let estimated_seconds = media_duration_seconds.map(|duration| {
        duration
            .saturating_mul(spec.speed_numerator)
            .div_ceil(spec.speed_denominator)
            .max(1)
    });
    let unavailable_reason_code = (!available).then(|| {
        if installable {
            "not_installed".into()
        } else {
            runtime
                .and_then(|status| status.reason.clone())
                .unwrap_or_else(|| "signed_release_unavailable".into())
        }
    });
    ImportAsrProfilePlan {
        profile: spec.profile,
        capability_id: spec.capability_id.into(),
        engine_name: spec.engine_name.into(),
        model_name: spec.model_name.into(),
        available,
        installable,
        download_bytes: (!available)
            .then(|| catalog.as_ref().map(|entry| entry.compressed_bytes))
            .flatten(),
        installed_bytes: catalog.as_ref().map(|entry| entry.installed_bytes),
        model_bytes: catalog.as_ref().and_then(|entry| entry.model_bytes),
        device: "cpu".into(),
        estimated_seconds,
        unavailable_reason_code,
        dependencies,
    }
}

fn recommend_asr_profile(
    profiles: &[ImportAsrProfilePlan],
    resource_mode: &ImportResourceMode,
    available_memory: Option<u64>,
    available_disk: Option<u64>,
) -> ImportAsrProfile {
    let usable = |profile: &ImportAsrProfile| {
        profiles
            .iter()
            .find(|plan| &plan.profile == profile)
            .is_some_and(|plan| {
                (plan.available || plan.installable)
                    && available_disk.is_none_or(|free| {
                        plan.installed_bytes
                            .is_none_or(|required| free >= required.saturating_mul(2))
                    })
            })
    };
    let low_memory = available_memory.is_some_and(|bytes| bytes < 6 * 1024 * 1024 * 1024);
    let high_memory = available_memory.is_none_or(|bytes| bytes >= 12 * 1024 * 1024 * 1024);
    if (*resource_mode == ImportResourceMode::Saver || low_memory)
        && usable(&ImportAsrProfile::Fast)
    {
        ImportAsrProfile::Fast
    } else if *resource_mode == ImportResourceMode::Performance
        && high_memory
        && usable(&ImportAsrProfile::Accurate)
    {
        ImportAsrProfile::Accurate
    } else if usable(&ImportAsrProfile::Balanced) {
        ImportAsrProfile::Balanced
    } else if usable(&ImportAsrProfile::Fast) {
        ImportAsrProfile::Fast
    } else if usable(&ImportAsrProfile::Accurate) {
        ImportAsrProfile::Accurate
    } else {
        ImportAsrProfile::Balanced
    }
}

fn local_wav_duration_seconds(item: &ImportItem) -> Option<u64> {
    if item.input.kind != ImportInputKind::File {
        return None;
    }
    let path = Path::new(&item.input.locator);
    if !path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("wav"))
    {
        return None;
    }
    let mut file = fs::File::open(path).ok()?;
    let mut bytes = vec![0_u8; 1024 * 1024];
    let read = file.read(&mut bytes).ok()?;
    bytes.truncate(read);
    wav_duration_seconds(&bytes)
}

fn wav_duration_seconds(bytes: &[u8]) -> Option<u64> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut offset: usize = 12;
    let mut bytes_per_second = None;
    let mut data_bytes = None;
    while offset.checked_add(8)? <= bytes.len() {
        let chunk = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().ok()?) as usize;
        let payload = offset.checked_add(8)?;
        if chunk == b"fmt " && size >= 12 && payload.checked_add(12)? <= bytes.len() {
            bytes_per_second =
                Some(u32::from_le_bytes(bytes[payload + 8..payload + 12].try_into().ok()?) as u64);
        } else if chunk == b"data" {
            data_bytes = Some(size as u64);
        }
        if bytes_per_second.is_some() && data_bytes.is_some() {
            break;
        }
        offset = payload.checked_add(size)?.checked_add(size % 2)?;
    }
    let rate = bytes_per_second.filter(|rate| *rate > 0)?;
    Some(data_bytes?.div_ceil(rate).max(1))
}

#[cfg(windows)]
fn available_memory_bytes() -> Option<u64> {
    use std::mem::{size_of, zeroed};
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let mut status: MEMORYSTATUSEX = unsafe { zeroed() };
    status.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
    (unsafe { GlobalMemoryStatusEx(&mut status) } != 0).then_some(status.ullAvailPhys)
}

#[cfg(unix)]
fn available_memory_bytes() -> Option<u64> {
    let pages = unsafe { libc::sysconf(libc::_SC_AVPHYS_PAGES) };
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    (pages > 0 && page_size > 0).then(|| (pages as u64).saturating_mul(page_size as u64))
}

#[cfg(not(any(windows, unix)))]
fn available_memory_bytes() -> Option<u64> {
    None
}

#[cfg(windows)]
fn available_disk_bytes(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut available = 0_u64;
    (unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } != 0)
        .then_some(available)
}

#[cfg(unix)]
fn available_disk_bytes(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::mem::zeroed;
    use std::os::unix::ffi::OsStrExt;

    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stats: libc::statvfs = unsafe { zeroed() };
    (unsafe { libc::statvfs(path.as_ptr(), &mut stats) } == 0)
        .then(|| (stats.f_bavail as u64).saturating_mul(stats.f_frsize as u64))
}

#[cfg(not(any(windows, unix)))]
fn available_disk_bytes(_: &Path) -> Option<u64> {
    None
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

    #[test]
    fn workbench_preferences_are_versioned_and_bounded() {
        let preferences = ImportWorkbenchPreferences {
            active_section: crate::models::import_v2_presentation::ImportWorkbenchSection::History,
            queue_filter: crate::models::import_v2_presentation::ImportQueuePreference::NeedsAction,
            workbench_scroll_top: 10,
            capabilities_scroll_top: 20,
            history_scroll_top: 30,
            source_methods_expanded: false,
            ..ImportWorkbenchPreferences::default()
        };
        validate_workbench_preferences(&preferences).unwrap();
        let value = serde_json::to_value(&preferences).unwrap();
        assert_eq!(value["activeSection"], "history");
        assert_eq!(value["queueFilter"], "needs_action");
        assert_eq!(value["sourceMethodsExpanded"], false);

        let invalid_version = ImportWorkbenchPreferences {
            schema_version: 2,
            ..preferences.clone()
        };
        assert_eq!(
            validate_workbench_preferences(&invalid_version)
                .unwrap_err()
                .code,
            "IMPORT_V2_WORKBENCH_PREFERENCES_INVALID"
        );
        let invalid_scroll = ImportWorkbenchPreferences {
            history_scroll_top: 10_000_001,
            ..preferences
        };
        assert_eq!(
            validate_workbench_preferences(&invalid_scroll)
                .unwrap_err()
                .code,
            "IMPORT_V2_WORKBENCH_PREFERENCES_INVALID"
        );
    }

    #[test]
    fn bounded_preview_markdown_preserves_a_cjk_character_boundary() {
        let repeated = "界".repeat((IMPORT_V2_PREVIEW_MAX_BYTES as usize / 3) + 2);
        let bounded = bounded_preview_markdown(repeated.into_bytes()).unwrap();
        assert!(bounded.len() <= IMPORT_V2_PREVIEW_MAX_BYTES as usize);
        assert!(bounded.ends_with('界'));
    }

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
            completion: None,
        }
    }

    fn item(id: &str, committed: bool, error_code: Option<&str>) -> ImportItemCommitResult {
        ImportItemCommitResult {
            item_id: id.into(),
            source_id: committed.then(|| "source-1".into()),
            version_id: committed.then(|| "version-1".into()),
            wiki_path: committed.then(|| "wiki/item.md".into()),
            content_hash: None,
            disposition: None,
            warnings: Vec::new(),
            committed,
            error_code: error_code.map(str::to_string),
        }
    }

    fn asr_profile_plan(
        profile: ImportAsrProfile,
        available: bool,
        installable: bool,
        installed_bytes: Option<u64>,
    ) -> ImportAsrProfilePlan {
        ImportAsrProfilePlan {
            profile,
            capability_id: "asr-fixture".into(),
            engine_name: "fixture".into(),
            model_name: "fixture".into(),
            available,
            installable,
            download_bytes: None,
            installed_bytes,
            model_bytes: None,
            device: "cpu".into(),
            estimated_seconds: None,
            unavailable_reason_code: None,
            dependencies: Vec::new(),
        }
    }

    #[test]
    fn wav_duration_uses_declared_byte_rate_and_rounds_up() {
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&32036_u32.to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&16_000_u32.to_le_bytes());
        wav.extend_from_slice(&16_000_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&8_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&32_000_u32.to_le_bytes());
        wav.resize(wav.len() + 32_000, 0);

        assert_eq!(wav_duration_seconds(&wav), Some(2));
        assert_eq!(wav_duration_seconds(b"not-wave"), None);
    }

    #[test]
    fn asr_recommendation_respects_resources_and_real_profile_usability() {
        let profiles = vec![
            asr_profile_plan(ImportAsrProfile::Fast, true, false, Some(1)),
            asr_profile_plan(ImportAsrProfile::Balanced, true, false, Some(1)),
            asr_profile_plan(ImportAsrProfile::Accurate, true, false, Some(1)),
        ];
        assert_eq!(
            recommend_asr_profile(
                &profiles,
                &ImportResourceMode::Performance,
                Some(4 * 1024 * 1024 * 1024),
                Some(1024),
            ),
            ImportAsrProfile::Fast
        );
        assert_eq!(
            recommend_asr_profile(
                &profiles,
                &ImportResourceMode::Performance,
                Some(16 * 1024 * 1024 * 1024),
                Some(1024),
            ),
            ImportAsrProfile::Accurate
        );
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
        let routes = vec![
            "web.generic.readability".into(),
            "web.wechat.article".into(),
        ];
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

    #[test]
    fn ability_readiness_reports_native_subtitles_and_runtime_routes() {
        let routes = vec!["ocr.basic".into()];
        let capabilities = vec![CapabilityRuntimeStatus {
            capability_id: "asr-sensevoice-small".into(),
            route: "media.asr".into(),
            available: true,
            reason: None,
        }];
        let abilities = ability_readiness(&routes, &capabilities);

        assert!(
            abilities
                .iter()
                .find(|item| item.id == "subtitle")
                .unwrap()
                .available
        );
        assert!(
            abilities
                .iter()
                .find(|item| item.id == "local_asr")
                .unwrap()
                .available
        );
        assert!(
            abilities
                .iter()
                .find(|item| item.id == "ocr")
                .unwrap()
                .available
        );
        let keyframes = abilities
            .iter()
            .find(|item| item.id == "keyframes")
            .unwrap();
        assert!(!keyframes.available);
        assert_eq!(keyframes.reason_code.as_deref(), Some("phase_two"));
    }

    #[test]
    fn file_readiness_exposes_the_complete_batch_three_format_matrix() {
        let routes = vec![
            "file.native".into(),
            "file.csv-package".into(),
            "office.modern.docx".into(),
            "office.modern.xlsx".into(),
            "office.modern.pptx".into(),
            "pdf.text".into(),
            "media.companion".into(),
            "media.subtitle".into(),
        ];
        let files = file_readiness(&routes, &[]);
        let ids = files
            .iter()
            .map(|file| file.id.as_str())
            .collect::<std::collections::HashSet<_>>();

        for expected in [
            "doc", "docx", "xls", "xlsx", "ppt", "pptx", "pdf", "md", "txt", "html", "csv", "png",
            "jpeg", "webp", "bmp", "tiff", "heic", "heif", "gif", "mp3", "wav", "m4a", "aac",
            "flac", "ogg", "opus", "wma", "mp4", "mov", "mkv", "webm", "avi", "m4v", "wmv", "srt",
            "vtt", "ass", "lrc",
        ] {
            assert!(ids.contains(expected), "missing readiness row: {expected}");
        }
        assert!(
            files
                .iter()
                .find(|file| file.id == "csv")
                .unwrap()
                .available
        );
        assert!(
            !files
                .iter()
                .find(|file| file.id == "lrc")
                .unwrap()
                .available
        );
        assert!(
            !files
                .iter()
                .find(|file| file.id == "png")
                .unwrap()
                .available
        );
    }
}
