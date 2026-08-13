use std::path::PathBuf;
use std::sync::Mutex;

use crate::models::task::TaskStatus;
use crate::models::workflow::{
    validate_workflow_execution_contract, WorkflowErrorSummary, WorkflowExecutionOptions,
    WorkflowExecutionState, WorkflowKind, WorkflowPersistenceMode, WorkflowPersistenceTransition,
    WorkflowProjectMutationState, WorkflowResult, WorkflowRetryLink, WorkflowRoute, WorkflowRun,
    WorkflowScope, WorkflowStage, WorkflowStartOutcome, WORKFLOW_SCHEMA_VERSION,
};
use crate::tasks::TaskService;
use chrono::{Duration, Utc};

use super::fingerprint::workflow_fingerprint;
use super::persistence::project_identity;

const UNDO_WINDOW_SECONDS: i64 = 10;

#[derive(Debug, Clone)]
pub enum WorkflowDispatchFailure {
    Identity(WorkflowErrorSummary),
    Stale(WorkflowErrorSummary),
    Invariant(WorkflowErrorSummary),
}

impl WorkflowDispatchFailure {
    pub fn identity(code: impl Into<String>, message_key: impl Into<String>) -> Self {
        Self::Identity(dispatch_error(code, message_key, false))
    }

    pub fn stale(code: impl Into<String>, message_key: impl Into<String>) -> Self {
        Self::Stale(dispatch_error(code, message_key, true))
    }

    pub fn stale_not_modified(code: impl Into<String>, message_key: impl Into<String>) -> Self {
        let mut error = dispatch_error(code, message_key, true);
        error.project_mutation_state = WorkflowProjectMutationState::NotModified;
        Self::Stale(error)
    }

    pub fn invariant(code: impl Into<String>, message_key: impl Into<String>) -> Self {
        Self::Invariant(dispatch_error(code, message_key, false))
    }
}

