import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";

import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import type { BackendTask } from "../../types/task";
import { CompileConflictDialog } from "./CompileConflictDialog";
import { ConfirmationDialog } from "./ConfirmationDialog";

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(error);
}

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
      task.taskType === "wiki_compile" &&
      task.status === "waiting_for_confirmation" &&
      task.result?.pendingAction,
  )?.result?.pendingAction;
  const displayedPendingAction = pendingAction ?? compilePendingAction;
  const [submitting, setSubmitting] = useState(false);
  const [submissionError, setSubmissionError] = useState<string | null>(null);

  useEffect(() => {
    setSubmitting(false);
    setSubmissionError(null);
  }, [displayedPendingAction?.id]);

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

  const submitConfirmation = async (confirmed: boolean) => {
    setSubmitting(true);
    setSubmissionError(null);
    try {
      if (pendingAction) {
        if (confirmed) {
          await confirmPendingAction();
        } else {
          await cancelPendingAction();
        }
      } else {
        await submitCompileConfirmation(confirmed);
      }
    } catch (error) {
      setSubmissionError(errorMessage(error));
    } finally {
      setSubmitting(false);
    }
  };

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
      busy={submitting}
      checkpointExists={displayedPendingAction.checkpointHash != null}
      error={submissionError}
      onCancel={() => {
        void submitConfirmation(false);
      }}
      onConfirm={() => {
        void submitConfirmation(true);
      }}
    />
  );
}
