use serde::Deserialize;
use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::workflow::{
    WorkflowKind, WorkflowPreparation, WorkflowRouteSelection, WorkflowScope, WorkflowStartOutcome,
    WorkflowsOverview,
};
use crate::services::{
    PrepareWorkflowInput, WorkflowAccessSnapshot, WorkflowPreparationEnvironment,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowProjectRequest {
    pub project_id: String,
    pub project_root_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareWorkflowRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub kind: WorkflowKind,
    pub scope: Option<WorkflowScope>,
    pub route_selection: Option<WorkflowRouteSelection>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartWorkflowRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub preparation_id: String,
    pub preparation_revision: String,
}

#[tauri::command]
pub fn get_workflows_overview(
    state: State<'_, AppState>,
    request: WorkflowProjectRequest,
) -> Result<WorkflowsOverview, BackendError> {
    if request.project_id.trim().is_empty() && request.project_root_path.trim().is_empty() {
        return Ok(state.workflow_service.no_project_overview());
    }
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let access = WorkflowAccessSnapshot::legacy_fail_closed(&context, &state.git_service)?;
    state.workflow_service.project_overview(
        &context,
        access,
        &state.settings_service,
        &state.secret_service,
        &state.agent_service,
        &state.task_service,
    )
}

#[tauri::command]
pub fn prepare_workflow(
    state: State<'_, AppState>,
    request: PrepareWorkflowRequest,
) -> Result<WorkflowPreparation, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let access = WorkflowAccessSnapshot::legacy_fail_closed(&context, &state.git_service)?;
    state.workflow_service.prepare(
        &WorkflowPreparationEnvironment {
            context: &context,
            access,
            settings_service: &state.settings_service,
            secret_service: &state.secret_service,
            agent_service: &state.agent_service,
        },
        PrepareWorkflowInput {
            kind: request.kind,
            scope: request.scope,
            route_selection: request.route_selection,
        },
    )
}

#[tauri::command]
pub fn start_workflow(
    state: State<'_, AppState>,
    request: StartWorkflowRequest,
) -> Result<WorkflowStartOutcome, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let access = WorkflowAccessSnapshot::legacy_fail_closed(&context, &state.git_service)?;
    state.workflow_service.start(
        &context,
        access,
        &state.settings_service,
        &state.secret_service,
        &state.agent_service,
        &state.task_service,
        &request.preparation_id,
        &request.preparation_revision,
    )
}
