use crate::errors::BackendError;
use crate::models::workflow::{
    WorkflowArtifactContextSummary, WorkflowContextSummary, WorkflowDisplayStatus,
    WorkflowHealthContextSummary, WorkflowKind, WorkflowOperation, WorkflowOverviewRow,
    WorkflowOverviewState, WorkflowPrerequisite, WorkflowPrerequisiteAction,
    WorkflowProjectAccessSummary, WorkflowQueueContextItem, WorkflowResult, WorkflowRun,
    WorkflowRunSummary, WorkflowsOverview, WORKFLOW_SCHEMA_VERSION,
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
            context_summary: None,
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
        let mut owner_runs = tasks
            .list_workflow_runs()
            .into_iter()
            .filter(|run| {
                run.canonical_identity_key == access.canonical_identity_key
                    && run.identity_revision == access.identity_revision
            })
            .collect::<Vec<_>>();
        owner_runs.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        let current_health_baseline = evaluation
            .prerequisites
            .iter()
            .find(|(kind, _, _)| *kind == WorkflowKind::HealthCheck)
            .map(|(_, _, baseline)| baseline.as_str());
        let has_current_health = current_health_baseline.is_some_and(|baseline| {
            owner_runs
                .iter()
                .any(|run| is_current_built_in_health(run, baseline))
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
            recent_runs: owner_runs
                .iter()
                .take(5)
                .map(WorkflowRunSummary::from)
                .collect(),
            context_summary: Some(context_summary(
                evaluation.changed_source_count,
                &owner_runs,
            )),
            rows,
        })
    }
}

fn context_summary(pending_source_count: usize, runs: &[WorkflowRun]) -> WorkflowContextSummary {
    let latest_health = runs
        .iter()
        .filter(|run| is_completed_built_in_health(run))
        .max_by(|left, right| left.completed_at.cmp(&right.completed_at));
    let last_health = latest_health.and_then(|run| match (&run.result, &run.completed_at) {
        (
            Some(WorkflowResult::HealthCheck {
                error_count,
                warning_count,
                info_count,
                ..
            }),
            Some(completed_at),
        ) => Some(WorkflowHealthContextSummary {
            task_id: run.task_id.clone(),
            completed_at: completed_at.clone(),
            error_count: *error_count,
            warning_count: *warning_count,
            info_count: *info_count,
        }),
        _ => None,
    });
    let latest_artifact = runs
        .iter()
        .filter(|run| {
            run.kind == WorkflowKind::GenerateContent
                && run.display_status == WorkflowDisplayStatus::Completed
        })
        .max_by(|left, right| left.completed_at.cmp(&right.completed_at));
    let recent_artifact = latest_artifact.and_then(|run| match (&run.result, &run.completed_at) {
        (Some(WorkflowResult::GenerateContent { artifact_type, .. }), Some(completed_at)) => {
            Some(WorkflowArtifactContextSummary {
                task_id: run.task_id.clone(),
                completed_at: completed_at.clone(),
                artifact_type: artifact_type.clone(),
            })
        }
        _ => None,
    });
    let mut queued = runs
        .iter()
        .filter(|run| run.display_status == WorkflowDisplayStatus::Queued)
        .collect::<Vec<_>>();
    queued.sort_by(|left, right| {
        left.queue_position
            .unwrap_or(u32::MAX)
            .cmp(&right.queue_position.unwrap_or(u32::MAX))
            .then_with(|| left.started_at.cmp(&right.started_at))
    });

    WorkflowContextSummary {
        pending_source_count,
        last_health,
        recent_artifact,
        queue_count: queued.len(),
        queued_runs: queued
            .into_iter()
            .take(5)
            .map(|run| WorkflowQueueContextItem {
                task_id: run.task_id.clone(),
                kind: run.kind.clone(),
                operation: run.operation.clone(),
                queue_position: run.queue_position,
                started_at: run.started_at.clone(),
            })
            .collect(),
    }
}

fn is_completed_built_in_health(run: &WorkflowRun) -> bool {
    run.kind == WorkflowKind::HealthCheck
        && run.operation == WorkflowOperation::BuiltIn
        && run.display_status == WorkflowDisplayStatus::Completed
        && matches!(run.result, Some(WorkflowResult::HealthCheck { .. }))
}

