import { invoke } from "@tauri-apps/api/core";
import { useCallback, useRef } from "react";
import { useTranslation } from "react-i18next";

import { cancelTaskRequest, useTaskStore } from "../stores/taskStore";
import { useToastStore } from "../stores/toastStore";
import type { AgentKind } from "../types/agent";
import type { ExportType } from "../types/export";
import type { LlmProviderKind } from "../types/llm";
import type { ProjectSummary } from "../types/project";
import type { BackendTask } from "../types/task";

export interface TaskLaunchOptions {
  route: "auto" | "agent" | "byok";
  agent: AgentKind | null;
  provider: LlmProviderKind | null;
}

export interface TaskCancelOptions {
  suppressToast?: boolean;
}

export interface TaskLauncher {
  startCompile: (
    options?: Partial<TaskLaunchOptions>,
  ) => Promise<BackendTask>;
  startDeepLint: (options: TaskLaunchOptions) => Promise<BackendTask>;
  startExport: (
    exportType: ExportType,
    sourcePath: string | null,
    options: TaskLaunchOptions,
  ) => Promise<BackendTask>;
  /** Resolves true when the backend accepted the cancellation request. */
  cancel: (taskId: string, options?: TaskCancelOptions) => Promise<boolean>;
}

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(error);
}

const defaultLaunchOptions: TaskLaunchOptions = {
  route: "auto",
  agent: null,
  provider: null,
};

export function useTaskLauncher(project: ProjectSummary): TaskLauncher {
  const { t } = useTranslation();
  const projectId = project.projectId;
  const rootPath = project.rootPath;
  const projectKey = `${projectId}\0${rootPath}`;
  const latestProjectKey = useRef(projectKey);
  latestProjectKey.current = projectKey;
  const upsertTask = useTaskStore((state) => state.upsertTask);
  const openTaskDrawer = useTaskStore((state) => state.openDrawer);
  const pushToast = useToastStore((state) => state.pushToast);

  const track = useCallback(
    (task: BackendTask, requestKey: string) => {
      upsertTask(task);
      if (latestProjectKey.current === requestKey) {
        openTaskDrawer(task.id);
      }
      return task;
    },
    [openTaskDrawer, upsertTask],
  );

  const startCompile = useCallback(
    async (options: Partial<TaskLaunchOptions> = {}) => {
      const task = await invoke<BackendTask>("start_wiki_compile", {
        request: {
          projectId,
          projectRootPath: rootPath,
          ...defaultLaunchOptions,
          ...options,
        },
      });
      return track(task, projectKey);
    },
    [projectId, projectKey, rootPath, track],
  );

  const startDeepLint = useCallback(
    async (options: TaskLaunchOptions) => {
      const task = await invoke<BackendTask>("start_deep_lint", {
        request: {
          projectId,
          projectRootPath: rootPath,
          ...options,
        },
      });
      return track(task, projectKey);
    },
    [projectId, projectKey, rootPath, track],
  );

  const startExport = useCallback(
    async (
      exportType: ExportType,
      sourcePath: string | null,
      options: TaskLaunchOptions,
    ) => {
      const task = await invoke<BackendTask>("start_export", {
        request: {
          projectId,
          projectRootPath: rootPath,
          exportType,
          sourcePath,
          ...options,
        },
      });
      return track(task, projectKey);
    },
    [projectId, projectKey, rootPath, track],
  );

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
            t("task.cancelError", { message: errorMessage(error) }),
          );
        }
        return false;
      }
    },
    [projectKey, pushToast, t],
  );

  return { startCompile, startDeepLint, startExport, cancel };
}