#[derive(Debug, Clone, Default)]
pub struct WorkflowTrustTransition {
    pub stopped_runs: Vec<WorkflowRun>,
    pub continued_local_quick_runs: Vec<WorkflowRun>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct EnqueueWorkflow {
    pub project_id: String,
    pub project_root: PathBuf,
    /// Backend-derived task-state directory. `None` creates an explicitly
    /// memory-only workflow for permitted read-only checks.
    pub task_state_root: Option<PathBuf>,
    pub title: String,
    pub kind: WorkflowKind,
    pub scope: WorkflowScope,
    pub route: Option<WorkflowRoute>,
    pub baseline_fingerprint: String,
    pub execution_options: WorkflowExecutionOptions,
    pub stages: Vec<WorkflowStage>,
    pub retry: Option<WorkflowRetryLink>,
}

#[derive(Default)]
pub struct WorkflowCoordinator {
    operation_lock: Mutex<()>,
}

impl WorkflowCoordinator {
    pub fn enqueue(
        &self,
        tasks: &TaskService,
        request: EnqueueWorkflow,
    ) -> Result<WorkflowStartOutcome, String> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| "Workflow coordinator lock is unavailable")?;
        self.enqueue_locked(tasks, request, true, None, false)
    }

    pub fn enqueue_for_owner(
        &self,
        tasks: &TaskService,
        request: EnqueueWorkflow,
        expected_identity_key: &str,
        expected_identity_revision: &str,
    ) -> Result<WorkflowStartOutcome, String> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| "Workflow coordinator lock is unavailable")?;
        self.enqueue_locked(
            tasks,
            request,
            true,
            Some((expected_identity_key, expected_identity_revision)),
            false,
        )
    }

    /// Create an approval-owned workflow in the project queue without making
    /// it eligible for claim. The confirmation owner must explicitly release
    /// this hold after the atomic claim disposition is known.
    pub fn enqueue_for_owner_pending_approval(
        &self,
        tasks: &TaskService,
        request: EnqueueWorkflow,
        expected_identity_key: &str,
        expected_identity_revision: &str,
    ) -> Result<WorkflowStartOutcome, String> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| "Workflow coordinator lock is unavailable")?;
        self.enqueue_locked(
            tasks,
            request,
            true,
            Some((expected_identity_key, expected_identity_revision)),
            true,
        )
    }

    fn enqueue_locked(
        &self,
        tasks: &TaskService,
        request: EnqueueWorkflow,
        deduplicate: bool,
        expected_owner: Option<(&str, &str)>,
        pending_initial_approval: bool,
    ) -> Result<WorkflowStartOutcome, String> {
        let identity = project_identity(&request.project_root)?;
        if expected_owner.is_some_and(|(key, revision)| {
            identity.canonical_identity_key != key || identity.identity_revision != revision
        }) {
            return Err("Workflow project identity changed before task creation".into());
        }
        validate_workflow_execution_contract(
            &request.kind,
            &request.scope,
            request.route.as_ref(),
            &request.execution_options,
        )?;
        let task_state_root = request
            .task_state_root
            .as_deref()
            .map(|path| {
                canonical_task_state_root(&request.project_root, &identity.canonical_root, path)
            })
            .transpose()?;
        let fingerprint = workflow_fingerprint(
            &identity.canonical_identity_key,
            &identity.identity_revision,
            &request.kind,
            &request.scope,
            &request.execution_options,
            &request.route,
            &request.baseline_fingerprint,
        )?;
        let owner_runs = self.owner_runs(
            tasks,
            &identity.canonical_identity_key,
            &identity.identity_revision,
        );
        if deduplicate {
            if let Some(existing) = owner_runs.iter().find(|run| {
                run.fingerprint == fingerprint
                    && matches!(
                        run.display_status,
                        crate::models::workflow::WorkflowDisplayStatus::Queued
                            | crate::models::workflow::WorkflowDisplayStatus::Running
                            | crate::models::workflow::WorkflowDisplayStatus::WaitingForConfirmation
                    )
            }) {
                return Ok(WorkflowStartOutcome::Existing {
                    run: existing.clone(),
                });
            }
        }

        let has_active = owner_runs.iter().any(|run| {
            matches!(
                run.display_status,
                crate::models::workflow::WorkflowDisplayStatus::Running
                    | crate::models::workflow::WorkflowDisplayStatus::WaitingForConfirmation
            )
        });
        let queued_count = owner_runs
            .iter()
            .filter(|run| {
                run.display_status == crate::models::workflow::WorkflowDisplayStatus::Queued
            })
            .count() as u32;
        let state = WorkflowExecutionState {
            schema_version: WORKFLOW_SCHEMA_VERSION,
            canonical_identity_key: identity.canonical_identity_key,
            identity_revision: identity.identity_revision,
            kind: request.kind,
            scope: request.scope,
            execution_options: request.execution_options,
            route: request.route,
            fingerprint,
            baseline_fingerprint: request.baseline_fingerprint,
            persistence: if task_state_root.is_some() {
                WorkflowPersistenceMode::Persistent
            } else {
                WorkflowPersistenceMode::MemoryOnly
            },
            persistence_transition: None,
            stages: request.stages,
            current_stage_id: None,
            queue_position: Some(queued_count + 1),
            continuation_required: pending_initial_approval,
            retry: request.retry,
            pending_action: None,
            result: None,
            error: None,
            cancelled_from_queue: false,
            undo_cancel_until: None,
        };
        let mut run = tasks.create_workflow_task(
            request.project_id,
            identity.canonical_root,
            request.title,
            state,
            task_state_root,
        )?;
        if !pending_initial_approval && !has_active && queued_count == 0 {
            run = tasks.transition_workflow_status(&run.task_id, TaskStatus::Running)?;
        }
        Ok(WorkflowStartOutcome::Created { run })
    }

    pub fn claim_next(
        &self,
        tasks: &TaskService,
        canonical_identity_key: &str,
        identity_revision: &str,
    ) -> Result<Option<WorkflowRun>, String> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| "Workflow coordinator lock is unavailable")?;
        self.claim_next_locked(tasks, canonical_identity_key, identity_revision)
    }

    fn claim_next_locked(
        &self,
        tasks: &TaskService,
        canonical_identity_key: &str,
        identity_revision: &str,
    ) -> Result<Option<WorkflowRun>, String> {
        let runs = self.owner_runs(tasks, canonical_identity_key, identity_revision);
        if runs.iter().any(|run| {
            matches!(
                run.display_status,
                crate::models::workflow::WorkflowDisplayStatus::Running
                    | crate::models::workflow::WorkflowDisplayStatus::WaitingForConfirmation
            )
        }) {
            return Ok(None);
        }
        let next = runs
            .iter()
            .filter(|run| {
                run.display_status == crate::models::workflow::WorkflowDisplayStatus::Queued
                    && !run.continuation_required
            })
            .min_by_key(|run| {
                (
                    run.queue_position.unwrap_or(u32::MAX),
                    run.started_at.clone(),
                )
            })
            .cloned();
        let Some(next) = next else { return Ok(None) };
        let claimed = tasks.transition_workflow_status(&next.task_id, TaskStatus::Running)?;
        self.renumber(tasks, canonical_identity_key, identity_revision)?;
        Ok(Some(claimed))
    }

    pub fn continue_queued(
        &self,
        tasks: &TaskService,
        canonical_identity_key: &str,
        identity_revision: &str,
    ) -> Result<(Vec<WorkflowRun>, Option<WorkflowRun>), String> {
        self.apply_persistence_and_continue_queued(
            tasks,
            canonical_identity_key,
            identity_revision,
            &[],
            true,
        )
    }

    pub fn apply_persistence_and_continue_queued(
        &self,
        tasks: &TaskService,
        canonical_identity_key: &str,
        identity_revision: &str,
        bindings: &[(String, Option<PathBuf>)],
        continue_queue: bool,
    ) -> Result<(Vec<WorkflowRun>, Option<WorkflowRun>), String> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| "Workflow coordinator lock is unavailable")?;
        for (task_id, task_state_root) in bindings {
            let run = tasks
                .get_workflow_run(task_id)
                .ok_or_else(|| format!("Workflow not found: {task_id}"))?;
            if run.canonical_identity_key != canonical_identity_key
                || run.identity_revision != identity_revision
            {
                return Err(format!(
                    "Workflow persistence binding does not belong to this queue: {task_id}"
                ));
            }
            let project_root = tasks
                .project_root_for_task(task_id)
                .ok_or_else(|| format!("Workflow has no project root: {task_id}"))?;
            tasks.rebind_workflow_persistence(task_id, &project_root, task_state_root.clone())?;
        }
        if !continue_queue {
            return Ok((
                self.owner_runs(tasks, canonical_identity_key, identity_revision),
                None,
            ));
        }
        for run in self.owner_runs(tasks, canonical_identity_key, identity_revision) {
            if run.display_status == crate::models::workflow::WorkflowDisplayStatus::Queued
                && run.continuation_required
            {
                tasks.set_workflow_queue_state(&run.task_id, run.queue_position, false)?;
            }
        }
        let claimed = self.claim_next_locked(tasks, canonical_identity_key, identity_revision)?;
        Ok((
            self.owner_runs(tasks, canonical_identity_key, identity_revision),
            claimed,
        ))
    }

    pub fn complete_and_claim_next(
        &self,
        tasks: &TaskService,
        task_id: &str,
        result: WorkflowResult,
    ) -> Result<(WorkflowRun, Option<WorkflowRun>), String> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| "Workflow coordinator lock is unavailable")?;
        let owner = tasks
            .get_workflow_run(task_id)
            .ok_or_else(|| format!("Workflow not found: {task_id}"))?;
        if is_terminal(&owner) {
            return Ok((owner, None));
        }
        if cancellation_wins(tasks, task_id) {
            return self.cancel_and_claim_next_locked(tasks, &owner);
        }
        let completed = tasks.complete_workflow(task_id, result)?;
        let next = self.claim_next_locked(
            tasks,
            &owner.canonical_identity_key,
            &owner.identity_revision,
        )?;
        Ok((completed, next))
    }

    pub fn fail_stage_and_claim_next(
        &self,
        tasks: &TaskService,
        task_id: &str,
        stage_id: &str,
        error: WorkflowErrorSummary,
    ) -> Result<(WorkflowRun, Option<WorkflowRun>), String> {
        self.fail_stage_and_claim_next_with_result(tasks, task_id, stage_id, error, None)
    }

    pub(crate) fn fail_stage_and_claim_next_with_result(
        &self,
        tasks: &TaskService,
        task_id: &str,
        stage_id: &str,
        error: WorkflowErrorSummary,
        result: Option<WorkflowResult>,
    ) -> Result<(WorkflowRun, Option<WorkflowRun>), String> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| "Workflow coordinator lock is unavailable")?;
        let owner = tasks
            .get_workflow_run(task_id)
            .ok_or_else(|| format!("Workflow not found: {task_id}"))?;
        if is_terminal(&owner) {
            return Ok((owner, None));
        }
        if cancellation_wins(tasks, task_id) {
            return self.cancel_and_claim_next_locked(tasks, &owner);
        }
        let failed = tasks.fail_workflow_stage_with_result(task_id, stage_id, error, result)?;
        let next = self.claim_next_locked(
            tasks,
            &owner.canonical_identity_key,
            &owner.identity_revision,
        )?;
        Ok((failed, next))
    }

    pub fn finish_cancelled_and_claim_next(
        &self,
        tasks: &TaskService,
        task_id: &str,
    ) -> Result<(WorkflowRun, Option<WorkflowRun>), String> {
        self.finish_cancelled_and_claim_next_with_result(tasks, task_id, None)
    }

    pub(crate) fn finish_cancelled_and_claim_next_with_result(
        &self,
        tasks: &TaskService,
        task_id: &str,
        result: Option<WorkflowResult>,
    ) -> Result<(WorkflowRun, Option<WorkflowRun>), String> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| "Workflow coordinator lock is unavailable")?;
        let owner = tasks
            .get_workflow_run(task_id)
            .ok_or_else(|| format!("Workflow not found: {task_id}"))?;
        if is_terminal(&owner) {
            return Ok((owner, None));
        }
        let cancelled = tasks.finalize_workflow_cancellation_with_result(task_id, result)?;
        let next = self.claim_next_locked(
            tasks,
            &owner.canonical_identity_key,
            &owner.identity_revision,
        )?;
        Ok((cancelled, next))
    }

    /// Permanently cancel a run created by an initial approval that lost a
    /// concurrent cancellation race. Unlike the user-facing queued cancel,
    /// this path deliberately has no undo window: replaying it would bypass
    /// the approval that was just consumed. It also handles the pre-dispatch
    /// Running state so the caller can use one coordinator-owned transition.
    pub fn cancel_created_without_undo_and_claim_next(
        &self,
        tasks: &TaskService,
        task_id: &str,
    ) -> Result<(WorkflowRun, Option<WorkflowRun>), String> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| "Workflow coordinator lock is unavailable")?;
        let owner = tasks
            .get_workflow_run(task_id)
            .ok_or_else(|| format!("Workflow not found: {task_id}"))?;
        if is_terminal(&owner) {
            return Ok((owner, None));
        }
        let cancelled = match owner.display_status {
            crate::models::workflow::WorkflowDisplayStatus::Queued => {
                tasks.mutate_workflow(task_id, |task, workflow| {
                    if task.status != TaskStatus::Queued {
                        return Err(format!(
                            "Workflow is no longer queued for approval cancellation: {task_id}"
                        ));
                    }
                    task.status = TaskStatus::Cancelled;
                    task.cancellable = false;
                    workflow.pending_action = None;
                    workflow.queue_position = None;
                    workflow.continuation_required = false;
                    workflow.cancelled_from_queue = false;
                    workflow.undo_cancel_until = None;
                    workflow.error = None;
                    for stage in &mut workflow.stages {
                        stage.decision = None;
                    }
                    Ok(())
                })?
            }
            crate::models::workflow::WorkflowDisplayStatus::Running
            | crate::models::workflow::WorkflowDisplayStatus::WaitingForConfirmation => {
                tasks.request_workflow_cancel(task_id)?;
                tasks.finalize_workflow_cancellation(task_id)?
            }
            _ => {
                return Err(format!(
                    "Workflow cannot be cancelled from its approval state: {task_id}"
                ))
            }
        };
        self.renumber(
            tasks,
            &owner.canonical_identity_key,
            &owner.identity_revision,
        )?;
        let next = self.claim_next_locked(
            tasks,
            &owner.canonical_identity_key,
            &owner.identity_revision,
        )?;
        Ok((cancelled, next))
    }

    /// Release the queue hold created by `enqueue_for_owner_pending_approval`
    /// and claim it only if the owner queue is otherwise idle. Both changes
    /// occur under the coordinator lock so predecessor completion cannot race
    /// the confirmation disposition.
    pub fn release_initial_approval_hold_and_claim_next(
        &self,
        tasks: &TaskService,
        task_id: &str,
    ) -> Result<(WorkflowRun, Option<WorkflowRun>), String> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| "Workflow coordinator lock is unavailable")?;
        let held = tasks
            .get_workflow_run(task_id)
            .ok_or_else(|| format!("Workflow not found: {task_id}"))?;
        if held.display_status != crate::models::workflow::WorkflowDisplayStatus::Queued
            || !held.continuation_required
        {
            return Err(format!(
                "Workflow is not waiting for initial approval release: {task_id}"
            ));
        }
        let released = tasks.set_workflow_queue_state(task_id, held.queue_position, false)?;
        let claimed =
            self.claim_next_locked(tasks, &held.canonical_identity_key, &held.identity_revision)?;
        let current = claimed
            .as_ref()
            .filter(|run| run.task_id == task_id)
            .cloned()
            .unwrap_or(released);
        Ok((current, claimed))
    }

    pub fn reject_claimed_dispatch(
        &self,
        tasks: &TaskService,
        task_id: &str,
        failure: WorkflowDispatchFailure,
    ) -> Result<(WorkflowRun, Option<WorkflowRun>), String> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| "Workflow coordinator lock is unavailable")?;
        let current = tasks
            .get_workflow_run(task_id)
            .ok_or_else(|| format!("Workflow not found: {task_id}"))?;
        if is_terminal(&current) {
            return Ok((current, None));
        }
        if cancellation_wins(tasks, task_id) {
            return self.cancel_and_claim_next_locked(tasks, &current);
        }
        if current.display_status == crate::models::workflow::WorkflowDisplayStatus::Queued
            && current.continuation_required
        {
            return Ok((current, None));
        }
        let (status, error) = match failure {
            WorkflowDispatchFailure::Identity(error) => (TaskStatus::Interrupted, error),
            WorkflowDispatchFailure::Stale(error) => (TaskStatus::Failed, error),
            WorkflowDispatchFailure::Invariant(error) => (TaskStatus::Interrupted, error),
        };
        let rejected = tasks.reject_workflow_dispatch(task_id, status, error)?;
        let next = self.claim_next_locked(
            tasks,
            &current.canonical_identity_key,
            &current.identity_revision,
        )?;
        Ok((rejected, next))
    }

    pub fn freeze_owner_for_trust_revocation(
        &self,
        tasks: &TaskService,
        project_root: &std::path::Path,
    ) -> Result<WorkflowTrustTransition, String> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| "Workflow coordinator lock is unavailable")?;
        let runs = tasks
            .list_workflow_runs()
            .into_iter()
            .filter(|run| tasks.task_belongs_to_root(&run.task_id, project_root))
            .collect::<Vec<_>>();
        let current_identity = super::persistence::project_identity(project_root).ok();
        let project_is_readable = local_quick_project_is_readable(project_root);
        let mut transition = WorkflowTrustTransition::default();
        for run in &runs {
            if run.display_status == crate::models::workflow::WorkflowDisplayStatus::Queued {
                if let Err(error) =
                    tasks.set_workflow_queue_state(&run.task_id, run.queue_position, true)
                {
                    transition.errors.push(error);
                }
            }
        }
        for run in runs {
            if !matches!(
                run.display_status,
                crate::models::workflow::WorkflowDisplayStatus::Running
                    | crate::models::workflow::WorkflowDisplayStatus::WaitingForConfirmation
            ) {
                continue;
            }
            let local_quick = project_is_readable
                && current_identity.as_ref().is_some_and(|identity| {
                    identity.canonical_identity_key == run.canonical_identity_key
                        && identity.identity_revision == run.identity_revision
                })
                && matches!(
                    run.scope,
                    WorkflowScope::HealthCheck {
                        mode: crate::models::workflow::HealthCheckMode::LocalQuick
                    }
                )
                && run.operation == crate::models::workflow::WorkflowOperation::BuiltIn
                && run.display_status == crate::models::workflow::WorkflowDisplayStatus::Running;
            if local_quick {
                transition.continued_local_quick_runs.push(run);
                continue;
            }
            let was_waiting = run.display_status
                == crate::models::workflow::WorkflowDisplayStatus::WaitingForConfirmation;
            transition.stopped_runs.push(run.clone());
            if let Err(error) = tasks.request_workflow_cancel(&run.task_id) {
                transition.errors.push(error);
                continue;
            }
            if was_waiting {
                if let Err(error) = tasks.finalize_workflow_cancellation(&run.task_id) {
                    transition.errors.push(error);
                }
            }
        }
        Ok(transition)
    }

    pub fn interrupt_invalid_confirmation(
        &self,
        tasks: &TaskService,
        task_id: &str,
        error: WorkflowErrorSummary,
    ) -> Result<(WorkflowRun, Option<WorkflowRun>), String> {
        self.interrupt_invalid_confirmation_with_result(tasks, task_id, error, None)
    }

    pub(crate) fn interrupt_invalid_confirmation_with_result(
        &self,
        tasks: &TaskService,
        task_id: &str,
        error: WorkflowErrorSummary,
        result: Option<WorkflowResult>,
    ) -> Result<(WorkflowRun, Option<WorkflowRun>), String> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| "Workflow coordinator lock is unavailable")?;
        let current = tasks
            .get_workflow_run(task_id)
            .ok_or_else(|| format!("Workflow not found: {task_id}"))?;
        if is_terminal(&current) {
            return Ok((current, None));
        }
        if cancellation_wins(tasks, task_id) {
            return self.cancel_and_claim_next_locked(tasks, &current);
        }
        let interrupted =
            tasks.interrupt_workflow_confirmation_with_result(task_id, error, result)?;
        let next = self.claim_next_locked(
            tasks,
            &current.canonical_identity_key,
            &current.identity_revision,
        )?;
        Ok((interrupted, next))
    }

    fn cancel_and_claim_next_locked(
        &self,
        tasks: &TaskService,
        owner: &WorkflowRun,
    ) -> Result<(WorkflowRun, Option<WorkflowRun>), String> {
        let cancelled = tasks.finalize_workflow_cancellation(&owner.task_id)?;
        let next = self.claim_next_locked(
            tasks,
            &owner.canonical_identity_key,
            &owner.identity_revision,
        )?;
        Ok((cancelled, next))
    }

    pub fn cancel(&self, tasks: &TaskService, task_id: &str) -> Result<WorkflowRun, String> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| "Workflow coordinator lock is unavailable")?;
        let run = tasks
            .get_workflow_run(task_id)
            .ok_or_else(|| format!("Workflow not found: {task_id}"))?;
        match run.display_status {
            crate::models::workflow::WorkflowDisplayStatus::Queued => {
                let deadline = (Utc::now() + Duration::seconds(UNDO_WINDOW_SECONDS)).to_rfc3339();
                let cancelled = tasks.mutate_workflow(task_id, |task, workflow| {
                    task.status = TaskStatus::Cancelled;
                    workflow.cancelled_from_queue = true;
                    workflow.undo_cancel_until = Some(deadline);
                    Ok(())
                })?;
                self.renumber(tasks, &run.canonical_identity_key, &run.identity_revision)?;
                Ok(cancelled)
            }
            crate::models::workflow::WorkflowDisplayStatus::Running
            | crate::models::workflow::WorkflowDisplayStatus::WaitingForConfirmation => {
                tasks.request_workflow_cancel(task_id)
            }
            _ => Ok(run),
        }
    }

    pub fn undo_cancel(
        &self,
        tasks: &TaskService,
        task_id: &str,
    ) -> Result<(WorkflowRun, Option<WorkflowRun>), String> {
        self.undo_cancel_inner(tasks, task_id, false)
    }

    /// Restore an approved repair behind a queue hold. The caller must first
    /// restore its app-owned authorization receipt and then release the hold;
    /// no predecessor completion can claim the task in between.
    pub fn undo_cancel_pending_approval(
        &self,
        tasks: &TaskService,
        task_id: &str,
    ) -> Result<(WorkflowRun, Option<WorkflowRun>), String> {
        self.undo_cancel_inner(tasks, task_id, true)
    }

    fn undo_cancel_inner(
        &self,
        tasks: &TaskService,
        task_id: &str,
        pending_approval: bool,
    ) -> Result<(WorkflowRun, Option<WorkflowRun>), String> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| "Workflow coordinator lock is unavailable")?;
        let run = tasks
            .get_workflow_run(task_id)
            .ok_or_else(|| format!("Workflow not found: {task_id}"))?;
        if matches!(
            run.display_status,
            crate::models::workflow::WorkflowDisplayStatus::Queued
                | crate::models::workflow::WorkflowDisplayStatus::Running
        ) {
            return Ok((run, None));
        }
        let restored = tasks.mutate_workflow(task_id, |task, workflow| {
            let unexpired = workflow
                .undo_cancel_until
                .as_deref()
                .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                .is_some_and(|deadline| deadline >= Utc::now());
            if task.status != TaskStatus::Cancelled || !workflow.cancelled_from_queue || !unexpired
            {
                return Err("Queued workflow cancellation can no longer be undone".into());
            }
            task.status = TaskStatus::Queued;
            task.completed_at = None;
            workflow.cancelled_from_queue = false;
            workflow.undo_cancel_until = None;
            workflow.continuation_required = pending_approval;
            Ok(())
        })?;
        tasks.reset_workflow_cancellation(task_id)?;
        self.renumber(tasks, &run.canonical_identity_key, &run.identity_revision)?;
        if pending_approval {
            return Ok((restored, None));
        }
        let claimed =
            self.claim_next_locked(tasks, &run.canonical_identity_key, &run.identity_revision)?;
        let current = if claimed.as_ref().is_some_and(|next| next.task_id == task_id) {
            claimed.as_ref().cloned().unwrap_or(restored)
        } else {
            restored
        };
        Ok((current, claimed))
    }

    pub fn reorder_queued(
        &self,
        tasks: &TaskService,
        task_id: &str,
        before_task_id: Option<&str>,
    ) -> Result<Vec<WorkflowRun>, String> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| "Workflow coordinator lock is unavailable")?;
        let target = tasks
            .get_workflow_run(task_id)
            .ok_or_else(|| format!("Workflow not found: {task_id}"))?;
        if target.display_status != crate::models::workflow::WorkflowDisplayStatus::Queued {
            return Err("Only queued workflows can be reordered".into());
        }
        let mut queued = self
            .owner_runs(
                tasks,
                &target.canonical_identity_key,
                &target.identity_revision,
            )
            .into_iter()
            .filter(|run| {
                run.display_status == crate::models::workflow::WorkflowDisplayStatus::Queued
            })
            .collect::<Vec<_>>();
        queued.sort_by_key(|run| {
            (
                run.queue_position.unwrap_or(u32::MAX),
                run.started_at.clone(),
            )
        });
        queued.retain(|run| run.task_id != task_id);
        let insertion = match before_task_id {
            Some(before) => queued
                .iter()
                .position(|run| run.task_id == before)
                .ok_or_else(|| "Queue anchor is not in the same project queue".to_string())?,
            None => queued.len(),
        };
        queued.insert(insertion, target);
        for (index, run) in queued.iter().enumerate() {
            tasks.set_workflow_queue_state(
                &run.task_id,
                Some((index + 1) as u32),
                run.continuation_required,
            )?;
        }
        Ok(self.owner_runs(
            tasks,
            &queued[0].canonical_identity_key,
            &queued[0].identity_revision,
        ))
    }

    pub fn retry(
        &self,
        tasks: &TaskService,
        task_id: &str,
        project_id: String,
        project_root: PathBuf,
        task_state_root: Option<PathBuf>,
    ) -> Result<WorkflowStartOutcome, String> {
        self.retry_with_hold(
            tasks,
            task_id,
            project_id,
            project_root,
            task_state_root,
            false,
        )
    }

    pub fn retry_pending_approval(
        &self,
        tasks: &TaskService,
        task_id: &str,
        project_id: String,
        project_root: PathBuf,
        task_state_root: Option<PathBuf>,
    ) -> Result<WorkflowStartOutcome, String> {
        self.retry_with_hold(
            tasks,
            task_id,
            project_id,
            project_root,
            task_state_root,
            true,
        )
    }

    fn retry_with_hold(
        &self,
        tasks: &TaskService,
        task_id: &str,
        project_id: String,
        project_root: PathBuf,
        task_state_root: Option<PathBuf>,
        pending_approval: bool,
    ) -> Result<WorkflowStartOutcome, String> {
        let original = tasks
            .get_workflow_run(task_id)
            .ok_or_else(|| format!("Workflow not found: {task_id}"))?;
        let completed_generate = original.display_status
            == crate::models::workflow::WorkflowDisplayStatus::Completed
            && original.kind == WorkflowKind::GenerateContent;
        if !completed_generate
            && !matches!(
                original.display_status,
                crate::models::workflow::WorkflowDisplayStatus::Failed
                    | crate::models::workflow::WorkflowDisplayStatus::Interrupted
            )
        {
            return Err(
                "Only failed/interrupted workflows or completed Generate Content workflows can be retried"
                    .into(),
            );
        }
        let retry_identity = project_identity(&project_root)?;
        if retry_identity.canonical_identity_key != original.canonical_identity_key
            || retry_identity.identity_revision != original.identity_revision
        {
            return Err("Retry project root does not match the original workflow".into());
        }
        let attempt_number = original
            .retry
            .as_ref()
            .map_or(2, |retry| retry.attempt_number + 1);
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| "Workflow coordinator lock is unavailable")?;
        let mut execution_options = tasks
            .workflow_execution_options(task_id)
            .ok_or_else(|| format!("Workflow execution options missing: {task_id}"))?;
        execution_options.preparation_fingerprint = None;
        let mut baseline_fingerprint = original.baseline_fingerprint.clone();
        let was_persistent = tasks.workflow_persistence_dir(task_id).is_some()
            || original.persistence_transition
                == Some(WorkflowPersistenceTransition::DowngradedToMemoryOnly);
        let is_persistent = task_state_root.is_some();
        if completed_generate {
            let context =
                crate::models::paths::ProjectContext::new(project_id.clone(), project_root.clone())
                    .with_resolved_layout()
                    .map_err(|error| error.message)?;
            if let WorkflowScope::GenerateContent {
                output_path: Some(output_path),
                ..
            } = &original.scope
            {
                execution_options.existing_target_hash = crate::services::FileStore
                    .file_hash_if_exists(&context, output_path)
                    .map_err(|error| error.message)?;
            }
            baseline_fingerprint =
                super::preparation::workflow_baseline_for_scope(&context, &original.scope)
                    .map_err(|error| error.message)?
                    .fingerprint;
        }
        let outcome = self.enqueue_locked(
            tasks,
            EnqueueWorkflow {
                project_id,
                project_root,
                task_state_root,
                title: format!("Retry {:?}", original.kind),
                kind: original.kind,
                scope: original.scope,
                route: original.route,
                baseline_fingerprint,
                execution_options,
                stages: original.stages.into_iter().map(reset_stage).collect(),
                retry: Some(WorkflowRetryLink {
                    attempt_of: original
                        .retry
                        .map_or(original.task_id, |retry| retry.attempt_of),
                    attempt_number,
                }),
            },
            false,
            Some((
                &original.canonical_identity_key,
                &original.identity_revision,
            )),
            pending_approval,
        )?;
        match outcome {
            WorkflowStartOutcome::Created { run } => {
                tasks.record_workflow_persistence_transition(
                    &run.task_id,
                    was_persistent,
                    is_persistent,
                )?;
                Ok(WorkflowStartOutcome::Created {
                    run: tasks.get_workflow_run(&run.task_id).ok_or_else(|| {
                        format!("Workflow not found after retry: {}", run.task_id)
                    })?,
                })
            }
            existing => Ok(existing),
        }
    }

    fn owner_runs(&self, tasks: &TaskService, key: &str, revision: &str) -> Vec<WorkflowRun> {
        tasks
            .list_workflow_runs()
            .into_iter()
            .filter(|run| run.canonical_identity_key == key && run.identity_revision == revision)
            .collect()
    }

    fn renumber(&self, tasks: &TaskService, key: &str, revision: &str) -> Result<(), String> {
        let mut queued = self
            .owner_runs(tasks, key, revision)
            .into_iter()
            .filter(|run| {
                run.display_status == crate::models::workflow::WorkflowDisplayStatus::Queued
            })
            .collect::<Vec<_>>();
        queued.sort_by_key(|run| {
            (
                run.queue_position.unwrap_or(u32::MAX),
                run.started_at.clone(),
            )
        });
        for (index, run) in queued.into_iter().enumerate() {
            let position = (index + 1) as u32;
            if run.queue_position != Some(position) {
                tasks.set_workflow_queue_state(
                    &run.task_id,
                    Some(position),
                    run.continuation_required,
                )?;
            }
        }
        Ok(())
    }
}

