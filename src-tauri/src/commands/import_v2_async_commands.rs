use tauri::{AppHandle, Manager};

use super::import_v2_activation::{
    self as activation, ActivateImportV2Request, GetImportBackendActivationRequest,
};
use super::import_v2_agent_commands as agent;
use super::import_v2_commands as core;
use super::import_v2_file_commands as file;
use super::import_v2_migration as migration;
use super::import_v2_presentation_commands as presentation;
use super::import_v2_web_commands as web;
use crate::app_state::AppState;
use crate::commands::import_v2_commands::{
    AddImportItemsV2Request, AddImportTextV2Request, CancelImportBatchV2Request,
    CancelImportItemV2Request, CreateImportSessionV2Request,
    GetImportRestrictedContentStatusV2Request, GetImportSessionV2Request,
    ImportMergeContextV2Request, ImportRestrictedContentStatus, ListImportSessionItemsV2Request,
    SelectImportSubtitleV2Request, SetImportItemResolutionV2Request,
    SetImportItemSelectionV2Request, StageImportManualMergeV2Request, StartImportBatchV2Request,
    StartImportItemsV2Request,
};
use crate::commands::import_v2_file_commands::{
    AcceptImportScanV2Request, AcceptImportScanV2Result, AddImportPathsV2Request,
    DiscardImportScanV2Request, GetImportScanResultV2Request,
};
use crate::commands::import_v2_migration::{
    ApplyImportV2MigrationRequest, GetImportV2MigrationStatusRequest, PlanImportV2MigrationRequest,
    ResumeImportV2MigrationRequest, ScanImportV2MigrationRequest,
};
use crate::commands::import_v2_web_commands::{
    AuthorizePrivateTargetRequest, CompleteLoginRequest, CompleteLoginResult, LoginRequest,
    RevokeRequest,
};
use crate::commands::runtime::run_blocking;
use crate::errors::BackendError;
use crate::models::import_backend_activation::{ActivationResult, ImportBackendActivation};
use crate::models::import_v2::{
    CommitImportSessionRequest, ImportCompletion, ImportItem, ImportItemPage, ImportSession,
    ImportSessionOverview, ImportThreeWayMergeContext,
};
use crate::models::import_v2_agent::{
    AcceptImportAgentCandidateRequest, AgentCandidateActionResult, AgentCandidateView,
    AgentInvocationRequest, DiscardImportAgentCandidateRequest, SelectImportAgentCandidateRequest,
};
use crate::models::import_v2_file::FileScanResult;
use crate::models::import_v2_migration::{
    LegacyInventory, MigrationPreparation, MigrationStatusSnapshot,
};
use crate::models::import_v2_presentation::{
    GetImportAsrEnablementPlanV2Request, GetImportCapabilityRequirementV2Request,
    GetImportFrontendReadinessV2Request, GetImportHistoryDetailV2Request,
    GetImportPreviewContentV2Request, ImportAsrEnablementPlan, ImportCapabilityRequirement,
    ImportFrontendReadiness, ImportHistoryDetailPage, ImportHistoryPage, ImportPreviewContent,
    ImportWorkbenchPreferences, ImportWorkbenchPreferencesRequest,
    InstallImportCapabilityV2Request, ListImportHistoryV2Request,
    RebuildImportHistoryIndexV2Request, SaveImportWorkbenchPreferencesRequest,
};
use crate::models::import_v2_web::{
    AddImportCollectionItemsV2Request, AddImportUrlV2Request, AuthorizeBilibiliAsrV2Request,
    AuthorizeLocalAsrV2Request, AuthorizeLocalOcrV2Request, ConfirmRemoteMediaRetentionV2Request,
    DiscoverImportCollectionV2Request, ImportCollectionPage, ImportCollectionPreview,
    LoadImportCollectionPageV2Request, RemoteMediaRetentionPlan, RemoteMediaRetentionRequest,
};
use crate::models::task::BackendTask;
use crate::services::import_v2::capability_runtime::CapabilityRuntimeStatus;
use crate::services::import_v2::connector_session::ConnectorSessionRef;
use crate::services::BlockingWorkClass;

