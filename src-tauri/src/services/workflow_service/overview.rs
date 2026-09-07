use crate::errors::BackendError;
use crate::models::workflow::{
    WorkflowDisplayStatus, WorkflowFilesystemAccess, WorkflowKind, WorkflowOverviewRow,
    WorkflowOverviewState, WorkflowPrerequisite, WorkflowPrerequisiteAction,
    WorkflowProjectAccessSummary, WorkflowProjectTrust, WorkflowRunSummary, WorkflowsOverview,
    WORKFLOW_SCHEMA_VERSION,
};
use crate::tasks::TaskService;

#[derive(Default)]
pub struct WorkflowOverviewService;

impl WorkflowOverviewService {
    pub fn no_project(&self) -> WorkflowsOverview {
        let prerequisite = open_project_prerequisite();
        WorkflowsOverview {
            schema_version: WORKFLOW_SCHEMA_VERSION,
            session_id: String::new(),
            active_runs: Vec::new(),
            project_access: None,
            recent_runs: Vec::new(),
            context_summary: None,
            rows: fixed_kinds()
                .into_iter()
                .map(|kind| row_for_kind(kind, None, None, Some(prerequisite.clone())))
                .collect(),
        }
    }

    /// Pure bounded task read model. The access snapshot is display information;
    /// preparation and execution still validate their actual inputs and authority.
    pub(crate) fn for_project(
        &self,
        access: WorkflowProjectAccessSummary,
        tasks: &TaskService,
    ) -> Result<WorkflowsOverview, BackendError> {
        let snapshot = tasks
            .workflow_owner_snapshot(&access.canonical_identity_key, &access.identity_revision);
        let rows = fixed_kinds()
            .into_iter()
            .map(|kind| {
                let prerequisite = match (&kind, &access.trust, &access.filesystem_access) {
                    (WorkflowKind::HealthCheck, _, _) => None,
                    (_, WorkflowProjectTrust::Untrusted, _) => Some(WorkflowPrerequisite {
                        code: "WORKFLOW_PROJECT_UNTRUSTED".into(),
                        message_key: "workflows.prerequisite.trustProject".into(),
                        blocking: true,
                        action: WorkflowPrerequisiteAction::TrustProject,
                    }),
                    (_, _, WorkflowFilesystemAccess::ReadOnly) => Some(WorkflowPrerequisite {
                        code: "WORKFLOW_PROJECT_READ_ONLY".into(),
                        message_key: "workflows.prerequisite.makeWritable".into(),
                        blocking: true,
                        action: WorkflowPrerequisiteAction::MakeWritable,
                    }),
                    _ => None,
                };
                let attention = snapshot.attention.iter().find(|run| run.kind == kind);
                let completed = snapshot.completed.iter().find(|run| run.kind == kind);
                row_for_kind(kind, attention, completed, prerequisite)
            })
            .collect();
        Ok(WorkflowsOverview {
            schema_version: WORKFLOW_SCHEMA_VERSION,
            session_id: tasks.workflow_session_id().to_string(),
            active_runs: snapshot.attention,
            project_access: Some(access),
            recent_runs: snapshot.recent_runs,
            context_summary: Some(snapshot.context),
            rows,
        })
    }
}

fn row_for_kind(
    kind: WorkflowKind,
    attention: Option<&WorkflowRunSummary>,
    completed: Option<&WorkflowRunSummary>,
    prerequisite: Option<WorkflowPrerequisite>,
) -> WorkflowOverviewRow {
    let state = attention.map_or_else(
        || {
            if prerequisite.is_some() {
                WorkflowOverviewState::NeedsPrerequisite
            } else {
                WorkflowOverviewState::Ready
            }
        },
        |run| match run.display_status {
            WorkflowDisplayStatus::Queued => WorkflowOverviewState::Queued,
            WorkflowDisplayStatus::Running => WorkflowOverviewState::Running,
            WorkflowDisplayStatus::WaitingForConfirmation => {
                WorkflowOverviewState::WaitingForConfirmation
            }
            WorkflowDisplayStatus::Failed => WorkflowOverviewState::Failed,
            WorkflowDisplayStatus::Interrupted => WorkflowOverviewState::Interrupted,
            WorkflowDisplayStatus::Completed | WorkflowDisplayStatus::Cancelled => {
                WorkflowOverviewState::Ready
            }
        },
    );
    WorkflowOverviewRow {
        kind,
        state,
        recommended: false,
        active_task_id: attention.map(|run| run.task_id.clone()),
        active_continuation_required: attention.is_some_and(|run| run.continuation_required),
        last_completed_at: completed.and_then(|run| run.completed_at.clone()),
        last_completed_task_id: completed.map(|run| run.task_id.clone()),
        prerequisite,
    }
}