fn local_quick_project_is_readable(project_root: &std::path::Path) -> bool {
    let Ok(context) = crate::models::paths::ProjectContext::new(
        "workflow-trust-revocation",
        project_root.to_path_buf(),
    )
    .with_resolved_layout() else {
        return false;
    };
    let roles = [
        crate::models::layout::ProjectMarkdownRootRole::Wiki,
        crate::models::layout::ProjectMarkdownRootRole::Source,
        crate::models::layout::ProjectMarkdownRootRole::Mixed,
    ];
    context
        .list_markdown_files_for_roles(&roles)
        .is_ok_and(|files| {
            files
                .into_iter()
                .all(|path| std::fs::File::open(path).is_ok())
        })
}

fn reset_stage(mut stage: WorkflowStage) -> WorkflowStage {
    stage.status = crate::models::workflow::WorkflowStageStatus::Pending;
    stage.started_at = None;
    stage.completed_at = None;
    stage.current_item = None;
    stage.progress = None;
    stage.decision = None;
    stage
}

fn dispatch_error(
    code: impl Into<String>,
    message_key: impl Into<String>,
    recoverable: bool,
) -> WorkflowErrorSummary {
    WorkflowErrorSummary {
        code: code.into(),
        message_key: message_key.into(),
        recoverable,
        user_action_required: true,
        suggested_action: Some(crate::models::workflow::WorkflowPrerequisiteAction::PrepareAgain),
        project_mutation_state: WorkflowProjectMutationState::Unknown,
    }
}