macro_rules! blocking {
    ($app:expr, $class:expr, |$worker_app:ident, $state:ident| $body:expr) => {{
        run_blocking($app, $class, move |$worker_app| {
            let $state = $worker_app.state::<AppState>();
            $body
        })
        .await
    }};
}

#[tauri::command]
pub async fn activate_import_v2(
    app: AppHandle,
    request: ActivateImportV2Request,
) -> Result<ActivationResult, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        activation::activate_import_v2(state, request)
    })
}

#[tauri::command]
pub async fn get_import_backend_activation(
    app: AppHandle,
    request: GetImportBackendActivationRequest,
) -> Result<Option<ImportBackendActivation>, BackendError> {
    blocking!(app, BlockingWorkClass::MetadataIo, |_app, state| {
        activation::get_import_backend_activation(state, request)
    })
}

#[tauri::command]
pub async fn accept_import_agent_candidate_v2(
    app: AppHandle,
    request: AcceptImportAgentCandidateRequest,
) -> Result<AgentCandidateView, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        agent::accept_import_agent_candidate_v2(state, request)
    })
}

#[tauri::command]
pub async fn select_import_agent_candidate_v2(
    app: AppHandle,
    request: SelectImportAgentCandidateRequest,
) -> Result<AgentCandidateActionResult, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        agent::select_import_agent_candidate_v2(state, request)
    })
}

#[tauri::command]
pub async fn discard_import_agent_candidate_v2(
    app: AppHandle,
    request: DiscardImportAgentCandidateRequest,
) -> Result<AgentCandidateActionResult, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        agent::discard_import_agent_candidate_v2(state, request)
    })
}

#[tauri::command]
pub async fn start_import_agent_assistance_v2(
    app: AppHandle,
    request: AgentInvocationRequest,
) -> Result<BackendTask, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |worker_app, state| {
        agent::start_import_agent_assistance_v2(worker_app.clone(), state, request)
    })
}

#[tauri::command]
pub async fn get_import_restricted_content_status_v2(
    app: AppHandle,
    request: GetImportRestrictedContentStatusV2Request,
) -> Result<ImportRestrictedContentStatus, BackendError> {
    blocking!(app, BlockingWorkClass::MetadataIo, |_app, state| {
        core::get_import_restricted_content_status_v2(state, request)
    })
}

#[tauri::command]
pub async fn create_import_session_v2(
    app: AppHandle,
    request: CreateImportSessionV2Request,
) -> Result<ImportSession, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        core::create_import_session_v2(state, request)
    })
}

#[tauri::command]
pub async fn get_import_session_v2(
    app: AppHandle,
    request: GetImportSessionV2Request,
) -> Result<ImportSession, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        core::get_import_session_v2(state, request)
    })
}

#[tauri::command]
pub async fn get_import_session_overview_v2(
    app: AppHandle,
    request: GetImportSessionV2Request,
) -> Result<ImportSessionOverview, BackendError> {
    blocking!(app, BlockingWorkClass::MetadataIo, |_app, state| {
        core::get_import_session_overview_v2(state, request)
    })
}

#[tauri::command]
pub async fn list_import_session_items_v2(
    app: AppHandle,
    request: ListImportSessionItemsV2Request,
) -> Result<ImportItemPage, BackendError> {
    blocking!(app, BlockingWorkClass::MetadataIo, |_app, state| {
        core::list_import_session_items_v2(state, request)
    })
}

#[tauri::command]
pub async fn start_import_session_recovery_v2(
    app: AppHandle,
    request: GetImportSessionV2Request,
) -> Result<BackendTask, BackendError> {
    blocking!(app, BlockingWorkClass::MetadataIo, |worker_app, state| {
        core::start_import_session_recovery_v2(worker_app.clone(), state, request)
    })
}

#[tauri::command]
pub async fn add_import_items_v2(
    app: AppHandle,
    request: AddImportItemsV2Request,
) -> Result<ImportSession, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        core::add_import_items_v2(state, request)
    })
}

#[tauri::command]
pub async fn add_import_text_v2(
    app: AppHandle,
    request: AddImportTextV2Request,
) -> Result<ImportSession, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        core::add_import_text_v2(state, request)
    })
}

