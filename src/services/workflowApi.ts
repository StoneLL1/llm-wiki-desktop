import { invoke } from "@tauri-apps/api/core";

import type {
  ConfirmWorkflowActionRequest,
  ListWorkflowRunsRequest,
  PrepareWorkflowRequest,
  ReorderQueuedWorkflowRequest,
  StartWorkflowRequest,
  WorkflowPreparation,
  WorkflowProjectRequest,
  WorkflowRun,
  WorkflowFileDiffPage,
  WorkflowFileDiffRequest,
  WorkflowRunHistoryPage,
  WorkflowRunPage,
  WorkflowRunRequest,
  WorkflowStartOutcome,
  WorkflowsOverview,
} from "../types/workflow";

export function getWorkflowsOverview(
  request: WorkflowProjectRequest,
): Promise<WorkflowsOverview> {
  return invoke<WorkflowsOverview>("get_workflows_overview", { request });
}

export function prepareWorkflow(
  request: PrepareWorkflowRequest,
): Promise<WorkflowPreparation> {
  return invoke<WorkflowPreparation>("prepare_workflow", { request });
}

export function startWorkflow(
  request: StartWorkflowRequest,
): Promise<WorkflowStartOutcome> {
  return invoke<WorkflowStartOutcome>("start_workflow", { request });
}

export function listWorkflowRuns(
  request: ListWorkflowRunsRequest,
): Promise<WorkflowRunHistoryPage> {
  return invoke<WorkflowRunHistoryPage>("list_workflow_runs", { request });
}

export function getWorkflowRun(
  request: WorkflowRunRequest,
): Promise<WorkflowRun> {
  return invoke<WorkflowRun>("get_workflow_run", { request });
}

export function getWorkflowFileDiff(
  request: WorkflowFileDiffRequest,
): Promise<WorkflowFileDiffPage> {
  return invoke<WorkflowFileDiffPage>("get_workflow_file_diff", { request });
}

export function cancelWorkflowRun(
  request: WorkflowRunRequest,
): Promise<WorkflowRun> {
  return invoke<WorkflowRun>("cancel_workflow_run", { request });
}

export function undoCancelQueuedWorkflow(
  request: WorkflowRunRequest,
): Promise<WorkflowRun> {
  return invoke<WorkflowRun>("undo_cancel_queued_workflow", { request });
}

export function reorderQueuedWorkflow(
  request: ReorderQueuedWorkflowRequest,
): Promise<WorkflowRunPage> {
  return invoke<WorkflowRunPage>("reorder_queued_workflow", { request });
}

export function continueQueuedWorkflows(
  request: WorkflowProjectRequest,
): Promise<WorkflowRunPage> {
  return invoke<WorkflowRunPage>("continue_queued_workflows", { request });
}

export function retryWorkflow(
  request: WorkflowRunRequest,
): Promise<WorkflowStartOutcome> {
  return invoke<WorkflowStartOutcome>("retry_workflow", { request });
}

export function confirmWorkflowAction(
  request: ConfirmWorkflowActionRequest,
): Promise<WorkflowRun> {
  return invoke<WorkflowRun>("confirm_workflow_action", { request });
}

export function discardWorkflowResult(
  request: WorkflowRunRequest,
): Promise<WorkflowRun> {
  return invoke<WorkflowRun>("discard_workflow_result", { request });
}
