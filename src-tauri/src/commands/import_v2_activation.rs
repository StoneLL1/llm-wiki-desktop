use serde::{Deserialize, Serialize};
use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::import_backend_activation::{
    ActivationConfirmation, ActivationResult, ImportBackendActivation,
};
use crate::models::import_v2_migration::MigrationReport;
use crate::services::import_v2::activation::ImportV2ActivationService;
use crate::services::import_v2::migration::MigrationReadinessEvidence;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivateImportV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub report: MigrationReport,
    pub readiness: MigrationReadinessEvidence,
    pub release_version: String,
    pub confirmation: ActivationConfirmation,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetImportBackendActivationRequest {
    pub project_id: String,
    pub project_root_path: String,
}

#[tauri::command]
pub fn activate_import_v2(
    state: State<'_, AppState>,
    request: ActivateImportV2Request,
) -> Result<ActivationResult, BackendError> {
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, context| {
            ImportV2ActivationService::default().activate(
                &state.import_v2_service,
                &state.git_service,
                context,
                &request.report,
                &request.readiness,
                &request.release_version,
                request.confirmation,
            )
        },
    )
}

#[tauri::command]
pub fn get_import_backend_activation(
    state: State<'_, AppState>,
    request: GetImportBackendActivationRequest,
) -> Result<Option<ImportBackendActivation>, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    ImportV2ActivationService::default().read(&context)
}