#[tauri::command]
pub async fn set_import_item_selection_v2(
    app: AppHandle,
    request: SetImportItemSelectionV2Request,
) -> Result<ImportSession, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        core::set_import_item_selection_v2(state, request)
    })
}

#[tauri::command]
pub async fn select_import_subtitle_v2(
    app: AppHandle,
    request: SelectImportSubtitleV2Request,
) -> Result<ImportSession, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        core::select_import_subtitle_v2(state, request)
    })
}

#[tauri::command]
pub async fn get_import_merge_context_v2(
    app: AppHandle,
    request: ImportMergeContextV2Request,
) -> Result<ImportThreeWayMergeContext, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        core::get_import_merge_context_v2(state, request)
    })
}

#[tauri::command]
pub async fn set_import_item_resolution_v2(
    app: AppHandle,
    request: SetImportItemResolutionV2Request,
) -> Result<ImportItem, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        core::set_import_item_resolution_v2(state, request)
    })
}

#[tauri::command]
pub async fn stage_import_manual_merge_v2(
    app: AppHandle,
    request: StageImportManualMergeV2Request,
) -> Result<ImportItem, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        core::stage_import_manual_merge_v2(state, request)
    })
}

#[tauri::command]
pub async fn get_import_history_session_v2(
    app: AppHandle,
    request: GetImportSessionV2Request,
) -> Result<ImportSession, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        core::get_import_history_session_v2(state, request)
    })
}

#[tauri::command]
pub async fn get_import_completion_v2(
    app: AppHandle,
    request: GetImportSessionV2Request,
) -> Result<Option<ImportCompletion>, BackendError> {
    blocking!(app, BlockingWorkClass::MetadataIo, |_app, state| {
        core::get_import_completion_v2(state, request)
    })
}

#[tauri::command]
pub async fn cancel_import_item_v2(
    app: AppHandle,
    request: CancelImportItemV2Request,
) -> Result<ImportSession, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        core::cancel_import_item_v2(state, request)
    })
}

#[tauri::command]
pub async fn skip_import_item_v2(
    app: AppHandle,
    request: CancelImportItemV2Request,
) -> Result<ImportSession, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        core::skip_import_item_v2(state, request)
    })
}

#[tauri::command]
pub async fn start_import_items_v2(
    app: AppHandle,
    request: StartImportItemsV2Request,
) -> Result<Vec<BackendTask>, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |worker_app, state| {
        core::start_import_items_v2(worker_app.clone(), state, request)
    })
}

#[tauri::command]
pub async fn start_import_batch_v2(
    app: AppHandle,
    request: StartImportBatchV2Request,
) -> Result<BackendTask, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |worker_app, state| {
        core::start_import_batch_v2(worker_app.clone(), state, request)
    })
}

#[tauri::command]
pub async fn cancel_import_batch_v2(
    app: AppHandle,
    request: CancelImportBatchV2Request,
) -> Result<Vec<BackendTask>, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        core::cancel_import_batch_v2(state, request)
    })
}

#[tauri::command]
pub async fn confirm_import_session_v2(
    app: AppHandle,
    request: CommitImportSessionRequest,
) -> Result<BackendTask, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |worker_app, state| {
        core::confirm_import_session_v2(worker_app.clone(), state, request)
    })
}

#[tauri::command]
pub async fn get_import_capability_statuses(
    app: AppHandle,
) -> Result<Vec<CapabilityRuntimeStatus>, BackendError> {
    blocking!(app, BlockingWorkClass::MetadataIo, |_app, state| Ok(
        file::get_import_capability_statuses(state)
    ))
}

#[tauri::command]
pub async fn start_add_import_paths_v2(
    app: AppHandle,
    request: AddImportPathsV2Request,
) -> Result<BackendTask, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |worker_app, _state| {
        file::start_add_import_paths_v2(worker_app.clone(), request)
    })
}

#[tauri::command]
pub async fn get_import_scan_result_v2(
    app: AppHandle,
    request: GetImportScanResultV2Request,
) -> Result<FileScanResult, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        file::get_import_scan_result_v2(state, request)
    })
}

#[tauri::command]
pub async fn accept_import_scan_v2(
    app: AppHandle,
    request: AcceptImportScanV2Request,
) -> Result<AcceptImportScanV2Result, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        file::accept_import_scan_v2(state, request)
    })
}