fn is_current_built_in_health(run: &WorkflowRun, baseline: &str) -> bool {
    is_completed_built_in_health(run) && run.baseline_fingerprint == baseline
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

    #[test]
    fn context_summary_uses_all_identity_owned_runs_but_bounds_queue_items() {
        let mut runs = (0..6)
            .map(|index| {
                workflow_run(
                    &format!("queued-{index}"),
                    "queued",
                    &format!("2026-08-10T1{index}:00:00Z"),
                    None,
                    false,
                )
            })
            .collect::<Vec<_>>();
        let queue_facts = [
            (Some(3), "2026-08-01T00:03:00Z"),
            (None, "2026-08-01T00:00:00Z"),
            (Some(1), "2026-08-01T00:01:00Z"),
            (Some(2), "2026-08-01T00:02:00Z"),
            (Some(2), "2026-08-01T00:04:00Z"),
            (Some(4), "2026-08-01T00:05:00Z"),
        ];
        for (run, (position, started_at)) in runs.iter_mut().zip(queue_facts) {
            run.queue_position = position;
            run.started_at = started_at.into();
        }
        runs.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));

        let mut health = workflow_run(
            "older-health",
            "completed",
            "2026-08-01T00:00:00Z",
            Some("2026-08-01T00:00:00Z"),
            false,
        );
        health.kind = WorkflowKind::HealthCheck;
        health.result = Some(WorkflowResult::HealthCheck {
            report_id: None,
            persistent: true,
            error_count: 1,
            warning_count: 2,
            info_count: 3,
            coverage: None,
            findings_by_type: Default::default(),
        });
        let mut artifact = workflow_run(
            "older-artifact",
            "completed",
            "2026-07-31T00:00:00Z",
            Some("2026-07-31T00:00:00Z"),
            false,
        );
        artifact.kind = WorkflowKind::GenerateContent;
        artifact.result = Some(WorkflowResult::GenerateContent {
            artifact_type: crate::models::workflow::WorkflowArtifactType::ProjectReport,
            record_id: None,
            output_paths: vec!["exports/report.html".into()],
            artifact_count: Some(1),
            validation_passed: true,
        });
        runs.extend([health, artifact]);

        let summary = context_summary(4, &runs);

        assert_eq!(summary.pending_source_count, 4);
        assert_eq!(summary.queue_count, 6);
        assert_eq!(summary.queued_runs.len(), 5);
        assert_eq!(
            summary
                .queued_runs
                .iter()
                .map(|run| run.task_id.as_str())
                .collect::<Vec<_>>(),
            vec!["queued-2", "queued-3", "queued-4", "queued-0", "queued-5"]
        );
        assert_eq!(summary.last_health.unwrap().task_id, "older-health");
        assert_eq!(summary.recent_artifact.unwrap().task_id, "older-artifact");
    }

    #[test]
    fn context_summary_ignores_resultless_health_completion() {
        let mut older = workflow_run(
            "older-health-with-result",
            "completed",
            "2026-08-01T00:00:00Z",
            Some("2026-08-01T00:00:00Z"),
            false,
        );
        older.kind = WorkflowKind::HealthCheck;
        older.result = Some(WorkflowResult::HealthCheck {
            report_id: None,
            persistent: true,
            error_count: 9,
            warning_count: 0,
            info_count: 0,
            coverage: None,
            findings_by_type: Default::default(),
        });
        let mut newer = workflow_run(
            "newer-health-without-result",
            "completed",
            "2026-08-02T00:00:00Z",
            Some("2026-08-02T00:00:00Z"),
            false,
        );
        newer.kind = WorkflowKind::HealthCheck;

        let summary = context_summary(0, &[newer, older]);

        assert_eq!(
            summary.last_health.unwrap().task_id,
            "older-health-with-result"
        );
    }

    #[test]
    fn newer_agent_repair_does_not_hide_or_satisfy_built_in_health() {
        let mut health = workflow_run(
            "older-health",
            "completed",
            "2026-08-01T00:00:00Z",
            Some("2026-08-01T00:00:00Z"),
            false,
        );
        health.kind = WorkflowKind::HealthCheck;
        health.result = Some(WorkflowResult::HealthCheck {
            report_id: Some("older-health".into()),
            persistent: false,
            error_count: 1,
            warning_count: 2,
            info_count: 3,
            coverage: None,
            findings_by_type: Default::default(),
        });
        let mut repair = workflow_run(
            "newer-repair",
            "completed",
            "2026-08-02T00:00:00Z",
            Some("2026-08-02T00:00:00Z"),
            false,
        );
        repair.kind = WorkflowKind::HealthCheck;
        repair.operation = crate::models::workflow::WorkflowOperation::AgentLintRepair {
            preparation_id: "prepare-1".into(),
            preparation_revision: "prepare-revision-1".into(),
            report_id: "older-health".into(),
            selection_revision: "selection-revision-1".into(),
            selected_finding_ids: vec!["contradiction:wiki/a.md".into()],
            skill: crate::models::lint::WikiLintSkillRef::builtin(),
            authorized_path_hashes: [("wiki/a.md".into(), Some("a".repeat(64)))]
                .into_iter()
                .collect(),
            expected_git_head: "b".repeat(40),
        };
        repair.result = Some(WorkflowResult::AgentLintRepair {
            outcome: crate::models::lint::AgentLintRepairOutcome::Succeeded,
            resolved_finding_ids: vec!["contradiction:wiki/a.md".into()],
            unresolved_finding_ids: Vec::new(),
            introduced_finding_ids: Vec::new(),
            skipped_finding_ids: Vec::new(),
            rounds: Vec::new(),
            affected_paths: vec!["wiki/a.md".into()],
            checkpoint_hash: Some("c".repeat(40)),
            final_commit: Some("d".repeat(40)),
            diff_available: true,
            rollback_available: true,
        });

        assert!(is_current_built_in_health(&health, "baseline-a"));
        assert!(!is_current_built_in_health(&repair, "baseline-a"));
        assert_eq!(
            context_summary(0, &[repair, health])
                .last_health
                .unwrap()
                .task_id,
            "older-health"
        );
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
        let evaluation = WorkflowOverviewEvaluationSnapshot {
            prerequisites: fixed_kinds()
                .into_iter()
                .map(|kind| (kind, None, "health-baseline".into()))
                .collect(),
            has_sources: true,
            changed_source_count: 7,
            has_readable_markdown: true,
        };

        let overview = WorkflowOverviewService
            .for_project(access, &evaluation, &tasks)
            .unwrap();
        let summary = overview.context_summary.unwrap();

        assert_eq!(summary.pending_source_count, 7);
        assert_eq!(summary.queue_count, 1);
        assert_eq!(summary.queued_runs[0].task_id, current_queue.task_id);
        assert_eq!(summary.last_health.unwrap().error_count, 1);
        assert!(overview.recent_runs.iter().all(|run| {
            run.canonical_identity_key == "identity-current"
                && run.identity_revision == "revision-current"
        }));
    }
}
