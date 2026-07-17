use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::import_v2::{
    CommitImportSessionRequest, ImportInput, ImportRecoveryAction, ImportResourceMode,
    ImportSession,
};
use crate::models::import_v2_agent::AgentAssistanceTrigger;
use crate::models::task::{BackendTask, TaskResult, TaskResultReference, TaskStatus, TaskType};
use crate::models::paths::ProjectContext;
use crate::services::import_v2::agent_assistance::AgentAssistanceService;
use crate::services::import_v2::agent_candidate::AgentCandidateService;

macro_rules! request {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        #[derive(Debug, Deserialize, Serialize)]
        #[serde(rename_all = "camelCase")]
        pub struct $name { $(pub $field: $ty),* }
    };
}
request!(CreateImportSessionV2Request {
    project_id: String,
    project_root_path: String,
    resource_mode: ImportResourceMode
});
request!(GetImportSessionV2Request {
    project_id: String,
    project_root_path: String,
    session_id: String,
    history_batch_id: Option<String>
});
request!(AddImportItemsV2Request { project_id: String, project_root_path: String, session_id: String, inputs: Vec<ImportInput> });
request!(CancelImportItemV2Request {
    project_id: String,
    project_root_path: String,
    session_id: String,
    item_id: String
});
request!(CancelImportBatchV2Request {
    project_id: String,
    project_root_path: String,
    session_id: String,
    batch_id: String
});
request!(AddImportTextV2Request {
    project_id: String,
    project_root_path: String,
    session_id: String,
    source_name: String,
    content: String
});
request!(SetImportItemSelectionV2Request {
    project_id: String,
    project_root_path: String,
    session_id: String,
    item_id: String,
    selected: bool
});
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartImportItemsV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub item_ids: Vec<String>,
    #[serde(default)]
    pub recovery_action: Option<ImportRecoveryAction>,
}

#[tauri::command]
pub fn create_import_session_v2(
    state: State<'_, AppState>,
    request: CreateImportSessionV2Request,
) -> Result<ImportSession, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .import_v2_service
        .create_session(&context, &state.file_store, request.resource_mode)
}
#[tauri::command]
pub fn get_import_session_v2(
    state: State<'_, AppState>,
    request: GetImportSessionV2Request,
) -> Result<ImportSession, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.import_v2_service.recover_session(
        &context,
        &state.file_store,
        &state.task_service,
        &request.session_id,
    )?;
    AgentCandidateService::new(
        &state.import_v2_service,
        &state.file_store,
        &state.task_service,
    )
    .recover_completed_outputs(&context, &request.session_id)
}
#[tauri::command]
pub fn add_import_items_v2(
    state: State<'_, AppState>,
    request: AddImportItemsV2Request,
) -> Result<ImportSession, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.import_v2_service.add_inputs(
        &context,
        &state.file_store,
        &request.session_id,
        request.inputs,
    )
}

#[tauri::command]
pub fn add_import_text_v2(
    state: State<'_, AppState>,
    request: AddImportTextV2Request,
) -> Result<ImportSession, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.import_v2_service.add_text_input(
        &context,
        &state.file_store,
        &request.session_id,
        &request.source_name,
        &request.content,
    )
}
#[tauri::command]
pub fn set_import_item_selection_v2(
    state: State<'_, AppState>,
    request: SetImportItemSelectionV2Request,
) -> Result<ImportSession, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.import_v2_service.set_item_selected(
        &context,
        &state.file_store,
        &request.session_id,
        &request.item_id,
        request.selected,
    )?;
    state
        .import_v2_service
        .load_session(&context, &state.file_store, &request.session_id)
}

/// Read a historical session without recovery side effects. History views are
/// inspection-only; opening them must not resume tasks, accept candidates, or
/// rewrite staging/session evidence.
#[tauri::command]
pub fn get_import_history_session_v2(
    state: State<'_, AppState>,
    request: GetImportSessionV2Request,
) -> Result<ImportSession, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    if let Some(batch_id) = request.history_batch_id.as_deref() {
        if let Some(snapshot) = load_history_snapshot(&context, &request.session_id, batch_id)? {
            return Ok(snapshot);
        }
    }
    state
        .import_v2_service
        .load_session(&context, &state.file_store, &request.session_id)
}