#[tauri::command]
pub async fn discard_import_scan_v2(
    app: AppHandle,
    request: DiscardImportScanV2Request,
) -> Result<FileScanResult, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        file::discard_import_scan_v2(state, request)
    })
}

#[tauri::command]
pub async fn scan_import_v2_migration(
    app: AppHandle,
    request: ScanImportV2MigrationRequest,
) -> Result<LegacyInventory, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        migration::scan_import_v2_migration(state, request)
    })
}

#[tauri::command]
pub async fn plan_import_v2_migration(
    app: AppHandle,
    request: PlanImportV2MigrationRequest,
) -> Result<MigrationPreparation, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        migration::plan_import_v2_migration(state, request)
    })
}

#[tauri::command]
pub async fn get_import_v2_migration_status(
    app: AppHandle,
    request: GetImportV2MigrationStatusRequest,
) -> Result<MigrationStatusSnapshot, BackendError> {
    blocking!(app, BlockingWorkClass::MetadataIo, |_app, state| {
        migration::get_import_v2_migration_status(state, request)
    })
}

#[tauri::command]
pub async fn apply_import_v2_migration(
    app: AppHandle,
    request: ApplyImportV2MigrationRequest,
) -> Result<BackendTask, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |worker_app, state| {
        migration::apply_import_v2_migration(worker_app.clone(), state, request)
    })
}

#[tauri::command]
pub async fn resume_import_v2_migration(
    app: AppHandle,
    request: ResumeImportV2MigrationRequest,
) -> Result<BackendTask, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |worker_app, state| {
        migration::resume_import_v2_migration(worker_app.clone(), state, request)
    })
}

#[tauri::command]
pub async fn get_import_workbench_preferences_v2(
    app: AppHandle,
    request: ImportWorkbenchPreferencesRequest,
) -> Result<ImportWorkbenchPreferences, BackendError> {
    blocking!(app, BlockingWorkClass::MetadataIo, |_app, state| {
        presentation::get_import_workbench_preferences_v2(state, request)
    })
}

#[tauri::command]
pub async fn save_import_workbench_preferences_v2(
    app: AppHandle,
    request: SaveImportWorkbenchPreferencesRequest,
) -> Result<ImportWorkbenchPreferences, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        presentation::save_import_workbench_preferences_v2(state, request)
    })
}

#[tauri::command]
pub async fn get_import_preview_content_v2(
    app: AppHandle,
    request: GetImportPreviewContentV2Request,
) -> Result<ImportPreviewContent, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        presentation::get_import_preview_content_v2(state, request)
    })
}

#[tauri::command]
pub async fn get_import_frontend_readiness_v2(
    app: AppHandle,
    request: GetImportFrontendReadinessV2Request,
) -> Result<ImportFrontendReadiness, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        presentation::get_import_frontend_readiness_v2(state, request)
    })
}

#[tauri::command]
pub async fn list_import_history_v2(
    app: AppHandle,
    request: ListImportHistoryV2Request,
) -> Result<ImportHistoryPage, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        presentation::list_import_history_v2(state, request)
    })
}

#[tauri::command]
pub async fn get_import_history_detail_v2(
    app: AppHandle,
    request: GetImportHistoryDetailV2Request,
) -> Result<ImportHistoryDetailPage, BackendError> {
    blocking!(app, BlockingWorkClass::MetadataIo, |_app, state| {
        presentation::get_import_history_detail_v2(state, request)
    })
}

#[tauri::command]
pub async fn rebuild_import_history_index_v2(
    app: AppHandle,
    request: RebuildImportHistoryIndexV2Request,
) -> Result<BackendTask, BackendError> {
    blocking!(app, BlockingWorkClass::MetadataIo, |worker_app, state| {
        presentation::rebuild_import_history_index_v2(worker_app.clone(), state, request)
    })
}

#[tauri::command]
pub async fn get_import_capability_requirement_v2(
    app: AppHandle,
    request: GetImportCapabilityRequirementV2Request,
) -> Result<ImportCapabilityRequirement, BackendError> {
    blocking!(app, BlockingWorkClass::MetadataIo, |_app, state| {
        presentation::get_import_capability_requirement_v2(state, request)
    })
}