fn fixed_kinds() -> [WorkflowKind; 3] {
    [
        WorkflowKind::UpdateWiki,
        WorkflowKind::HealthCheck,
        WorkflowKind::GenerateContent,
    ]
}

fn open_project_prerequisite() -> WorkflowPrerequisite {
    WorkflowPrerequisite {
        code: "WORKFLOW_PROJECT_REQUIRED".into(),
        message_key: "workflows.prerequisite.openOrCreateProject".into(),
        blocking: true,
        action: WorkflowPrerequisiteAction::OpenOrCreateProject,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::workflow::WorkflowResult;

    #[test]
    fn unknown_content_does_not_claim_update_is_current() {
        let row = row_for_kind(WorkflowKind::UpdateWiki, None, None, None);
        assert_eq!(row.state, WorkflowOverviewState::Ready);
        assert!(!row.recommended);
    }

    fn test_workflow_state(
        identity_key: &str,
        identity_revision: &str,
        queue_position: u32,
    ) -> crate::models::workflow::WorkflowExecutionState {
        serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "canonicalIdentityKey": identity_key,
            "identityRevision": identity_revision,
            "kind": "health_check",
            "scope": { "kind": "health_check", "mode": "local_quick" },
            "executionOptions": { "preparationRevision": "test-preparation" },
            "route": { "kind": "local", "routeRevision": "local" },
            "fingerprint": format!("fingerprint-{identity_key}-{identity_revision}-{queue_position}"),
            "baselineFingerprint": "health-baseline",
            "persistence": "memory_only",
            "stages": [],
            "currentStageId": null,
            "queuePosition": queue_position,
            "continuationRequired": false,
            "retry": null,
            "pendingAction": null,
            "result": null,
            "error": null,
            "cancelledFromQueue": false,
            "undoCancelUntil": null
        }))
        .unwrap()
    }

    #[test]
    fn project_overview_context_excludes_foreign_identity_and_old_revision_runs() {
        let root = tempfile::tempdir().unwrap();
        let tasks = TaskService::default();
        let create = |identity_key: &str, identity_revision: &str, queue_position: u32| {
            tasks
                .create_workflow_task(
                    "project-a".into(),
                    root.path().to_path_buf(),
                    "Health".into(),
                    test_workflow_state(identity_key, identity_revision, queue_position),
                    None,
                )
                .unwrap()
        };
        let current_completed = create("identity-current", "revision-current", 1);
        tasks
            .transition_workflow_status(
                &current_completed.task_id,
                crate::models::task::TaskStatus::Running,
            )
            .unwrap();
        tasks
            .complete_workflow(
                &current_completed.task_id,
                WorkflowResult::HealthCheck {
                    report_id: None,
                    persistent: true,
                    report_digest: None,
                    error_count: 1,
                    warning_count: 2,
                    info_count: 3,
                    coverage: None,
                    findings_by_type: Default::default(),
                },
            )
            .unwrap();
        let current_queue = create("identity-current", "revision-current", 2);
        let _foreign_queue = create("identity-foreign", "revision-current", 1);
        let _old_revision_queue = create("identity-current", "revision-old", 1);
        let access: WorkflowProjectAccessSummary = serde_json::from_value(serde_json::json!({
            "projectId": "project-a",
            "canonicalIdentityKey": "identity-current",
            "identityRevision": "revision-current",
            "trust": "trusted",
            "filesystemAccess": "writable",
            "persistence": "memory_only",
            "gitState": "clean"
        }))
        .unwrap();
        let overview = WorkflowOverviewService.for_project(access, &tasks).unwrap();
        let summary = overview.context_summary.unwrap();

        assert_eq!(summary.pending_source_count, None);
        assert_eq!(summary.queue_count, 1);
        assert_eq!(summary.queued_runs[0].task_id, current_queue.task_id);
        assert_eq!(summary.last_health.unwrap().error_count, 1);
        assert!(overview.recent_runs.iter().all(|run| {
            run.canonical_identity_key == "identity-current"
                && run.identity_revision == "revision-current"
        }));
    }
}
