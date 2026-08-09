import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";

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

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function resolveRoute(
  agents: AgentInfo[],
  providers: ProviderStatus[],
): AgentRoute {
  const agentReady = agents.some(
    (agent) => agent.isDefault && agent.state === "installed",
  );
  if (agentReady) return "agent";

  const byokReady = providers.some(
    (provider) =>
      provider.config.enabled &&
      (provider.hasSecret || provider.config.provider === "ollama"),
  );
  return byokReady ? "byok" : "unconfigured";
}

export function useAiCapabilities(
  project: ProjectSummary,
  refreshWhenVisible: boolean,
): AiCapabilitiesWorkflow {
  const projectId = project.projectId;
  const rootPath = project.rootPath;
  const projectKey = `${projectId}\0${rootPath}`;
  const latestProjectKey = useRef(projectKey);
  latestProjectKey.current = projectKey;
  const requestEpoch = useRef(0);
  const visibleRef = useRef(refreshWhenVisible);
  const setAgentRoute = useProjectStore((state) => state.setAgentRoute);
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  const [providers, setProviders] = useState<ProviderStatus[]>([]);
  const [refreshing, setRefreshing] = useState(false);

  const refresh = useCallback(async (forceRefresh = false) => {
    if (!hasTauri() || !projectId) return;
    const requestKey = projectKey;
    const epoch = ++requestEpoch.current;
    setRefreshing(true);
    try {
      const request = { projectId, projectRootPath: rootPath, forceRefresh };
      const [detectedAgents, providerStatuses] = await Promise.all([
        invoke<AgentInfo[]>("detect_agents", { request }),
        invoke<ProviderStatus[]>("list_llm_providers", { request }),
      ]);
      if (latestProjectKey.current !== requestKey || requestEpoch.current !== epoch) return;
      setAgents(detectedAgents);
      setProviders(providerStatuses);
      setAgentRoute(
        projectId,
        rootPath,
        resolveRoute(detectedAgents, providerStatuses),
      );
    } finally {
      if (latestProjectKey.current === requestKey && requestEpoch.current === epoch) {
        setRefreshing(false);
      }
    }
  }, [projectId, projectKey, rootPath, setAgentRoute]);

  useEffect(() => {
    setAgents([]);
    setProviders([]);
    void refresh().catch(() => undefined);
  }, [projectKey, refresh]);

  useEffect(() => {
    const wasVisible = visibleRef.current;
    visibleRef.current = refreshWhenVisible;
    if (refreshWhenVisible && !wasVisible) {
      void refresh().catch(() => undefined);
    }
  }, [refreshWhenVisible, refresh]);

  return { agents, providers, refreshing, refresh };
}
