import { useCallback, useRef } from "react";
import { useTranslation } from "react-i18next";

import { translateBackendError } from "../lib/backendError";
import { cancelTaskRequest } from "../stores/taskStore";
import { useToastStore } from "../stores/toastStore";
import type { ProjectSummary } from "../types/project";

export interface TaskCancelOptions {
  suppressToast?: boolean;
}

export interface TaskLauncher {
  /** Resolves true when the backend accepted the cancellation request. */
  cancel: (taskId: string, options?: TaskCancelOptions) => Promise<boolean>;
}

export function useTaskLauncher(project: ProjectSummary): TaskLauncher {
  const { t } = useTranslation();
  const projectId = project.projectId;
  const rootPath = project.rootPath;
  const projectKey = `${projectId}\0${rootPath}`;
  const latestProjectKey = useRef(projectKey);
  latestProjectKey.current = projectKey;
  const pushToast = useToastStore((state) => state.pushToast);

  const cancel = useCallback(
    async (taskId: string, options: TaskCancelOptions = {}) => {
      const requestKey = projectKey;
      try {
        await cancelTaskRequest(taskId);
        return true;
      } catch (error) {
        if (latestProjectKey.current !== requestKey) return false;
        if (!options.suppressToast) {
          pushToast(
            "error",
            t("task.cancelError", { message: translateBackendError(error, t) }),
          );
        }
        return false;
      }
    },
    [projectKey, pushToast, t],
  );

  return { cancel };
}
