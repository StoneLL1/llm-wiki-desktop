use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::import_v2::{ImportItem, ImportItemStatus, ImportRecoveryAction};
use crate::models::import_v2_file::CapabilityRequirement;
use crate::models::import_v2_migration::{LegacyHistoryView, MigrationStatus};
use crate::models::import_v2_presentation::{
    GetImportCapabilityRequirementV2Request, GetImportFrontendReadinessV2Request,
    GetImportPreviewContentV2Request, ImportCapabilityRequirement, ImportFrontendReadiness,
    ImportHistoryPage, ImportPreviewContent, InstallImportCapabilityV2Request,
    ListImportHistoryV2Request, IMPORT_V2_PREVIEW_MAX_BYTES,
};
use crate::models::task::BackendTask;
use crate::services::import_v2::activation::ImportV2ActivationService;
use crate::services::import_v2::migration::{
    LegacyHistoryAdapter, MigrationService, REQUIRED_IMPORT_V2_CONTRACT,
};

#[tauri::command]
pub fn get_import_preview_content_v2(
    state: State<'_, AppState>,
    request: GetImportPreviewContentV2Request,
) -> Result<ImportPreviewContent, BackendError> {
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
    Ok(ImportFrontendReadiness {
        backend_version: REQUIRED_IMPORT_V2_CONTRACT.into(),
        active,
        migration_status: migration.status,
        unfinished_session_id: state
            .import_v2_service
            .find_unfinished_session(&context, &state.file_store)?,
        legacy_history_available: !legacy_history.entries.is_empty(),
    })
}

#[tauri::command]
pub fn list_import_history_v2(
    state: State<'_, AppState>,
    request: ListImportHistoryV2Request,
) -> Result<ImportHistoryPage, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let LegacyHistoryView { entries, warnings } =
        LegacyHistoryAdapter::default().list(&context.root)?;
    let start = parse_cursor(request.cursor.as_deref(), entries.len())?;
    let limit = request.limit.unwrap_or(50).clamp(1, 50) as usize;
    let end = start.saturating_add(limit).min(entries.len());
    let next_cursor = (end < entries.len()).then(|| end.to_string());
    Ok(ImportHistoryPage {
        entries: Vec::new(),
        legacy_read_only: entries[start..end].to_vec(),
        next_cursor,
        warnings,
    })
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
    let mut file = fs::File::open(&path).map_err(|_| {
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

fn parse_cursor(value: Option<&str>, length: usize) -> Result<usize, BackendError> {
    let Some(value) = value else { return Ok(0) };
    let parsed = value.parse::<usize>().map_err(|_| {
        presentation_error(
            "IMPORT_V2_HISTORY_CURSOR_INVALID",
            "History cursor is invalid.",
        )
    })?;
    if parsed > length {
        return Err(presentation_error(
            "IMPORT_V2_HISTORY_CURSOR_INVALID",
            "History cursor is out of range.",
        ));
    }
    Ok(parsed)
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

fn presentation_error(code: &'static str, message: &'static str) -> BackendError {
    BackendError::new(code, message, true, true)
}
