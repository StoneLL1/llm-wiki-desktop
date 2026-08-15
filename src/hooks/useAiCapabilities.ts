import { useCallback, useEffect, useMemo, useRef } from "react";

import {
  ensureProjectFacts,
  nextProjectFactsExpiryAt,
  projectFactsKey,
  refreshProjectFacts,
  useProjectFactsStore,
} from "../stores/projectFactsStore";
import { useProjectStore } from "../stores/projectStore";
import type { AgentInfo } from "../types/agent";
import type { ProviderStatus } from "../types/llm";
import type { AgentRoute, ProjectSummary } from "../types/project";

export interface AiCapabilitiesWorkflow {
  agents: AgentInfo[];
  providers: ProviderStatus[];
  refreshing: boolean;
  refresh: (forceRefresh?: boolean) => Promise<void>;
}

const EMPTY_AGENTS: AgentInfo[] = [];
const EMPTY_PROVIDERS: ProviderStatus[] = [];
const CAPABILITY_FACTS = ["agents", "providers"] as const;

function resolveKnownRoute(
  agents: AgentInfo[] | null,
  providers: ProviderStatus[] | null,
): AgentRoute | null {
  const agentReady = agents?.some(
    (agent) => agent.isDefault && agent.state === "installed",
  );
  if (agentReady) return "agent";

  const byokReady = providers?.some(
    (provider) =>
      provider.config.enabled &&
      (provider.hasSecret || provider.config.provider === "ollama"),
  );
  if (byokReady) return "byok";
  return agents !== null && providers !== null ? "unconfigured" : null;
}

export function useAiCapabilities(
  project: ProjectSummary,
  refreshWhenVisible: boolean,
): AiCapabilitiesWorkflow {
  const projectId = project.projectId;
  const rootPath = project.rootPath;
  const scope = useMemo(() => ({ projectId, rootPath }), [projectId, rootPath]);
  const projectKey = projectFactsKey(scope);
  const authorityIdentityKey = useProjectStore((state) =>
    state.currentProject.projectId === projectId
      && state.currentProject.rootPath === rootPath
      && state.authority?.projectId === projectId
      ? `${state.authority.canonicalIdentityKey}\0${state.authority.identityRevision}`
      : null
  );
  const storedEntry = useProjectFactsStore((state) => state.entries[projectKey] ?? null);
  const authorityMatches = !storedEntry
    || storedEntry.authorityIdentityKey === authorityIdentityKey;
  const entry = storedEntry && authorityMatches
    ? storedEntry
    : null;
  const setAgentRoute = useProjectStore((state) => state.setAgentRoute);
  const visibleRef = useRef(refreshWhenVisible);
  const agents = entry?.agents.value ?? EMPTY_AGENTS;
  const providers = entry?.providers.value ?? EMPTY_PROVIDERS;
  const expiryAt = nextProjectFactsExpiryAt(entry, CAPABILITY_FACTS);
  const refreshing = entry?.agents.status === "loading"
    || entry?.agents.status === "stale"
    || entry?.providers.status === "loading"
    || entry?.providers.status === "stale";

  const refresh = useCallback((forceRefresh = false) => {
    return forceRefresh
      ? refreshProjectFacts(scope, CAPABILITY_FACTS)
      : ensureProjectFacts(scope, CAPABILITY_FACTS);
  }, [scope]);

  useEffect(() => {
    if (!authorityMatches) return;
    void ensureProjectFacts(scope, CAPABILITY_FACTS).catch(() => undefined);
  }, [authorityMatches, scope]);

  useEffect(() => {
    if (
      entry?.agents.status !== "idle"
      && entry?.agents.status !== "stale"
      && entry?.providers.status !== "idle"
      && entry?.providers.status !== "stale"
    ) return;
    void ensureProjectFacts(scope, CAPABILITY_FACTS).catch(() => undefined);
  }, [entry?.agents.status, entry?.providers.status, scope]);

  useEffect(() => {
    if (expiryAt === null) return;
    const delay = Math.max(1, expiryAt - Date.now() + 1);
    const timeout = window.setTimeout(() => {
      void ensureProjectFacts(scope, CAPABILITY_FACTS).catch(() => undefined);
    }, delay);
    return () => window.clearTimeout(timeout);
  }, [expiryAt, scope]);

  useEffect(() => {
    const wasVisible = visibleRef.current;
    visibleRef.current = refreshWhenVisible;
    if (refreshWhenVisible && !wasVisible) {
      void ensureProjectFacts(scope, CAPABILITY_FACTS).catch(() => undefined);
    }
  }, [refreshWhenVisible, scope]);

  useEffect(() => {
    if (!entry) return;
    const route = resolveKnownRoute(entry.agents.value, entry.providers.value);
    if (route === null) return;
    const current = useProjectStore.getState().currentProject;
    if (current.projectId !== projectId || current.rootPath !== rootPath) return;
    setAgentRoute(projectId, rootPath, route);
  }, [entry?.agents.value, entry?.providers.value, projectId, rootPath, setAgentRoute]);

  return { agents, providers, refreshing, refresh };
}
