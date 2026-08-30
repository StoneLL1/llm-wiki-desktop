import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";

import {
  normalizeBackendError,
  type NormalizedBackendError,
} from "../../lib/backendError";
import { importV2Api } from "../../services/importV2Api";
import { importProjectKey, useImportStore, type ImportQueueFilter } from "../../stores/importStore";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import { useToastStore } from "../../stores/toastStore";
import type { AppView } from "../../stores/navigationStore";
import type { ImportFrontendReadiness } from "../../types/importV2Presentation";
import type { ProjectSessionAuthority, ProjectSummary } from "../../types/project";
import type { ImportBootstrapState } from "./importWorkflow";

export const hasImportTauriRuntime = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

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

export function normalizeImportWorkflowError(error: unknown): NormalizedBackendError {
  return normalizeBackendError(error, {
    defaultSummaryKey: "backendError.summary.import",
    defaultRecoverable: true,
    defaultActionKind: "retry",
  });
}

export function importWorkflowErrorMessage(error: unknown, t: TFunction): string {
  const normalized = normalizeImportWorkflowError(error);
  return t(normalized.summaryKey, normalized.summaryParams);
}

export async function loadConsistentSessionWindow(
  projectId: string,
  projectRootPath: string,
  sessionId: string,
  filter: ImportQueueFilter,
) {
  let lastError: unknown = null;
  for (let attempt = 0; attempt < 3; attempt += 1) {
    try {
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
      if (
        overview.projectId === projectId
        && overview.sessionId === sessionId
        && page.sessionId === sessionId
        && overview.semanticRevision === page.snapshotRevision
      ) return { overview, page, filter };
      lastError = new Error("Import session identity or revision changed while its first page was loading.");
    } catch (error) {
      lastError = error;
    }
  }
  throw lastError ?? new Error("Import session changed while its first page was loading.");
}

export interface ImportSessionScope {
  projectId: string;
  rootPath: string;
  projectKey: string;
  readiness: ImportFrontendReadiness | null;
  readinessWarning: NormalizedBackendError | null;
  readinessRetrying: boolean;
  recoveryWarning: NormalizedBackendError | null;
  recoveryRetrying: boolean;
  bootstrapError: NormalizedBackendError | null;
  bootstrapState: ImportBootstrapState;
  isSyncingSession: boolean;
  retryBootstrap: () => void;
  retryReadiness: () => Promise<void>;
  retryRecovery: () => Promise<void>;
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
  const [readinessWarning, setReadinessWarning] = useState<NormalizedBackendError | null>(null);
  const [readinessRetrying, setReadinessRetrying] = useState(false);
  const [recoveryWarning, setRecoveryWarning] = useState<NormalizedBackendError | null>(null);
  const [recoveryRetrying, setRecoveryRetrying] = useState(false);
  const [recoverySessionId, setRecoverySessionId] = useState<string | null>(null);
  const [bootstrapError, setBootstrapError] = useState<NormalizedBackendError | null>(null);
  const [bootstrapState, setBootstrapState] = useState<ImportBootstrapState>("loading");
  const [bootstrapAttempt, setBootstrapAttempt] = useState(0);
  const [isSyncingSession, setIsSyncingSession] = useState(false);
  const refreshInFlight = useRef<{ scopeKey: string; promise: Promise<void> } | null>(null);
  const sessionMutationRevisionRef = useRef(0);
  const readinessRequestRevisionRef = useRef(0);
  const recoveryRequestRevisionRef = useRef(0);

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

  const retryReadiness = useCallback(async () => {
    const requestKey = projectKey;
    const epoch = useImportStore.getState().sessionEpoch;
    const requestRevision = ++readinessRequestRevisionRef.current;
    if (!isScopeCurrent(requestKey, epoch)) return;
    setReadinessRetrying(true);
    try {
      const next = await importV2Api.getReadiness({ projectId, projectRootPath: rootPath });
      if (readinessRequestRevisionRef.current !== requestRevision || !isScopeCurrent(requestKey, epoch)) return;
      setReadiness(next);
      setReadinessWarning(null);
    } catch (error) {
      if (readinessRequestRevisionRef.current === requestRevision && isScopeCurrent(requestKey, epoch)) {
        setReadinessWarning(normalizeImportWorkflowError(error));
      }
    } finally {
      if (readinessRequestRevisionRef.current === requestRevision && isScopeCurrent(requestKey, epoch)) {
        setReadinessRetrying(false);
      }
    }
  }, [isScopeCurrent, projectId, projectKey, rootPath]);

