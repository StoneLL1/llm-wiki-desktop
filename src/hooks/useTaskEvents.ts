import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useProjectStore } from "../stores/projectStore";
import { useToastStore } from "../stores/toastStore";
import i18next from "i18next";
import {
  invalidateNotificationPermissionEpoch,
  notifyTaskEvent,
  registerNotificationActionListener,
} from "../services/notifications";
import {
  clearPendingTaskEvents,
  dispatchTaskEvent,
  registerTaskEventOwner,
  retainTaskEventProject,
} from "../services/taskEventDispatcher";
import {
  handleTaskEvent,
  recoverTasksForProject,
} from "../stores/taskStore";
import {
  ensureProjectFacts,
  invalidateProjectFacts,
  projectFactsAuthorityKey,
  projectFactsAuthorityMatches,
} from "../stores/projectFactsStore";
import type { BackendEvent } from "../types/task";
import type { ProjectSummary } from "../types/project";
import {
  captureProjectScope,
  invalidateObservedProjectResourcesOnFocus,
  invalidateProjectResources,
  isProjectScopeCurrent,
} from "../stores/projectScope";
import { translateBackendError } from "../lib/backendError";

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const TASK_EVENT_CHANNELS = [
  "task://updated",
  "task://log",
  "task://completed",
  "task://failed",
  "task://cancelled",
  "task://activity",
  "task://stream-output",
  "workflow://updated",
  "confirmation://requested",
  "project://refreshed",
  "wiki://changed",
  "graph://updated",
  "agent://output",
  "import://session-patch",
] as const;

function isProjectSummary(payload: unknown): payload is ProjectSummary {
  return typeof payload === "object"
    && payload !== null
    && "projectId" in payload
    && "rootPath" in payload
    && "inventoryState" in payload;
}

export function isTaskEventForProject(event: BackendEvent, projectId: string): boolean {
  return event.projectId === projectId;
}

/**
 * Subscribes to all backend task/event channels and keeps the task store in sync.
 * Also recovers persisted tasks when the active project root changes, so background
 * work survives view switches and app restarts, and fires OS notifications for
 * completion/failure/confirmation events.
 */
export function useTaskEvents(): void {
  const currentProject = useProjectStore((state) => state.currentProject);
  const pushToast = useToastStore((state) => state.pushToast);

  useEffect(() => {
    if (!hasTauri()) return;
    const unlisteners: Array<() => void> = [];
    let cancelled = false;

    const unregisterStoreListener = registerTaskEventOwner((event) => {
      const projectState = useProjectStore.getState();
      const activeProject = projectState.currentProject;
      const scopeEpoch = captureProjectScope();
      // The owner records every valid task snapshot for audit/recovery. Only
      // current-project presentation effects continue past the scope guard.
      handleTaskEvent(event);
      if (!isTaskEventForProject(event, activeProject.projectId)) return;
      const authority = projectState.authority;
      const scope = { projectId: activeProject.projectId, rootPath: activeProject.rootPath };
      const authorityKey = authority
        && authority.projectId === activeProject.projectId
        && activeProject.rootPath
        ? projectFactsAuthorityKey(authority)
        : null;
      const factsScopeMatches = authorityKey !== null
        && projectFactsAuthorityMatches(scope, authorityKey);
      if (event.eventType === "project_refreshed" && isProjectSummary(event.payload)) {
        if (
          factsScopeMatches
          && activeProject.rootPath === event.payload.rootPath
        ) {
          useProjectStore.getState().setCurrentProject(event.payload);
          invalidateProjectFacts(scope, ["git"], "project_refreshed");
          void ensureProjectFacts(scope, ["git"]).catch(() => undefined);
        }
      }
      void import("../services/projectResourceInvalidation").then((service) => {
        if (!isProjectScopeCurrent(scopeEpoch)) return;
        invalidateProjectResources(
          scope,
          service.projectResourcesForBackendEvent(event),
          true,
        );
        const latestAuthority = useProjectStore.getState().authority;
        const authorityStillMatches = factsScopeMatches
          && latestAuthority?.projectId === scope.projectId
          && projectFactsAuthorityKey(latestAuthority) === authorityKey
          && projectFactsAuthorityMatches(scope, authorityKey);
        if (authorityStillMatches && service.gitFactsChangedForBackendEvent(event)) {
          invalidateProjectFacts(scope, ["git"], "task_affected_paths");
          void ensureProjectFacts(scope, ["git"]).catch(() => undefined);
        }
      });
      if (event.eventType !== "workflow_updated") void notifyTaskEvent(event);
    });

    for (const channel of TASK_EVENT_CHANNELS) {
      listen<BackendEvent>(channel, (evt) => {
        if (cancelled) return;
        const event = evt.payload as BackendEvent;
        const activeProject = useProjectStore.getState().currentProject;
        if (
          event.eventType === "workflow_updated"
          && isTaskEventForProject(event, activeProject.projectId)
        ) void notifyTaskEvent(event);
        dispatchTaskEvent(event);
      })
        .then((unlisten) => {
          if (cancelled) {
            unlisten();
          } else {
            unlisteners.push(unlisten);
          }
        })
        .catch(() => {
          // Tauri event system unavailable (browser-only dev)
        });
    }

    registerNotificationActionListener()
      .then((unlisten) => {
        if (cancelled) unlisten();
        else unlisteners.push(unlisten);
      })
      .catch(() => {
        // Notification actions are unavailable in browser-only development.
      });

    const refreshPermissionEpoch = () => {
      invalidateNotificationPermissionEpoch();
      const activeProject = useProjectStore.getState().currentProject;
      invalidateObservedProjectResourcesOnFocus({
        projectId: activeProject.projectId,
        rootPath: activeProject.rootPath,
      });
    };
    window.addEventListener("focus", refreshPermissionEpoch);

    return () => {
      cancelled = true;
      const activeProjectId = useProjectStore.getState().currentProject.projectId;
      clearPendingTaskEvents((event) => event.projectId === activeProjectId);
      unregisterStoreListener();
      window.removeEventListener("focus", refreshPermissionEpoch);
      unlisteners.forEach((fn) => fn());
    };
  }, []);

  useEffect(() => {
    retainTaskEventProject(currentProject.projectId);
  }, [currentProject.projectId]);

  // Recover persisted tasks whenever the active project root changes.
  useEffect(() => {
    if (currentProject.rootPath) {
      recoverTasksForProject(currentProject.projectId, currentProject.rootPath).catch((error) => {
        pushToast("error", i18next.t("task.recoverError", {
          message: translateBackendError(error, i18next.t.bind(i18next)),
        }));
      });
    }
  }, [currentProject.projectId, currentProject.rootPath, pushToast]);
}
