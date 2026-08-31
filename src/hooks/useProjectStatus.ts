import { useEffect, useMemo } from "react";

import {
  bindProjectFactsAuthority,
  ensureProjectFacts,
  projectFactsAuthorityKey,
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
      ? projectFactsAuthorityKey(state.authority)
      : null
  );
  const storedEntry = useProjectFactsStore((state) => state.entries[key] ?? null);
  const authorityMatches = authorityIdentityKey === null
    ? !storedEntry || storedEntry.authorityIdentityKey === null
    : storedEntry?.authorityIdentityKey === authorityIdentityKey;
  const entry = storedEntry && authorityMatches
    ? storedEntry
    : null;
  const staleKindsKey = requestedKinds
    .filter((kind) => {
      const status = entry?.[kind].status;
      return status === undefined || status === "idle" || status === "stale";
    })
    .join(",");
  const staleKinds = useMemo(
    () => staleKindsKey ? staleKindsKey.split(",") as ProjectFactKind[] : [],
    [staleKindsKey],
  );

  useEffect(() => {
    if (!enabled || !projectId || !rootPath || authorityIdentityKey === null) return;
    if (storedEntry || authorityMatches) return;
    bindProjectFactsAuthority(scope, authorityIdentityKey);
  }, [authorityIdentityKey, authorityMatches, enabled, projectId, rootPath, scope, storedEntry]);

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
      || !authorityMatches
      || staleKinds.length === 0
    ) {
      return;
    }
    void ensureProjectFacts(scope, staleKinds).catch(() => undefined);
  }, [authorityMatches, enabled, scope, staleKinds]);

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
