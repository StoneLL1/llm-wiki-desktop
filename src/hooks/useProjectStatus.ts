import { useEffect, useMemo } from "react";

import {
  ensureProjectFacts,
  nextProjectFactsExpiryAt,
  projectFactsKey,
  useProjectFactsStore,
  type GitRepositoryStatus,
  type ProjectFactKind,
} from "../stores/projectFactsStore";
import { useProjectStore } from "../stores/projectStore";
import type { AgentInfo } from "../types/agent";
import type { ProviderStatus } from "../types/llm";

export type { GitRepositoryStatus } from "../stores/projectFactsStore";

export interface ProjectStatusSnapshot {
  git: GitRepositoryStatus | null;
  agents: AgentInfo[];
  providers: ProviderStatus[];
}

const ALL_FACT_KINDS: readonly ProjectFactKind[] = ["git", "agents", "providers"];

/**
 * Read-only shell adapter over the shared project facts coordinator. The
 * legacy snapshot shape stays stable while loading/error/freshness remain
 * explicit inside projectFactsStore.
 */
export function useProjectStatus(
  projectId: string,
  rootPath: string,
  enabled = true,
  kinds: readonly ProjectFactKind[] = ALL_FACT_KINDS,
): ProjectStatusSnapshot | null {
  const scope = useMemo(() => ({ projectId, rootPath }), [projectId, rootPath]);
  const key = projectFactsKey(scope);
  const kindsKey = [...new Set(kinds)].sort().join(",");
  const requestedKinds = useMemo(
    () => kindsKey ? kindsKey.split(",") as ProjectFactKind[] : [],
    [kindsKey],
  );
  const authorityIdentityKey = useProjectStore((state) =>
    state.currentProject.projectId === projectId
      && state.currentProject.rootPath === rootPath
      && state.authority?.projectId === projectId
      ? `${state.authority.canonicalIdentityKey}\0${state.authority.identityRevision}`
      : null
  );
  const storedEntry = useProjectFactsStore((state) => state.entries[key] ?? null);
  const authorityMatches = !storedEntry
    || storedEntry.authorityIdentityKey === authorityIdentityKey;
  const entry = storedEntry && authorityMatches
    ? storedEntry
    : null;
  const staleSignature = requestedKinds
    .map((kind) => `${kind}:${entry?.[kind].status ?? "idle"}`)
    .join("|");
  const expiryAt = nextProjectFactsExpiryAt(entry, requestedKinds);

  useEffect(() => {
    if (!enabled || !projectId || !rootPath || requestedKinds.length === 0) return;
    if (!authorityMatches) return;
    void ensureProjectFacts(scope, requestedKinds).catch(() => undefined);
  }, [authorityMatches, enabled, requestedKinds, scope]);

  useEffect(() => {
    if (
      !enabled
      || !projectId
      || !rootPath
      || !requestedKinds.some((kind) => {
        const status = entry?.[kind].status;
        return status === "idle" || status === "stale";
      })
    ) {
      return;
    }
    void ensureProjectFacts(scope, requestedKinds).catch(() => undefined);
  }, [enabled, entry, requestedKinds, scope, staleSignature]);

  useEffect(() => {
    if (!enabled) return;
    if (expiryAt === null) return;
    const delay = Math.max(1, expiryAt - Date.now() + 1);
    const timeout = window.setTimeout(() => {
      void ensureProjectFacts(scope, requestedKinds).catch(() => undefined);
    }, delay);
    return () => window.clearTimeout(timeout);
  }, [enabled, expiryAt, requestedKinds, scope]);

  if (!enabled || !entry) return null;
  const hasKnownResult = requestedKinds.length > 0 && requestedKinds.every((kind) => {
    const resource = entry[kind];
    return resource.value !== null || resource.status === "error";
  });
  if (!hasKnownResult) return null;

  return {
    git: entry.git.value,
    agents: entry.agents.value ?? [],
    providers: entry.providers.value ?? [],
  };
}
