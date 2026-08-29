import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { importV2Api } from "../../services/importV2Api";
import { importProjectKey, useImportStore, type ImportQueueFilter } from "../../stores/importStore";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import { useToastStore } from "../../stores/toastStore";
import type { AppView } from "../../stores/navigationStore";
import type { ImportSession, ImportSessionOverview } from "../../types/importV2";
import type { ImportFrontendReadiness } from "../../types/importV2Presentation";
import type { ProjectSessionAuthority, ProjectSummary } from "../../types/project";
import type { ImportBootstrapState } from "./importWorkflow";

export const hasImportTauriRuntime = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function createdSessionOverview(session: ImportSession): ImportSessionOverview {
  const activeStatuses = new Set(["queued", "inspecting", "extracting", "validating", "committing"]);
  const readyItems = session.items.filter((item) => item.status === "preview_ready");
  const selected = readyItems.filter((item) => item.selected && item.preview?.quality.level !== "fail");
  return {
    ...session,
    itemCount: session.items.length,
    semanticRevision: 1,
    selectionRevision: 1,
    confirmationDigest: `created:${session.sessionId}`,
    counts: {
      all: session.items.filter((item) => item.status !== "completed" && item.status !== "skipped").length,
      active: session.items.filter((item) => activeStatuses.has(item.status)).length,
      ready: readyItems.length,
      needsAction: session.items.filter((item) => ["waiting_capability", "waiting_login", "waiting_authorization", "needs_merge"].includes(item.status)).length,
      failed: session.items.filter((item) => item.status === "failed").length,
      completed: session.items.filter((item) => item.status === "completed").length,
      waiting: session.items.filter((item) => ["waiting_capability", "waiting_login", "waiting_authorization"].includes(item.status)).length,
      processed: session.items.filter((item) => ["preview_ready", "needs_merge", "completed", "failed", "cancelled", "skipped"].includes(item.status)).length,
      cancelled: session.items.filter((item) => item.status === "cancelled").length,
    },
    statusCounts: {
      queued: session.items.filter((item) => item.status === "queued").length,
      inspecting: session.items.filter((item) => item.status === "inspecting").length,
      waitingCapability: session.items.filter((item) => item.status === "waiting_capability").length,
      waitingLogin: session.items.filter((item) => item.status === "waiting_login").length,
      waitingAuthorization: session.items.filter((item) => item.status === "waiting_authorization").length,
      extracting: session.items.filter((item) => item.status === "extracting").length,
      validating: session.items.filter((item) => item.status === "validating").length,
      previewReady: session.items.filter((item) => item.status === "preview_ready").length,
      needsMerge: session.items.filter((item) => item.status === "needs_merge").length,
      committing: session.items.filter((item) => item.status === "committing").length,
      completed: session.items.filter((item) => item.status === "completed").length,
      paused: session.items.filter((item) => item.status === "paused").length,
      cancelled: session.items.filter((item) => item.status === "cancelled").length,
      skipped: session.items.filter((item) => item.status === "skipped").length,
      failed: session.items.filter((item) => item.status === "failed").length,
    },
    selection: {
      selected: selected.length,
      newSources: selected.filter((item) => !item.preview?.resolution || item.preview.resolution.kind === "new_source").length,
      updates: selected.filter((item) => item.preview?.resolution?.kind === "same_source_new_version" || item.preview?.resolution?.kind === "needs_three_way_merge").length,
      warnings: selected.filter((item) => item.preview?.quality.level === "warning").length,
      pending: session.items.filter((item) => ["failed", "needs_merge", "waiting_capability", "waiting_login", "waiting_authorization"].includes(item.status)).length,
      restricted: selected.filter((item) => item.restrictedContent).length,
    },
    indexState: "ready",
    recoveryRequired: false,
    recoveryReasons: [],
  };
}

