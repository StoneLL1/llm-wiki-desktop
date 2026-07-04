use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::confirmation::{
    ActionPreview, ConfirmationExecution, PendingAction, PendingActionType, RiskLevel,
};
use crate::models::git::CheckpointPurpose;
use crate::models::wiki::{
    CreateWikiPageRequest, DeleteWikiPageRequest, ReadWikiPageRequest, RenameWikiPageRequest,
    RenameWikiPageResponse, SaveWikiPageRequest, SaveWikiPageResponse, ToggleBookmarkRequest,
    ToggleBookmarkResponse, WikiPageContent, WikiTree,
};

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanWikiRequest {
    pub project_id: String,
    pub project_root_path: String,
}

#[tauri::command]
pub fn scan_wiki(
    state: State<'_, AppState>,
    request: ScanWikiRequest,
) -> Result<WikiTree, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let bookmark_paths = state.bookmark_service.wiki_page_paths(&context)?;
    state.search_service.scan_wiki(&context, &bookmark_paths)
}

#[tauri::command]
pub fn read_wiki_page(
    state: State<'_, AppState>,
    request: ReadWikiPageRequest,
) -> Result<WikiPageContent, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let bookmark_paths = state.bookmark_service.wiki_page_paths(&context)?;
    state
        .search_service
        .read_page(&context, &request.relative_path, &bookmark_paths)
}

#[tauri::command]
pub fn save_wiki_page(
    state: State<'_, AppState>,
    request: SaveWikiPageRequest,
) -> Result<SaveWikiPageResponse, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
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
