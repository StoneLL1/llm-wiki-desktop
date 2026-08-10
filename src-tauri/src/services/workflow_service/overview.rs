use crate::errors::BackendError;
use crate::models::workflow::{
    WorkflowDisplayStatus, WorkflowKind, WorkflowOverviewRow, WorkflowOverviewState,
    WorkflowPrerequisite, WorkflowPrerequisiteAction, WorkflowProjectAccessSummary,
    WorkflowsOverview, WORKFLOW_SCHEMA_VERSION,
};
use crate::tasks::TaskService;

use super::preparation::WorkflowOverviewEvaluationSnapshot;

#[derive(Default)]
pub struct WorkflowOverviewService;

impl WorkflowOverviewService {
    pub fn no_project(&self) -> WorkflowsOverview {
        let prerequisite = open_project_prerequisite();
        WorkflowsOverview {
            schema_version: WORKFLOW_SCHEMA_VERSION,
            project_access: None,
            recent_runs: Vec::new(),
            rows: fixed_kinds()
                .into_iter()
                .map(|kind| WorkflowOverviewRow {
                    kind,
                    state: WorkflowOverviewState::NeedsPrerequisite,
                    recommended: false,
                    active_task_id: None,
                    active_continuation_required: false,
                    last_completed_at: None,
                    last_completed_task_id: None,
                    prerequisite: Some(prerequisite.clone()),
                })
                .collect(),
        }
    }

    pub(super) fn for_project(
        &self,
        access: WorkflowProjectAccessSummary,
        evaluation: &WorkflowOverviewEvaluationSnapshot,
        tasks: &TaskService,
    ) -> Result<WorkflowsOverview, BackendError> {
        let owner_runs = tasks
            .list_workflow_runs()
            .into_iter()
            .filter(|run| {
                run.canonical_identity_key == access.canonical_identity_key
                    && run.identity_revision == access.identity_revision
            })
            .collect::<Vec<_>>();
        let current_health_baseline = evaluation
            .prerequisites
            .iter()
            .find(|(kind, _, _)| *kind == WorkflowKind::HealthCheck)
            .map(|(_, _, baseline)| baseline.as_str());
        let has_current_health = owner_runs.iter().any(|run| {
            run.kind == WorkflowKind::HealthCheck
                && run.display_status == WorkflowDisplayStatus::Completed
                && current_health_baseline == Some(run.baseline_fingerprint.as_str())
        });
        let recommendation = if !evaluation.has_sources || evaluation.changed_source_count > 0 {
            Some(WorkflowKind::UpdateWiki)
        } else if evaluation.has_readable_markdown && !has_current_health {
            Some(WorkflowKind::HealthCheck)
        } else {
            None
        };
        let rows = fixed_kinds()
            .into_iter()
            .map(|kind| {
                row_for_kind(
                    kind.clone(),
                    &owner_runs,
                    recommendation.as_ref(),
                    evaluation.has_sources,
                    evaluation.changed_source_count,
                    evaluation
                        .prerequisites
                        .iter()
                        .find(|(candidate, _, _)| *candidate == kind)
                        .and_then(|(_, prerequisite, _)| prerequisite.clone()),
                )
            })
            .collect();
        Ok(WorkflowsOverview {
            schema_version: WORKFLOW_SCHEMA_VERSION,
            project_access: Some(access),
            recent_runs: owner_runs.iter().take(5).cloned().collect(),
            rows,
        })
    }
}