pub(crate) fn load_history_snapshot(
    context: &ProjectContext,
    session_id: &str,
    batch_id: &str,
) -> Result<Option<ImportSession>, BackendError> {
    if batch_id.is_empty()
        || batch_id.len() > 64
        || !batch_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(BackendError::new(
            "IMPORT_V2_HISTORY_INVALID",
            "The historical import identity is invalid.",
            false,
            true,
        ));
    }
    let path = context.resolve_project_path(&format!(".app/import-history/{batch_id}.json"))?;
    let bytes = std::fs::read(&path).map_err(|_| {
        BackendError::new(
            "IMPORT_V2_HISTORY_NOT_FOUND",
            "The historical import record could not be opened.",
            true,
            true,
        )
    })?;
    let batch: crate::models::import_v2::ImportBatchResult = serde_json::from_slice(&bytes).map_err(|_| {
        BackendError::new(
            "IMPORT_V2_HISTORY_CORRUPT",
            "The historical import record is incomplete.",
            true,
            true,
        )
    })?;
    if batch.session_id != session_id {
        return Err(BackendError::new(
            "IMPORT_V2_HISTORY_SCOPE_MISMATCH",
            "The historical import record does not belong to this session.",
            false,
            true,
        ));
    }
    Ok(batch.history_snapshot)
}

#[tauri::command]
pub fn cancel_import_item_v2(
    state: State<'_, AppState>,
    request: CancelImportItemV2Request,
) -> Result<ImportSession, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.import_v2_service.cancel_queued_item(
        &context,
        &state.file_store,
        &request.session_id,
        &request.item_id,
    )?;
    state
        .import_v2_service
        .load_session(&context, &state.file_store, &request.session_id)
}

#[tauri::command]
pub fn skip_import_item_v2(
    state: State<'_, AppState>,
    request: CancelImportItemV2Request,
) -> Result<ImportSession, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.import_v2_service.skip_item(
        &context,
        &state.file_store,
        &state.task_service,
        &request.session_id,
        &request.item_id,
    )?;
    state
        .import_v2_service
        .load_session(&context, &state.file_store, &request.session_id)
}

#[tauri::command]
pub fn start_import_items_v2(
    app: AppHandle,
    state: State<'_, AppState>,
    request: StartImportItemsV2Request,
) -> Result<Vec<BackendTask>, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let session =
        state
            .import_v2_service
            .load_session(&context, &state.file_store, &request.session_id)?;
    let unique_ids: HashSet<&str> = request.item_ids.iter().map(String::as_str).collect();
    if unique_ids.len() != request.item_ids.len() {
        return Err(task_error("Import item ids must be unique."));
    }
    for item_id in &request.item_ids {
        if !session.items.iter().any(|item| item.item_id == *item_id) {
            return Err(task_error("Import item was not found."));
        }
    }
    // One IPC call is one user-visible import operation. Persist the identity
    // on every child task so parallel operations remain independently
    // observable and cancellable after navigation or app restart.
    let batch_id = Uuid::new_v4().to_string();
    let recovery_action = request.recovery_action.clone();
    let prepared = prepare_all(
        request.item_ids,
        |item_id| {
            let item = session
                .items
                .iter()
                .find(|item| item.item_id == item_id)
                .expect("all item ids were validated before task creation");
            state
                .task_service
                .create_project_task_with_batch(
                    TaskType::Import,
                    request.project_id.clone(),
                    context.root.clone(),
                    format!("Import {}", item.input.display_name),
                    true,
                    batch_id.clone(),
                )
                .map_err(|error| task_error(&error))
        },
        |task| {
            state
                .task_service
                .discard_unstarted_tasks(std::slice::from_ref(&task.id))
                .map_err(|error| task_error(&error))
        },
    )?;
    let bindings = prepared
        .iter()
        .map(|(item_id, task)| (item_id.clone(), task.id.clone()))
        .collect::<Vec<_>>();
    if let Err(error) = state.import_v2_service.bind_item_task_ids(
        &context,
        &state.file_store,
        &request.session_id,
        &bindings,
    ) {
        for (_, task) in &prepared {
            let _ = state
                .task_service
                .discard_unstarted_tasks(std::slice::from_ref(&task.id));
        }
        return Err(error);
    }
    let mut tasks = Vec::with_capacity(prepared.len());
    for (item_id, task) in prepared {
        let (app, project_id, root, session_id, task_id, recovery_action) = (
            app.clone(),
            request.project_id.clone(),
            request.project_root_path.clone(),
            request.session_id.clone(),
            task.id.clone(),
            recovery_action.clone(),
        );
        tauri::async_runtime::spawn(async move {
            let state = app.state::<AppState>();
            let result = state
                .resolve_project_context(&project_id, &root)
                .and_then(|context| {
                    state.import_v2_service.run_item_with_recovery(
                        &context,
                        &state.file_store,
                        &state.task_service,
                        &session_id,
                        &item_id,
                        &task_id,
                        recovery_action.as_ref(),
                    )
                });
            match result {
                Err(error) => {
                    fail_task_unless_cancelled(&state, &task_id, error);
                    if let Ok(context) = state.resolve_project_context(&project_id, &root) {
                        if let Ok(settings) = state.settings_service.read_settings(&context) {
                            if let Some(agent_kind) = settings.agent_default {
                                run_local_agent_candidate(
                                    &state,
                                    &context,
                                    &session_id,
                                    &item_id,
                                    AgentAssistanceTrigger::DeterministicHardFailure,
                                    agent_kind,
                                );
                            }
                        }
                    }
                }
                Ok(_) => {}
            }
        });
        tasks.push(task);
    }
    Ok(tasks)
}

