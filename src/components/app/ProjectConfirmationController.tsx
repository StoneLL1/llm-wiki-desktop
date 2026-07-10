import { invoke } from "@tauri-apps/api/core";
import { useCallback } from "react";
import { useTranslation } from "react-i18next";

import { useTaskLauncher } from "../../hooks/useTaskLauncher";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import { useToastStore } from "../../stores/toastStore";
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
  const { t } = useTranslation();
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
  const pushToast = useToastStore((state) => state.pushToast);
  const { startCompile } = useTaskLauncher(currentProject);
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

  const confirmProjectAction = useCallback(async () => {
    const action = pendingAction;
    const requestProjectId = currentProject.projectId;
    const requestRootPath = currentProject.rootPath;
    const confirmed = await confirmPendingAction();
    if (
      !confirmed ||
      !action ||
      (action.actionType !== "delete_source" &&
        action.actionType !== "replace_source")
    ) {
      return;
    }
    const latestProject = useProjectStore.getState().currentProject;
    if (
      latestProject.projectId !== requestProjectId ||
      latestProject.rootPath !== requestRootPath
    ) {
      return;
    }
    try {
      await startCompile({ route: "auto", agent: null, provider: null });
    } catch (error) {
      pushToast(
        "error",
        t("import.sourceCompileError", { message: errorMessage(error) }),
      );
    }
  }, [confirmPendingAction, currentProject.projectId, currentProject.rootPath, pendingAction, pushToast, startCompile, t]);

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
          void confirmProjectAction();
        } else {
          void submitCompileConfirmation(true);
        }
      }}
    />
  );
}