  const startRecoveryForScope = useCallback(async (
    requestKey: string,
    epoch: number,
    sessionId: string,
  ) => {
    const requestRevision = ++recoveryRequestRevisionRef.current;
    if (!isScopeCurrent(requestKey, epoch, sessionId)) return;
    setRecoverySessionId(sessionId);
    setRecoveryWarning(null);
    setRecoveryRetrying(true);
    setIsSyncingSession(true);
    try {
      const task = await importV2Api.startSessionRecovery({
        projectId,
        projectRootPath: rootPath,
        sessionId,
      });
      const taskStore = useTaskStore.getState();
      if (
        recoveryRequestRevisionRef.current === requestRevision
        && isScopeCurrent(requestKey, epoch, sessionId)
      ) {
        taskStore.upsertTask(task);
      } else {
        taskStore.recordTaskFact(task);
      }
    } catch (error) {
      if (
        recoveryRequestRevisionRef.current === requestRevision
        && isScopeCurrent(requestKey, epoch, sessionId)
      ) {
        setIsSyncingSession(false);
        setRecoveryWarning(normalizeImportWorkflowError(error));
      }
    } finally {
      if (
        recoveryRequestRevisionRef.current === requestRevision
        && isScopeCurrent(requestKey, epoch, sessionId)
      ) {
        setRecoveryRetrying(false);
      }
    }
  }, [isScopeCurrent, projectId, rootPath]);

  const retryRecovery = useCallback(async () => {
    const current = useImportStore.getState();
    if (!recoverySessionId || current.session?.sessionId !== recoverySessionId) return;
    await startRecoveryForScope(projectKey, current.sessionEpoch, recoverySessionId);
  }, [projectKey, recoverySessionId, startRecoveryForScope]);

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
    const request = loadConsistentSessionWindow(projectId, rootPath, sessionId, filter);
    const refresh = request
      .then((window) => {
        if (
          isScopeCurrent(requestKey, epoch)
          && useImportStore.getState().filter === window.filter
          && sessionMutationRevisionRef.current === refreshRevision
        ) {
          if (!useImportStore.getState().attachSessionWindow(requestKey, window.overview, window.page, epoch)) {
            refreshAgain = true;
          }
        } else if (isScopeCurrent(requestKey, epoch)) {
          refreshAgain = true;
        }
      })
      .catch((error) => {
        if (isScopeCurrent(requestKey, epoch)) {
          pushToast("error", t("importV2.workflow.error", { message: importWorkflowErrorMessage(error, t) }));
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
    readinessRequestRevisionRef.current += 1;
    recoveryRequestRevisionRef.current += 1;
    setReadiness(null);
    setReadinessWarning(null);
    setReadinessRetrying(false);
    setRecoveryWarning(null);
    setRecoveryRetrying(false);
    setRecoverySessionId(null);
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
          setReadinessWarning(normalizeImportWorkflowError(error));
        }
        if (cancelled || !isScopeCurrent(projectKey, epoch)) return;
        setReadiness(nextReadiness);

        let sessionId: string;
        if (nextReadiness?.unfinishedSessionId) {
          sessionId = nextReadiness.unfinishedSessionId;
          const { overview, page, filter } = await loadConsistentSessionWindow(
            projectId,
            rootPath,
            sessionId,
            useImportStore.getState().filter,
          );
          if (cancelled || !isScopeCurrent(projectKey, epoch) || useImportStore.getState().filter !== filter) return;
          useImportStore.getState().attachSessionWindow(projectKey, overview, page, epoch);
          if (overview.recoveryRequired || overview.indexState === "rebuild_required") {
            void startRecoveryForScope(projectKey, epoch, sessionId);
          }
        } else {
          const created = await importV2Api.createSession({
            projectId,
            projectRootPath: rootPath,
            resourceMode: "balanced",
          });
          sessionId = created.sessionId;
          const { overview, page, filter } = await loadConsistentSessionWindow(
            projectId,
            rootPath,
            sessionId,
            useImportStore.getState().filter,
          );
          if (cancelled || !isScopeCurrent(projectKey, epoch) || useImportStore.getState().filter !== filter) return;
          useImportStore.getState().attachSessionWindow(projectKey, overview, page, epoch);
        }
        setBootstrapState("ready");
      } catch (error) {
        if (cancelled || !isScopeCurrent(projectKey, epoch)) return;
        setBootstrapError(normalizeImportWorkflowError(error));
        setBootstrapState("error");
        pushToast("error", t("importV2.workflow.error", { message: importWorkflowErrorMessage(error, t) }));
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [activeView, bootstrapAttempt, expectedAuthorityRevisionKey, isScopeCurrent, projectId, projectKey, pushToast, rootPath, startRecoveryForScope, t]);

  return {
    projectId,
    rootPath,
    projectKey,
    readiness,
    readinessWarning,
    readinessRetrying,
    recoveryWarning,
    recoveryRetrying,
    bootstrapError,
    bootstrapState,
    isSyncingSession,
    retryBootstrap,
    retryReadiness,
    retryRecovery,
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