/// Cancel only the import tasks belonging to one backend-issued batch. The
/// session lookup is intentional: a batch id alone must never reach tasks
/// from another import session in the same project.
#[tauri::command]
pub fn cancel_import_batch_v2(
    state: State<'_, AppState>,
    request: CancelImportBatchV2Request,
) -> Result<Vec<BackendTask>, BackendError> {
    if request.batch_id.trim().is_empty() {
        return Err(task_error("Import batch id must not be empty."));
    }
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let _session = state
        .import_v2_service
        .load_session(&context, &state.file_store, &request.session_id)?;
    // Do not depend on the orchestrator having claimed the session item yet.
    // The task is created and returned before `claim_item_for_run` persists
    // `item.task_id`, so an immediate user cancellation must discover the
    // group from the durable task identity instead of the eventually-written
    // session mapping.
    let task_ids = state
        .task_service
        .list_tasks(None)
        .into_iter()
        .filter(|task| {
            task.project_id.as_deref() == Some(request.project_id.as_str())
                && task.batch_id.as_deref() == Some(request.batch_id.as_str())
                && task.cancellable
                && matches!(
                    task.status,
                    TaskStatus::Queued | TaskStatus::Running | TaskStatus::Cancelling
                )
        })
        .map(|task| task.id)
        .collect::<Vec<_>>();

    task_ids
        .into_iter()
        .map(|task_id| {
            state
                .task_service
                .cancel_task(&task_id)
                .map_err(|error| task_error(&error))
        })
        .collect()
}

fn run_local_agent_candidate(
    state: &AppState,
    context: &crate::models::paths::ProjectContext,
    session_id: &str,
    item_id: &str,
    trigger: AgentAssistanceTrigger,
    agent_kind: crate::models::agent::AgentKind,
) {
    let assistance = AgentAssistanceService::new(
        &state.import_v2_service,
        &state.file_store,
        &state.settings_service,
        &state.agent_service,
        &state.task_service,
        AgentAssistanceService::bundled_skill_path(),
    );
    let Ok(agent_task) = assistance.start_local(context, session_id, item_id, trigger, agent_kind)
    else {
        return;
    };
    if assistance
        .run_local(
            context,
            session_id,
            item_id,
            &agent_task.id,
            trigger,
            agent_kind,
        )
        .is_ok()
    {
        let accepted = AgentCandidateService::new(
            &state.import_v2_service,
            &state.file_store,
            &state.task_service,
        )
        .accept_staged_output(context, session_id, item_id, &agent_task.id);
        if accepted.is_err() {
            let _ = state.task_service.append_log(
                &agent_task.id,
                crate::tasks::task_model::LogLevel::Warn,
                "Agent output was staged but candidate validation failed; the deterministic result was preserved.".into(),
            );
        }
    }
}

