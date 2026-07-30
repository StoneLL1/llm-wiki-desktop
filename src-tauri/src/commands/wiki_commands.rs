use std::fs;

use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::confirmation::{
    ActionPreview, ConfirmationExecution, PendingAction, PendingActionType, RiskLevel,
};
use crate::models::git::CheckpointPurpose;
use crate::models::wiki::{
    CreateWikiPageRequest, DeleteWikiPageRequest, ReadWikiAssetRequest, ReadWikiPageRequest,
    RenameWikiPageRequest, RenameWikiPageResponse, SaveWikiPageRequest, SaveWikiPageResponse,
    ToggleBookmarkRequest, ToggleBookmarkResponse, WikiAssetContent, WikiPageContent, WikiTree,
};
use crate::services::import_v2::source_lifecycle::{
    apply_validated_page_binding, apply_validated_source_bindings, reject_generic_source_create,
    reject_generic_source_path,
};
use crate::services::import_v2::source_registry::SourceRegistry;

const MAX_WIKI_ASSET_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanWikiRequest {
    pub project_id: String,
    pub project_root_path: String,
}

#[tauri::command]
pub async fn scan_wiki(app: AppHandle, request: ScanWikiRequest) -> Result<WikiTree, BackendError> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let context =
            state.resolve_project_context(&request.project_id, &request.project_root_path)?;
        let bookmark_paths = state.bookmark_service.wiki_page_paths(&context)?;
        let mut tree = state.search_service.scan_wiki(&context, &bookmark_paths)?;
        apply_validated_source_bindings(&context, &state.file_store, &mut tree)?;
        Ok(tree)
    })
    .await
    .map_err(wiki_io_worker_failed)?
}

#[tauri::command]
pub async fn read_wiki_page(
    app: AppHandle,
    request: ReadWikiPageRequest,
) -> Result<WikiPageContent, BackendError> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let context =
            state.resolve_project_context(&request.project_id, &request.project_root_path)?;
        let bookmark_paths = state.bookmark_service.wiki_page_paths(&context)?;
        let mut page =
            state
                .search_service
                .read_page(&context, &request.relative_path, &bookmark_paths)?;
        apply_validated_page_binding(&context, &state.file_store, &mut page)?;
        Ok(page)
    })
    .await
    .map_err(wiki_io_worker_failed)?
}

/// Read an imported Wiki image through a project-scoped backend command.
///
/// Markdown stores the portable `assets/...` reference, while the source
/// registry maps that reference to the current immutable raw source version.
/// Returning bytes keeps arbitrary filesystem paths out of the renderer and
/// gives the command one place to enforce the asset size and path boundary.
#[tauri::command]
pub async fn read_wiki_asset(
    app: AppHandle,
    request: ReadWikiAssetRequest,
) -> Result<WikiAssetContent, BackendError> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let context =
            state.resolve_project_context(&request.project_id, &request.project_root_path)?;
        let path = SourceRegistry::resolve_wiki_asset_path(
            &context,
            &state.file_store,
            &request.page_path,
            &request.asset_path,
        )?;
        let size = fs::metadata(&path)
            .map_err(|error| wiki_asset_read_failed(&path, error))?
            .len();
        if size > MAX_WIKI_ASSET_BYTES as u64 {
            return Err(wiki_asset_too_large(size));
        }
        let bytes = fs::read(&path).map_err(|error| wiki_asset_read_failed(&path, error))?;
        if bytes.len() > MAX_WIKI_ASSET_BYTES {
            return Err(wiki_asset_too_large(bytes.len() as u64));
        }

        Ok(WikiAssetContent {
            content_type: wiki_asset_content_type(&path),
            bytes,
        })
    })
    .await
    .map_err(wiki_io_worker_failed)?
}

fn wiki_asset_read_failed(path: &std::path::Path, error: std::io::Error) -> BackendError {
    BackendError::new("WIKI_ASSET_READ_FAILED", error.to_string(), true, false)
        .with_details(serde_json::json!({ "path": path.to_string_lossy() }))
}

fn wiki_asset_too_large(size: u64) -> BackendError {
    BackendError::new(
        "WIKI_ASSET_TOO_LARGE",
        "The Wiki image asset is larger than the reader limit.",
        false,
        true,
    )
    .with_details(serde_json::json!({
        "size": size,
        "limit": MAX_WIKI_ASSET_BYTES,
    }))
}

fn wiki_io_worker_failed(error: impl std::fmt::Display) -> BackendError {
    BackendError::new(
        "WIKI_IO_WORKER_FAILED",
        format!("The Wiki I/O worker stopped unexpectedly: {error}"),
        true,
        false,
    )
}

fn wiki_asset_content_type(path: &std::path::Path) -> String {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        _ => "application/octet-stream",
    }
    .to_string()
}