function attachCreatedSession(projectKey: string, epoch: number, session: ImportSession): void {
  const overview = createdSessionOverview(session);
  useImportStore.getState().attachSessionWindow(projectKey, overview, {
    sessionId: session.sessionId,
    snapshotRevision: overview.semanticRevision,
    items: session.items.slice(0, 200),
    nextCursor: session.items.length > 200 ? "created-session-window" : null,
    total: session.items.length,
  }, epoch);
}

function authorityRevisionKey(
  authority: ProjectSessionAuthority | null,
  projectId: string,
): string {
  if (
    !authority
    || authority.projectId !== projectId
  ) return "unresolved";
  return [
    authority.canonicalIdentityKey,
    authority.canonicalRootPath,
    authority.identityRevision,
    authority.authorityRevision,
  ].join("\0");
}

export function importWorkflowErrorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") {
      return message;
    }
  }
  return String(error);
}

async function loadConsistentSessionWindow(
  projectId: string,
  projectRootPath: string,
  sessionId: string,
  filter: ImportQueueFilter,
) {
  for (let attempt = 0; attempt < 3; attempt += 1) {
    const [overview, page] = await Promise.all([
      importV2Api.getSessionOverview({ projectId, projectRootPath, sessionId }),
      importV2Api.listSessionItems({
        projectId,
        projectRootPath,
        sessionId,
        filter,
        limit: 200,
      }),
    ]);
    if (overview.semanticRevision === page.snapshotRevision) return { overview, page };
  }
  throw new Error("Import session changed while its first page was loading.");
}

export interface ImportSessionScope {
  projectId: string;
  rootPath: string;
  projectKey: string;
  readiness: ImportFrontendReadiness | null;
  readinessWarning: string | null;
  bootstrapError: string | null;
  bootstrapState: ImportBootstrapState;
  isSyncingSession: boolean;
  retryBootstrap: () => void;
  isProjectCurrent: (requestKey: string) => boolean;
  isScopeCurrent: (requestKey: string, epoch: number, expectedSessionId?: string) => boolean;
  nextSessionMutationRevision: () => number;
  isSessionMutationRevisionCurrent: (revision: number) => boolean;
  refreshForScope: (requestKey: string, epoch: number, expectedSessionId?: string) => Promise<void>;
}