#[tauri::command]
pub async fn get_import_asr_enablement_plan_v2(
    app: AppHandle,
    request: GetImportAsrEnablementPlanV2Request,
) -> Result<ImportAsrEnablementPlan, BackendError> {
    blocking!(app, BlockingWorkClass::MetadataIo, |_app, state| {
        presentation::get_import_asr_enablement_plan_v2(state, request)
    })
}

#[tauri::command]
pub async fn install_import_capability_v2(
    app: AppHandle,
    request: InstallImportCapabilityV2Request,
) -> Result<BackendTask, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |worker_app, state| {
        presentation::install_import_capability_v2(worker_app.clone(), state, request)
    })
}

#[tauri::command]
pub async fn add_import_url_v2(
    app: AppHandle,
    request: AddImportUrlV2Request,
) -> Result<ImportSession, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        web::add_import_url_v2(state, request)
    })
}

#[tauri::command]
pub async fn discover_import_collection_v2(
    app: AppHandle,
    request: DiscoverImportCollectionV2Request,
) -> Result<Option<ImportCollectionPreview>, BackendError> {
    web::discover_import_collection_v2(app, request).await
}

#[tauri::command]
pub async fn load_import_collection_page_v2(
    app: AppHandle,
    request: LoadImportCollectionPageV2Request,
) -> Result<ImportCollectionPage, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        web::load_import_collection_page_v2(state, request)
    })
}

#[tauri::command]
pub async fn add_import_collection_items_v2(
    app: AppHandle,
    request: AddImportCollectionItemsV2Request,
) -> Result<ImportSession, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        web::add_import_collection_items_v2(state, request)
    })
}

#[tauri::command]
pub async fn get_remote_media_retention_plan_v2(
    app: AppHandle,
    request: RemoteMediaRetentionRequest,
) -> Result<RemoteMediaRetentionPlan, BackendError> {
    blocking!(app, BlockingWorkClass::MetadataIo, |_app, state| {
        web::get_remote_media_retention_plan_v2(state, request)
    })
}

#[tauri::command]
pub async fn confirm_remote_media_retention_v2(
    app: AppHandle,
    request: ConfirmRemoteMediaRetentionV2Request,
) -> Result<ImportSession, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        web::confirm_remote_media_retention_v2(state, request)
    })
}

#[tauri::command]
pub async fn begin_import_login_v2(
    app: AppHandle,
    request: LoginRequest,
) -> Result<ConnectorSessionRef, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |worker_app, state| {
        web::begin_import_login_v2(worker_app.clone(), state, request)
    })
}

#[tauri::command]
pub async fn revoke_import_login_v2(
    app: AppHandle,
    request: RevokeRequest,
) -> Result<(), BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |worker_app, state| {
        web::revoke_import_login_v2(worker_app.clone(), state, request)
    })
}

#[tauri::command]
pub async fn complete_import_login_v2(
    app: AppHandle,
    request: CompleteLoginRequest,
) -> Result<CompleteLoginResult, BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |worker_app, state| {
        web::complete_import_login_v2(worker_app.clone(), state, request)
    })
}

#[tauri::command]
pub async fn authorize_import_private_target_v2(
    app: AppHandle,
    request: AuthorizePrivateTargetRequest,
) -> Result<String, BackendError> {
    web::authorize_import_private_target_v2(app, request).await
}

#[tauri::command]
pub async fn authorize_local_asr_v2(
    app: AppHandle,
    request: AuthorizeLocalAsrV2Request,
) -> Result<(), BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        web::authorize_local_asr_v2(state, request)
    })
}

#[tauri::command]
pub async fn authorize_local_ocr_v2(
    app: AppHandle,
    request: AuthorizeLocalOcrV2Request,
) -> Result<(), BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        web::authorize_local_ocr_v2(state, request)
    })
}

#[tauri::command]
pub async fn authorize_bilibili_asr_v2(
    app: AppHandle,
    request: AuthorizeBilibiliAsrV2Request,
) -> Result<(), BackendError> {
    blocking!(app, BlockingWorkClass::HeavyIo, |_app, state| {
        web::authorize_bilibili_asr_v2(state, request)
    })
}