fn row_for_kind(
    kind: WorkflowKind,
    runs: &[crate::models::workflow::WorkflowRun],
    recommendation: Option<&WorkflowKind>,
    has_sources: bool,
    changed_source_count: usize,
    mut prerequisite: Option<WorkflowPrerequisite>,
) -> WorkflowOverviewRow {
    let mut matching = runs
        .iter()
        .filter(|run| run.kind == kind)
        .collect::<Vec<_>>();
    matching.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    let attention = [
        WorkflowDisplayStatus::WaitingForConfirmation,
        WorkflowDisplayStatus::Running,
        WorkflowDisplayStatus::Queued,
        WorkflowDisplayStatus::Failed,
        WorkflowDisplayStatus::Interrupted,
    ]
    .into_iter()
    .find_map(|status| {
        matching
            .iter()
            .copied()
            .find(|run| run.display_status == status)
    });
    let last_completed = matching
        .iter()
        .copied()
        .filter(|run| run.display_status == WorkflowDisplayStatus::Completed)
        .filter(|run| run.completed_at.is_some())
        .max_by(|left, right| left.completed_at.cmp(&right.completed_at));
    let last_completed_at = last_completed.and_then(|run| run.completed_at.clone());
    let update_is_current =
        kind == WorkflowKind::UpdateWiki && has_sources && changed_source_count == 0;
    if update_is_current {
        prerequisite = None;
    }
    let state = attention.map_or_else(
        || {
            if update_is_current {
                WorkflowOverviewState::UpToDate
            } else if prerequisite.is_some() {
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
        recommended: recommendation.is_some_and(|recommended| *recommended == kind),
        active_task_id: attention.map(|run| run.task_id.clone()),
        active_continuation_required: attention.is_some_and(|run| run.continuation_required),
        last_completed_at,
        last_completed_task_id: last_completed.map(|run| run.task_id.clone()),
        prerequisite,
        kind,
        state,
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

    fn workflow_run(
        task_id: &str,
        status: &str,
        updated_at: &str,
        completed_at: Option<&str>,
        continuation_required: bool,
    ) -> crate::models::workflow::WorkflowRun {
        serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "taskId": task_id,
            "projectId": "project-a",
            "canonicalIdentityKey": "identity-a",
            "identityRevision": "revision-a",
            "kind": "update_wiki",
            "displayStatus": status,
            "scope": { "kind": "update_wiki", "mode": "changed_sources", "sourceVersions": [] },
            "route": null,
            "fingerprint": format!("fingerprint-{task_id}"),
            "baselineFingerprint": "baseline-a",
            "stages": [],
            "currentStageId": null,
            "queuePosition": if status == "queued" { Some(1) } else { None },
            "continuationRequired": continuation_required,
            "retry": null,
            "pendingAction": null,
            "decisionReview": null,
            "result": null,
            "error": null,
            "startedAt": "2026-08-01T00:00:00Z",
            "updatedAt": updated_at,
            "completedAt": completed_at,
            "cancellable": status == "queued",
            "undoCancelUntil": null
        }))
        .expect("workflow test run must deserialize")
    }

    #[test]
    fn consumed_sources_are_up_to_date_even_when_execution_is_currently_blocked() {
        let row = row_for_kind(
            WorkflowKind::UpdateWiki,
            &[],
            None,
            true,
            0,
            Some(WorkflowPrerequisite {
                code: "WORKFLOW_PROJECT_UNTRUSTED".into(),
                message_key: "workflows.prerequisite.trustProject".into(),
                blocking: true,
                action: WorkflowPrerequisiteAction::TrustProject,
            }),
        );
        assert_eq!(row.state, WorkflowOverviewState::UpToDate);
        assert!(row.prerequisite.is_none());
    }

    #[test]
    fn overview_row_carries_bounded_action_targets_outside_recent_runs() {
        let completed = workflow_run(
            "older-completed-update",
            "completed",
            "2026-08-01T00:00:00Z",
            Some("2026-08-01T00:00:00Z"),
            false,
        );
        let recovered_queue = workflow_run(
            "recovered-queued-update",
            "queued",
            "2026-08-10T00:00:00Z",
            None,
            true,
        );

        let row = row_for_kind(
            WorkflowKind::UpdateWiki,
            &[completed, recovered_queue],
            None,
            true,
            1,
            None,
        );

        assert_eq!(
            row.active_task_id.as_deref(),
            Some("recovered-queued-update")
        );
        assert!(row.active_continuation_required);
        assert_eq!(
            row.last_completed_task_id.as_deref(),
            Some("older-completed-update")
        );
        assert_eq!(
            row.last_completed_at.as_deref(),
            Some("2026-08-01T00:00:00Z")
        );
    }

    #[test]
    fn same_kind_attention_prefers_waiting_and_running_over_a_newer_queue() {
        let queued = workflow_run(
            "newer-queued-update",
            "queued",
            "2026-08-10T10:00:00Z",
            None,
            true,
        );
        let waiting = workflow_run(
            "older-waiting-update",
            "waiting_for_confirmation",
            "2026-08-10T08:00:00Z",
            None,
            false,
        );
        let running = workflow_run(
            "older-running-update",
            "running",
            "2026-08-10T09:00:00Z",
            None,
            false,
        );

        let waiting_row = row_for_kind(
            WorkflowKind::UpdateWiki,
            &[queued.clone(), waiting],
            None,
            true,
            1,
            None,
        );
        assert_eq!(
            waiting_row.state,
            WorkflowOverviewState::WaitingForConfirmation
        );
        assert_eq!(
            waiting_row.active_task_id.as_deref(),
            Some("older-waiting-update")
        );
        assert!(!waiting_row.active_continuation_required);

        let running_row = row_for_kind(
            WorkflowKind::UpdateWiki,
            &[queued, running],
            None,
            true,
            1,
            None,
        );
        assert_eq!(running_row.state, WorkflowOverviewState::Running);
        assert_eq!(
            running_row.active_task_id.as_deref(),
            Some("older-running-update")
        );
        assert!(!running_row.active_continuation_required);
    }
}
