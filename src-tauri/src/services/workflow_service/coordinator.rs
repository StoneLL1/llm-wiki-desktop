use std::path::PathBuf;
use std::sync::Mutex;

use crate::models::task::TaskStatus;
use crate::models::workflow::{
    WorkflowErrorSummary, WorkflowExecutionOptions, WorkflowExecutionState, WorkflowKind,
    WorkflowPersistenceMode, WorkflowPersistenceTransition, WorkflowResult, WorkflowRetryLink,
    WorkflowRoute, WorkflowRun, WorkflowScope, WorkflowStage, WorkflowStartOutcome,
    WORKFLOW_SCHEMA_VERSION,
};
use crate::tasks::TaskService;
use chrono::{Duration, Utc};

use super::fingerprint::workflow_fingerprint;
use super::persistence::project_identity;

const UNDO_WINDOW_SECONDS: i64 = 10;

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
        self.enqueue_locked(tasks, request, true)
    }

    fn enqueue_locked(
        &self,
        tasks: &TaskService,
        request: EnqueueWorkflow,
        deduplicate: bool,
    ) -> Result<WorkflowStartOutcome, String> {
        let identity = project_identity(&request.project_root)?;
        request.execution_options.validate()?;
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
            continuation_required: false,
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
        if !has_active && queued_count == 0 {
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
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| "Workflow coordinator lock is unavailable")?;
        let owner = tasks
            .get_workflow_run(task_id)
            .ok_or_else(|| format!("Workflow not found: {task_id}"))?;
        let failed = tasks.fail_workflow_stage(task_id, stage_id, error)?;
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
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| "Workflow coordinator lock is unavailable")?;
        let owner = tasks
            .get_workflow_run(task_id)
            .ok_or_else(|| format!("Workflow not found: {task_id}"))?;
        let cancelled = tasks.transition_workflow_status(task_id, TaskStatus::Cancelled)?;
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
            Ok(())
        })?;
        tasks.reset_workflow_cancellation(task_id)?;
        self.renumber(tasks, &run.canonical_identity_key, &run.identity_revision)?;
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
        project_root: PathBuf,
        task_state_root: Option<PathBuf>,
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
        let mut baseline_fingerprint = original.baseline_fingerprint.clone();
        let was_persistent = tasks.workflow_persistence_dir(task_id).is_some()
            || original.persistence_transition
                == Some(WorkflowPersistenceTransition::DowngradedToMemoryOnly);
        let is_persistent = task_state_root.is_some();
        if completed_generate {
            let context = crate::models::paths::ProjectContext::new(
                original.project_id.clone(),
                project_root.clone(),
            )
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
                project_id: original.project_id.clone(),
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

fn reset_stage(mut stage: WorkflowStage) -> WorkflowStage {
    stage.status = crate::models::workflow::WorkflowStageStatus::Pending;
    stage.started_at = None;
    stage.completed_at = None;
    stage.current_item = None;
    stage.progress = None;
    stage.decision = None;
    stage
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