export function useImportSessionScope(
  project: ProjectSummary,
  activeView: AppView,
): ImportSessionScope {
  const { t } = useTranslation();
  const pushToast = useToastStore((state) => state.pushToast);
  const projectId = project.projectId;
  const rootPath = project.rootPath;
  const authority = useProjectStore((state) => state.authority);
  const expectedAuthorityRevisionKey = authorityRevisionKey(authority, projectId);
  const projectKey = importProjectKey(projectId, rootPath);
  const latestProjectKey = useRef(projectKey);
  latestProjectKey.current = projectKey;
  const latestAuthorityRevisionKey = useRef(expectedAuthorityRevisionKey);
  latestAuthorityRevisionKey.current = expectedAuthorityRevisionKey;

  const [readiness, setReadiness] = useState<ImportFrontendReadiness | null>(null);
  const [readinessWarning, setReadinessWarning] = useState<string | null>(null);
  const [bootstrapError, setBootstrapError] = useState<string | null>(null);
  const [bootstrapState, setBootstrapState] = useState<ImportBootstrapState>("loading");
  const [bootstrapAttempt, setBootstrapAttempt] = useState(0);
  const [isSyncingSession, setIsSyncingSession] = useState(false);
  const refreshInFlight = useRef<{ scopeKey: string; promise: Promise<void> } | null>(null);
  const sessionMutationRevisionRef = useRef(0);

  const retryBootstrap = useCallback(() => setBootstrapAttempt((attempt) => attempt + 1), []);
  const isProjectCurrent = useCallback(
    (requestKey: string) => latestProjectKey.current === requestKey
      && latestAuthorityRevisionKey.current === expectedAuthorityRevisionKey,
    [expectedAuthorityRevisionKey],
  );
  const isScopeCurrent = useCallback(
    (requestKey: string, epoch: number, expectedSessionId?: string) => {
      const current = useImportStore.getState();
      return latestProjectKey.current === requestKey
        && latestAuthorityRevisionKey.current === expectedAuthorityRevisionKey
        && current.projectKey === requestKey
        && current.sessionEpoch === epoch
        && (!expectedSessionId || current.session?.sessionId === expectedSessionId);
    },
    [expectedAuthorityRevisionKey],
  );
  const nextSessionMutationRevision = useCallback(() => {
    sessionMutationRevisionRef.current += 1;
    return sessionMutationRevisionRef.current;
  }, []);
  const isSessionMutationRevisionCurrent = useCallback(
    (revision: number) => sessionMutationRevisionRef.current === revision,
    [],
  );

  const refreshForScope = useCallback(async (
    requestKey: string,
    epoch: number,
    expectedSessionId?: string,
  ) => {
    if (!isScopeCurrent(requestKey, epoch)) return;
    const currentSession = useImportStore.getState().session;
    const sessionId = expectedSessionId ?? currentSession?.sessionId;
    if (!sessionId) return;

    const scopeKey = `${requestKey}\0${epoch}\0${sessionId}`;
    if (refreshInFlight.current?.scopeKey === scopeKey) return refreshInFlight.current.promise;
    const refreshRevision = sessionMutationRevisionRef.current;
    let refreshAgain = false;
    setIsSyncingSession(true);

    const filter = useImportStore.getState().filter;
    const request = Promise.all([
      loadConsistentSessionWindow(projectId, rootPath, sessionId, filter),
      importV2Api.getReadiness({ projectId, projectRootPath: rootPath })
        .then((nextReadiness) => ({ nextReadiness, warning: null as string | null }))
        .catch((error) => ({
          nextReadiness: null,
          warning: importWorkflowErrorMessage(error),
        })),
    ]);
    const refresh = request
      .then(([window, readinessResult]) => {
        if (isScopeCurrent(requestKey, epoch)) {
          if (readinessResult.nextReadiness) setReadiness(readinessResult.nextReadiness);
          setReadinessWarning(readinessResult.warning);
        }
        if (isScopeCurrent(requestKey, epoch) && sessionMutationRevisionRef.current === refreshRevision) {
          if (!useImportStore.getState().attachSessionWindow(requestKey, window.overview, window.page, epoch)) {
            refreshAgain = true;
          }
        } else if (isScopeCurrent(requestKey, epoch)) {
          refreshAgain = true;
        }
      })
      .catch((error) => {
        if (isScopeCurrent(requestKey, epoch)) {
          pushToast("error", t("importV2.workflow.error", { message: importWorkflowErrorMessage(error) }));
        }
        throw error;
      });

    refreshInFlight.current = { scopeKey, promise: refresh };
    try {
      await refresh;
      if (refreshAgain && isScopeCurrent(requestKey, epoch, sessionId)) {
        refreshInFlight.current = null;
        await refreshForScope(requestKey, epoch, sessionId);
      }
    } finally {
      if (refreshInFlight.current?.promise === refresh) {
        refreshInFlight.current = null;
        if (isScopeCurrent(requestKey, epoch, sessionId)) setIsSyncingSession(false);
      }
    }
  }, [isScopeCurrent, projectId, pushToast, rootPath, t]);

  useEffect(() => {
    if (!projectId) {
      setBootstrapState("ready");
      return;
    }
    if (activeView !== "import") {
      setBootstrapState("ready");
      return;
    }

    const currentStore = useImportStore.getState();
    if (
      currentStore.projectKey === projectKey
      && currentStore.session?.projectId === projectId
      && !matchesEndedSession(currentStore.session)
    ) {
      setBootstrapState("ready");
      return;
    }

    currentStore.resetProjectPresentation(projectKey);
    const epoch = currentStore.beginSessionEpoch(projectKey);
    setReadiness(null);
    setReadinessWarning(null);
    setBootstrapError(null);
    setIsSyncingSession(false);
    setBootstrapState("loading");

    if (!hasImportTauriRuntime()) {
      setBootstrapState("blocked");
      return;
    }

    let cancelled = false;
    void (async () => {
      try {
        let nextReadiness: ImportFrontendReadiness | null = null;
        try {
          nextReadiness = await importV2Api.getReadiness({ projectId, projectRootPath: rootPath });
        } catch (error) {
          if (cancelled || !isScopeCurrent(projectKey, epoch)) return;
          setReadinessWarning(importWorkflowErrorMessage(error));
        }
        if (cancelled || !isScopeCurrent(projectKey, epoch)) return;
        setReadiness(nextReadiness);

        let sessionId: string;
        let attached = false;
        if (nextReadiness?.unfinishedSessionId) {
          try {
            sessionId = nextReadiness.unfinishedSessionId;
            const { overview, page } = await loadConsistentSessionWindow(
              projectId,
              rootPath,
              sessionId,
              useImportStore.getState().filter,
            );
            if (cancelled || !isScopeCurrent(projectKey, epoch)) return;
            useImportStore.getState().attachSessionWindow(projectKey, overview, page, epoch);
            if (overview.recoveryRequired || overview.indexState === "rebuild_required") {
              setIsSyncingSession(true);
              void importV2Api
                .startSessionRecovery({
                  projectId,
                  projectRootPath: rootPath,
                  sessionId: nextReadiness.unfinishedSessionId,
                })
                .then((task) => {
                  const taskStore = useTaskStore.getState();
                  if (!cancelled && isScopeCurrent(projectKey, epoch)) {
                    taskStore.upsertTask(task);
                  } else {
                    taskStore.recordTaskFact(task);
                  }
                })
                .catch((error) => {
                  if (!cancelled && isScopeCurrent(projectKey, epoch)) {
                    setIsSyncingSession(false);
                    setReadinessWarning(importWorkflowErrorMessage(error));
                  }
                });
            }
          } catch (error) {
            if (cancelled || !isScopeCurrent(projectKey, epoch)) return;
            setReadinessWarning(importWorkflowErrorMessage(error));
            const created = await importV2Api.createSession({
              projectId,
              projectRootPath: rootPath,
              resourceMode: "balanced",
            });
            sessionId = created.sessionId;
            attachCreatedSession(projectKey, epoch, created);
            attached = true;
          }
        } else {
          const created = await importV2Api.createSession({
            projectId,
            projectRootPath: rootPath,
            resourceMode: "balanced",
          });
          sessionId = created.sessionId;
          attachCreatedSession(projectKey, epoch, created);
          attached = true;
        }
        if (cancelled || !isScopeCurrent(projectKey, epoch)) return;
        if (!attached && useImportStore.getState().session?.sessionId !== sessionId) {
          const { overview, page } = await loadConsistentSessionWindow(
            projectId,
            rootPath,
            sessionId,
            useImportStore.getState().filter,
          );
          if (cancelled || !isScopeCurrent(projectKey, epoch)) return;
          useImportStore.getState().attachSessionWindow(projectKey, overview, page, epoch);
        }
        setBootstrapState("ready");
      } catch (error) {
        if (cancelled || !isScopeCurrent(projectKey, epoch)) return;
        setBootstrapError(importWorkflowErrorMessage(error));
        setBootstrapState("error");
        pushToast("error", t("importV2.workflow.error", { message: importWorkflowErrorMessage(error) }));
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [activeView, bootstrapAttempt, expectedAuthorityRevisionKey, isScopeCurrent, projectId, projectKey, pushToast, rootPath, t]);

  return {
    projectId,
    rootPath,
    projectKey,
    readiness,
    readinessWarning,
    bootstrapError,
    bootstrapState,
    isSyncingSession,
    retryBootstrap,
    isProjectCurrent,
    isScopeCurrent,
    nextSessionMutationRevision,
    isSessionMutationRevisionCurrent,
    refreshForScope,
  };
}

function matchesEndedSession(session: { status: string; items: readonly { status: string }[] }): boolean {
  return session.status === "completed" || session.status === "cancelled";
}
