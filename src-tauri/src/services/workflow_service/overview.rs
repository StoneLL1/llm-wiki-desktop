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
                    last_completed_at: None,
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
    let attention = matching.iter().copied().find(|run| {
        matches!(
            run.display_status,
            WorkflowDisplayStatus::Running
                | WorkflowDisplayStatus::WaitingForConfirmation
                | WorkflowDisplayStatus::Queued
                | WorkflowDisplayStatus::Failed
                | WorkflowDisplayStatus::Interrupted
        )
    });
    let last_completed_at = matching
        .iter()
        .copied()
        .filter(|run| run.display_status == WorkflowDisplayStatus::Completed)
        .filter_map(|run| run.completed_at.clone())
        .max();
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
        last_completed_at,
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
}