fn prepare_all<T>(
    item_ids: Vec<String>,
    mut create: impl FnMut(&str) -> Result<T, BackendError>,
    mut rollback: impl FnMut(&T) -> Result<(), BackendError>,
) -> Result<Vec<(String, T)>, BackendError> {
    let mut prepared = Vec::with_capacity(item_ids.len());
    for item_id in item_ids {
        match create(&item_id) {
            Ok(task) => prepared.push((item_id, task)),
            Err(error) => {
                let mut rollback_error = None;
                for (_, task) in &prepared {
                    if let Err(error) = rollback(task) {
                        rollback_error.get_or_insert(error);
                    }
                }
                return Err(rollback_error.unwrap_or(error));
            }
        }
    }
    Ok(prepared)
}

#[tauri::command]
pub fn confirm_import_session_v2(
    app: AppHandle,
    state: State<'_, AppState>,
    request: CommitImportSessionRequest,
) -> Result<BackendTask, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let task = state
        .task_service
        .create_project_task(
            TaskType::Import,
            request.project_id.clone(),
            context.root,
            "Confirm import session".into(),
            true,
        )
        .map_err(|error| task_error(&error))?;
    let task_id = task.id.clone();
    let mut commit_request = request.clone();
    commit_request.batch_task_id = Some(task_id.clone());
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let result = (|| -> Result<TaskResult, BackendError> {
            let context =
                state.resolve_project_context(&request.project_id, &request.project_root_path)?;
            state
                .task_service
                .transition_status(&task_id, TaskStatus::Running)
                .map_err(|error| task_error(&error))?;
            let batch = state.import_v2_service.commit_items_cancellable(
                &context,
                &state.file_store,
                &state.git_service,
                &commit_request,
                || state.task_service.is_cancelled(&task_id),
            )?;
            Ok(TaskResult {
                summary: format!(
                    "Committed {} import item(s); {} failed.",
                    batch.committed_count, batch.failed_count
                ),
                affected_paths: vec![format!(".app/import-history/{}.json", batch.batch_id)],
                reference: Some(TaskResultReference::ImportV2SessionPreview {
                    session_id: batch.session_id.clone(),
                    batch_id: Some(batch.batch_id.clone()),
                }),
                pending_action: None,
            })
        })();
        match result {
            Ok(result) => {
                let _ = state
                    .task_service
                    .complete_running_with_result(&task_id, result);
            }
            Err(error) => {
                fail_task_unless_cancelled(&state, &task_id, error);
            }
        }
    });
    Ok(task)
}

fn task_error(message: &str) -> BackendError {
    BackendError::new("IMPORT_V2_TASK_FAILED", message, true, false)
}

fn fail_task_unless_cancelled(state: &AppState, task_id: &str, error: BackendError) {
    let _ = state.task_service.set_error(task_id, error);
    if !matches!(
        state.task_service.get_task(task_id).map(|task| task.status),
        Some(TaskStatus::Cancelled | TaskStatus::WaitingForConfirmation)
    ) {
        let _ = state
            .task_service
            .transition_status(task_id, TaskStatus::Failed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::import_v2::ImportInputKind;

    #[test]
    fn add_items_request_uses_ids_and_inputs_not_target_paths() {
        let request = AddImportItemsV2Request {
            project_id: "p1".into(),
            project_root_path: "fixture/project".into(),
            session_id: "s1".into(),
            inputs: vec![ImportInput {
                kind: ImportInputKind::File,
                display_name: "a.pdf".into(),
                locator: "fixture/in/a.pdf".into(),
                normalized_locator: None,
                source_identity: None,
            }],
        };
        let value = serde_json::to_value(request).unwrap();
        assert_eq!(value["sessionId"], "s1");
        assert!(value.get("targetPath").is_none());
        assert!(value.get("wikiPath").is_none());
    }

    #[test]
    fn task_preparation_rolls_back_every_created_task_on_later_failure() {
        let mut attempts = 0;
        let mut rolled_back = Vec::new();
        let result = prepare_all(
            ["first", "second", "third"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            |_| {
                attempts += 1;
                if attempts == 3 {
                    Err(task_error("injected persistence failure"))
                } else {
                    Ok(attempts)
                }
            },
            |task| {
                rolled_back.push(*task);
                Ok(())
            },
        );

        assert!(result.is_err());
        assert_eq!(rolled_back, vec![1, 2]);
    }
}
