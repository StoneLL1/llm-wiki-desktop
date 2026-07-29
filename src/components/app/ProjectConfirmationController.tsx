import { invoke } from "@tauri-apps/api/core";
import { useCallback } from "react";

import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import type { BackendTask } from "../../types/task";
import { CompileConflictDialog } from "./CompileConflictDialog";
import { ConfirmationDialog } from "./ConfirmationDialog";

export function ProjectConfirmationController() {
  const currentProject = useProjectStore((state) => state.currentProject);
  const pendingAction = useProjectStore((state) => state.pendingAction);
  const confirmPendingAction = useProjectStore(
    (state) => state.confirmPendingAction,
  );
  const cancelPendingAction = useProjectStore(
    (state) => state.cancelPendingAction,
  );
  const tasks = useTaskStore((state) => state.tasks);
  const upsertTask = useTaskStore((state) => state.upsertTask);
  const compilePendingAction = tasks.find(
    (task) =>
      task.projectId === currentProject.projectId &&
      task.status === "waiting_for_confirmation" &&
      task.result?.pendingAction,
  )?.result?.pendingAction;
  const displayedPendingAction = pendingAction ?? compilePendingAction;

  const submitCompileConfirmation = useCallback(
    async (confirmed: boolean) => {
      if (!compilePendingAction) return;
      const task = await invoke<BackendTask>("confirm_compile_action", {
        request: { actionId: compilePendingAction.id, confirmed },
      });
      upsertTask(task);
    },
    [compilePendingAction, upsertTask],
  );

  if (!displayedPendingAction) return null;

  if (
    displayedPendingAction.actionType === "merge_conflict" &&
    compilePendingAction
  ) {
    return (
      <CompileConflictDialog
        action={displayedPendingAction}
        onCancel={() => {
          void submitCompileConfirmation(false);
        }}
        onResolved={upsertTask}
      />
    );
  }

  return (
    <ConfirmationDialog
      action={displayedPendingAction}
      checkpointExists={displayedPendingAction.checkpointHash != null}
      onCancel={() => {
        if (pendingAction) {
          void cancelPendingAction();
        } else {
          void submitCompileConfirmation(false);
        }
      }}
      onConfirm={() => {
        if (pendingAction) {
          void confirmPendingAction();
        } else {
          void submitCompileConfirmation(true);
        }
      }}
    />
  );
}
