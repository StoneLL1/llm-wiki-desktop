use crate::models::workflow::{
    WorkflowErrorSummary, WorkflowPendingAction, WorkflowResult, WorkflowRun,
};
use crate::tasks::TaskService;

use super::WorkflowCoordinator;

pub struct WorkflowStageSink<'a> {
    tasks: &'a TaskService,
    coordinator: &'a WorkflowCoordinator,
    task_id: &'a str,
}

impl<'a> WorkflowStageSink<'a> {
    pub fn new(
        tasks: &'a TaskService,
        coordinator: &'a WorkflowCoordinator,
        task_id: &'a str,
    ) -> Self {
        Self {
            tasks,
            coordinator,
            task_id,
        }
    }

    pub fn start(&self, stage_id: &str) -> Result<WorkflowRun, String> {
        self.tasks.start_workflow_stage(self.task_id, stage_id)
    }

    pub fn progress(
        &self,
        stage_id: &str,
        current_item: Option<String>,
        current: u64,
        total: Option<u64>,
    ) -> Result<WorkflowRun, String> {
        self.tasks.update_workflow_stage_progress(
            self.task_id,
            stage_id,
            current_item,
            current,
            total,
        )
    }

    pub fn complete(&self, stage_id: &str) -> Result<WorkflowRun, String> {
        self.tasks.complete_workflow_stage(self.task_id, stage_id)
    }

    pub fn skip(&self, stage_id: &str) -> Result<WorkflowRun, String> {
        self.tasks.skip_workflow_stage(self.task_id, stage_id)
    }

    pub fn fail(
        &self,
        stage_id: &str,
        error: WorkflowErrorSummary,
    ) -> Result<(WorkflowRun, Option<WorkflowRun>), String> {
        self.coordinator
            .fail_stage_and_claim_next(self.tasks, self.task_id, stage_id, error)
    }

    pub fn wait(
        &self,
        stage_id: &str,
        pending: WorkflowPendingAction,
    ) -> Result<WorkflowRun, String> {
        self.tasks
            .wait_workflow_stage(self.task_id, stage_id, pending)
    }

    pub fn finish(
        &self,
        result: WorkflowResult,
    ) -> Result<(WorkflowRun, Option<WorkflowRun>), String> {
        self.coordinator
            .complete_and_claim_next(self.tasks, self.task_id, result)
    }
}
