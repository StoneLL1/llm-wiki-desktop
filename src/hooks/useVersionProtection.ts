import { useCallback, useEffect, useRef, useState } from "react";

import { normalizeBackendError, type NormalizedBackendError } from "../lib/backendError";
import { getVersionStatus, prepareVersionAction, confirmVersionAction } from "../services/versionHistoryApi";
import { captureProjectScope, isProjectScopeCurrent } from "../stores/projectScope";
import { useProjectStore } from "../stores/projectStore";

/** Project-bound protection checks and explicitly consented enablement. */
export function useVersionProtection(projectId: string, rootPath: string) {
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState<NormalizedBackendError | null>(null);
  const epoch = useRef(0);
  const busy = useRef(false);

  useEffect(() => {
    busy.current = false;
    setChecking(false);
    setError(null);
    return () => { epoch.current += 1; };
  }, [projectId, rootPath]);

  const check = useCallback(async () => {
    if (busy.current) return false;
    busy.current = true;
    setChecking(true);
    setError(null);
    const requestEpoch = ++epoch.current;
    const scope = captureProjectScope();
    const authority = useProjectStore.getState().authority;
    let status: Awaited<ReturnType<typeof getVersionStatus>> | null = null;
    let failure: unknown = null;
    try { status = await getVersionStatus({ projectId, projectRootPath: rootPath }); }
    catch (error) { failure = error; }
    const current = useProjectStore.getState();
    const isCurrent = requestEpoch === epoch.current
      && isProjectScopeCurrent(scope)
      && current.currentProject.projectId === projectId
      && current.currentProject.rootPath === rootPath
      && current.authority?.canonicalIdentityKey === authority?.canonicalIdentityKey
      && current.authority?.identityRevision === authority?.identityRevision;
    if (requestEpoch === epoch.current) {
      busy.current = false;
      setChecking(false);
    }
    if (!isCurrent) return false;
    if (!status) {
      setError(normalizeBackendError(failure, {
        defaultSummaryKey: "lint.git.unavailable",
        actionKindOverride: null,
      }));
      return false;
    }
    if (!status.enabled) {
      setError(normalizeBackendError({ code: "VERSION_NOT_ENABLED" }));
      return false;
    }
    return true;
  }, [projectId, rootPath]);

  const enable = useCallback(async () => {
    if (busy.current) return false;
    busy.current = true;
    setChecking(true);
    const scope = captureProjectScope();
    const authority = useProjectStore.getState().authority;
    const requestEpoch = ++epoch.current;
    const current = () => {
      const latest = useProjectStore.getState();
      return requestEpoch === epoch.current && isProjectScopeCurrent(scope)
        && latest.currentProject.projectId === projectId
        && latest.currentProject.rootPath === rootPath
        && latest.authority?.canonicalIdentityKey === authority?.canonicalIdentityKey
        && latest.authority?.identityRevision === authority?.identityRevision;
    };
    try {
      const action = await prepareVersionAction({ projectId, projectRootPath: rootPath });
      if (!current()) { await confirmVersionAction(action.id, false); return false; }
      await confirmVersionAction(action.id, true);
      if (!current()) return false;
      setError(null);
      return true;
    } catch (failure) {
      if (current()) setError(normalizeBackendError(failure, { defaultSummaryKey: "lint.git.unavailable", actionKindOverride: null }));
      return false;
    } finally {
      if (requestEpoch === epoch.current) { busy.current = false; setChecking(false); }
    }
  }, [projectId, rootPath]);

  const clearError = useCallback(() => setError(null), []);
  return { check, enable, checking, error, clearError };
}