#[tauri::command]
pub fn save_wiki_page(
    state: State<'_, AppState>,
    request: SaveWikiPageRequest,
) -> Result<SaveWikiPageResponse, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    if request.expected_hash.is_none() {
        reject_generic_source_create(&request.relative_path, None, Some(&request.contents))?;
    }
    state.search_service.save_page(
        &context,
        &request.relative_path,
        &request.contents,
        request.expected_hash,
    )
}

#[tauri::command]
pub fn toggle_bookmark(
    state: State<'_, AppState>,
    request: ToggleBookmarkRequest,
) -> Result<ToggleBookmarkResponse, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let bookmark_paths = state.bookmark_service.wiki_page_paths(&context)?;
    let page = state
        .search_service
        .read_page(&context, &request.relative_path, &bookmark_paths)?;
    state
        .bookmark_service
        .toggle_wiki_page(&context, &page.meta.path, &page.meta.title)
}

/// Create a new wiki page with seeded frontmatter + H1. Non-destructive (no Git
/// checkpoint); rejects existing paths. The path must resolve under `wiki/`.
#[tauri::command]
pub fn create_wiki_page(
    state: State<'_, AppState>,
    request: CreateWikiPageRequest,
) -> Result<SaveWikiPageResponse, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    reject_generic_source_create(&request.relative_path, request.page_type.as_deref(), None)?;
    state.search_service.create_page(&context, &request)
}

/// Rename a wiki page and rewrite every `[[old]]` reference across the wiki to
/// `[[new]]` (preserving aliases/anchors). A rename is a file move plus a batch
/// rewrite, which the CLAUDE.md hard boundary covers ("覆盖、批量替换 — 操作前
/// 必须创建 Git 检查点"); the checkpoint is created here before the service
/// performs the move so the old page and all reference files are recoverable.
#[tauri::command]
pub fn rename_wiki_page(
    state: State<'_, AppState>,
    request: RenameWikiPageRequest,
) -> Result<RenameWikiPageResponse, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    reject_generic_source_path(&context, &state.file_store, &request.relative_path)?;
    reject_generic_source_create(&request.new_relative_path, None, None)?;
    state.git_service.create_checkpoint(
        &context,
        CheckpointPurpose::HighRiskOperation,
        "Before renaming wiki page",
    )?;
    state
        .search_service
        .rename_page(&context, &request.relative_path, &request.new_relative_path)
}

/// Request deletion of a wiki page. Does not delete immediately: registers a
/// Destructive `PendingAction` (with a preview listing pages that link to it)
/// and returns it so the frontend can confirm via `confirm_pending_action`. The
/// actual delete + Git checkpoint happens on confirmation
/// (`file_commands::confirm_pending_action` → `DeleteWikiPage` execution).
#[tauri::command]
pub fn request_delete_wiki_page(
    state: State<'_, AppState>,
    request: DeleteWikiPageRequest,
) -> Result<PendingAction, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    reject_generic_source_path(&context, &state.file_store, &request.relative_path)?;
    let absolute = context.resolve_project_path(&request.relative_path)?;
    if absolute.strip_prefix(&context.wiki_dir).is_err() {
        return Err(BackendError::new(
            "PATH_OUTSIDE_PROJECT",
            "Only wiki pages can be deleted here.".to_string(),
            false,
            true,
        ));
    }
    if !absolute.exists() || !absolute.is_file() {
        return Err(BackendError::new(
            "FILE_NOT_FOUND",
            "Wiki page does not exist.".to_string(),
            false,
            true,
        )
        .with_details(serde_json::json!({ "path": request.relative_path })));
    }
    let target_hash = state
        .file_store
        .file_hash(&context, &request.relative_path)?;
    let target_stem = absolute
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let referenced_by = state
        .search_service
        .find_pages_referencing(&context, &target_stem)?;

    let summary = if referenced_by.is_empty() {
        format!("Delete {}.", request.relative_path)
    } else {
        format!(
            "Delete {}. {} page(s) link to it and will show as missing.",
            request.relative_path,
            referenced_by.len()
        )
    };
    let action = PendingAction {
        id: uuid::Uuid::new_v4().to_string(),
        action_type: PendingActionType::DeleteFile,
        title: "Delete wiki page".to_string(),
        message: "The wiki page will be deleted after a Git checkpoint. Pages linking to it will show as missing.".to_string(),
        risk_level: RiskLevel::Destructive,
        affected_paths: vec![request.relative_path.clone()],
        preview: Some(ActionPreview {
            summary,
            before: None,
            after: None,
            diff: None,
        }),
        expires_at: None,
        // The checkpoint is created only after the user confirms the deletion,
        // so there is no hash to surface yet (same semantics as DeleteSource).
        checkpoint_hash: None,
    };
    state.confirmation_registry.register_with_execution(
        action.clone(),
        Some(ConfirmationExecution::DeleteWikiPage {
            project_id: request.project_id,
            root_path: request.project_root_path,
            target_path: request.relative_path,
            target_hash,
        }),
    )?;
    Ok(action)
}
