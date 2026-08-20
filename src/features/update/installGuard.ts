import type { BackendTask } from "../../types/task";
import type { ImportSession } from "../../types/importV2";
import type { WorkflowRun } from "../../types/workflow";
import type { UpdateInstallGuardSnapshot } from "../../types/update";

export interface UpdateInstallGuardInput {
  editor: {
    mode: "read" | "edit" | "preview";
    saveState: "idle" | "saving" | "saved" | "conflict" | "error";
    draft: string;
    savedMarkdown: string | null;
  };
  importSession: ImportSession | null;
  importConfirming: boolean;
  workflowRuns: readonly WorkflowRun[];
  tasks: readonly BackendTask[];
  projectPendingAction: boolean;
  lintPendingConfirmation: boolean;
}

const isActiveTask = (task: BackendTask): boolean =>
  task.status === "queued" || task.status === "running" || task.status === "cancelling";

export function collectUpdateInstallGuard(
  input: UpdateInstallGuardInput,
): UpdateInstallGuardSnapshot {
  const blockers: UpdateInstallGuardSnapshot["blockers"] = [];
  const unsavedEditor = input.editor.savedMarkdown !== null
    && input.editor.draft !== input.editor.savedMarkdown;
  if (input.editor.saveState === "saving") blockers.push("editor_saving");
  else if (unsavedEditor || input.editor.saveState === "conflict") blockers.push("unsaved_editor");

  const importCommitActive = input.importConfirming
    || input.importSession?.items.some((item) => item.status === "committing") === true;
  if (importCommitActive) blockers.push("import_commit");

  const applyingWorkflowTaskIds = new Set(input.workflowRuns.filter((run) =>
    run.displayStatus === "running"
    && Boolean(
      run.currentStageId === "apply_changes"
      || run.currentStageId?.startsWith("apply_changes_"),
    ),
  ).map((run) => run.taskId));
  const workflowApplyActive = applyingWorkflowTaskIds.size > 0;
  if (workflowApplyActive) blockers.push("workflow_apply");

  const criticalTaskActive = input.tasks.some(
    (task) => isActiveTask(task) && (!task.cancellable || task.operation?.kind === "import_commit"),
  );
  if (criticalTaskActive) blockers.push("critical_task");

  const pendingUserConfirmation = input.projectPendingAction
    || input.lintPendingConfirmation
    || input.importSession?.status === "waiting_for_confirmation"
    || input.tasks.some((task) => task.status === "waiting_for_confirmation")
    || input.workflowRuns.some((run) => run.displayStatus === "waiting_for_confirmation");
  if (pendingUserConfirmation) blockers.push("pending_confirmation");

  const safeRunningTaskCount = input.tasks.filter(
    (task) => isActiveTask(task)
      && task.cancellable
      && task.operation?.kind !== "import_commit"
      && !applyingWorkflowTaskIds.has(task.id),
  ).length;

  return {
    blockers,
    safeRunningTaskCount,
    request: {
      unsavedEditor: unsavedEditor || input.editor.saveState === "saving" || input.editor.saveState === "conflict",
      importCommitActive,
      pendingUserConfirmation,
    },
  };
}