fn cancellation_wins(tasks: &TaskService, task_id: &str) -> bool {
    tasks.is_cancelled(task_id)
        || tasks
            .get_task(task_id)
            .is_some_and(|task| task.status == TaskStatus::Cancelling)
}

fn is_terminal(run: &WorkflowRun) -> bool {
    matches!(
        run.display_status,
        crate::models::workflow::WorkflowDisplayStatus::Completed
            | crate::models::workflow::WorkflowDisplayStatus::Failed
            | crate::models::workflow::WorkflowDisplayStatus::Cancelled
            | crate::models::workflow::WorkflowDisplayStatus::Interrupted
    )
}

fn canonical_task_state_root(
    asserted_root: &std::path::Path,
    canonical_root: &std::path::Path,
    task_state_root: &std::path::Path,
) -> Result<PathBuf, String> {
    let relative = task_state_root
        .strip_prefix(asserted_root)
        .or_else(|_| task_state_root.strip_prefix(canonical_root))
        .map_err(|_| "Task state root must be inside the asserted project root")?;
    if relative.components().any(|component| {
        matches!(
            component,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    }) {
        return Err("Task state root is not a safe project-relative path".into());
    }
    let candidate = canonical_root.join(relative);
    let mut existing_ancestor = candidate.as_path();
    while !existing_ancestor.exists() {
        existing_ancestor = existing_ancestor
            .parent()
            .ok_or_else(|| "Task state root has no existing project ancestor".to_string())?;
    }
    let canonical_ancestor = existing_ancestor
        .canonicalize()
        .map_err(|error| format!("Task state root ancestor is unavailable: {error}"))?;
    if !canonical_ancestor.starts_with(canonical_root) {
        return Err("Task state root resolves outside the canonical project root".into());
    }
    Ok(candidate)
}
