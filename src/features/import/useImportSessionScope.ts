import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { importV2Api } from "../../services/importV2Api";
import { importProjectKey, useImportStore } from "../../stores/importStore";
import { useToastStore } from "../../stores/toastStore";
import type { AppView } from "../../stores/navigationStore";
import type { ImportFrontendReadiness } from "../../types/importV2Presentation";
import type { ProjectSummary } from "../../types/project";
import type { ImportBootstrapState } from "./importWorkflow";

export const hasImportTauriRuntime = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export function importWorkflowErrorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") {
      return message;
    }
  }
  return String(error);
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
  const projectKey = importProjectKey(projectId, rootPath);
  const latestProjectKey = useRef(projectKey);
  latestProjectKey.current = projectKey;

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
    (requestKey: string) => latestProjectKey.current === requestKey,
    [],
  );
  const isScopeCurrent = useCallback(
    (requestKey: string, epoch: number, expectedSessionId?: string) => {
      const current = useImportStore.getState();
      return latestProjectKey.current === requestKey
        && current.projectKey === requestKey
        && current.sessionEpoch === epoch
        && (!expectedSessionId || current.session?.sessionId === expectedSessionId);
    },
    [],
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

    const request = Promise.all([
      importV2Api.getSession({ projectId, projectRootPath: rootPath, sessionId }),
      importV2Api.getReadiness({ projectId, projectRootPath: rootPath })
        .then((nextReadiness) => ({ nextReadiness, warning: null as string | null }))
        .catch((error) => ({
          nextReadiness: null,
          warning: importWorkflowErrorMessage(error),
        })),
    ]);
    const refresh = request
      .then(([nextSession, readinessResult]) => {
        if (isScopeCurrent(requestKey, epoch)) {
          if (readinessResult.nextReadiness) setReadiness(readinessResult.nextReadiness);
          setReadinessWarning(readinessResult.warning);
        }
        if (isScopeCurrent(requestKey, epoch) && sessionMutationRevisionRef.current === refreshRevision) {
          useImportStore.getState().replaceSession(requestKey, nextSession, epoch);
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

        let nextSession;
        if (nextReadiness?.unfinishedSessionId) {
          try {
            nextSession = await importV2Api.getSession({
              projectId,
              projectRootPath: rootPath,
              sessionId: nextReadiness.unfinishedSessionId,
            });
          } catch (error) {
            if (cancelled || !isScopeCurrent(projectKey, epoch)) return;
            setReadinessWarning(importWorkflowErrorMessage(error));
            nextSession = await importV2Api.createSession({
              projectId,
              projectRootPath: rootPath,
              resourceMode: "balanced",
            });
          }
        } else {
          nextSession = await importV2Api.createSession({
            projectId,
            projectRootPath: rootPath,
            resourceMode: "balanced",
          });
        }
        if (cancelled || !isScopeCurrent(projectKey, epoch)) return;
        useImportStore.getState().attachSession(projectKey, nextSession, epoch);
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
  }, [activeView, bootstrapAttempt, isScopeCurrent, projectId, projectKey, pushToast, rootPath, t]);

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
  if (session.status === "completed" || session.status === "cancelled") return true;
  return session.items.length > 0 && session.items.every(
    (item) => item.status === "completed" || item.status === "skipped" || item.status === "cancelled",
  );
}
