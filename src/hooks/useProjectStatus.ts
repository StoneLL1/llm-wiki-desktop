import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

import type { AgentInfo } from "../types/agent";
import type { ProviderStatus } from "../types/llm";

export interface GitRepositoryStatus {
  isRepository: boolean;
  branch: string | null;
  head: string | null;
  hasChanges: boolean;
}

export interface ProjectStatusSnapshot {
  git: GitRepositoryStatus | null;
  agents: AgentInfo[];
  providers: ProviderStatus[];
}

interface CachedEntry {
  key: string;
  snapshot: ProjectStatusSnapshot;
}

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

let cache: CachedEntry | null = null;

/**
 * Fetches git status + detected Agent CLIs + BYOK providers for the current
 * project. Results are cached per project key so the three shell surfaces that
 * need this data (right panel, status bar, sidebar foot) don't each re-spawn
 * the CLI probes. The cache is replaced on project switch.
 */
export function useProjectStatus(
  projectId: string,
  rootPath: string,
): ProjectStatusSnapshot | null {
  const key = `${projectId}@${rootPath}`;
  const [snapshot, setSnapshot] = useState<ProjectStatusSnapshot | null>(
    () => (cache && cache.key === key ? cache.snapshot : null),
  );

  useEffect(() => {
    if (!hasTauri() || !projectId || !rootPath) return;
    if (cache && cache.key === key) {
      setSnapshot(cache.snapshot);
      return;
    }
    let active = true;
    const request = { projectId, projectRootPath: rootPath };
    void Promise.all([
      invoke<GitRepositoryStatus>("git_status", { request }).catch(
        (): GitRepositoryStatus | null => null,
      ),
      invoke<AgentInfo[]>("detect_agents", { request }).catch(
        (): AgentInfo[] => [],
      ),
      invoke<ProviderStatus[]>("list_llm_providers", { request }).catch(
        (): ProviderStatus[] => [],
      ),
    ]).then(([git, agents, providers]) => {
      if (!active) return;
      const next: ProjectStatusSnapshot = { git, agents, providers };
      cache = { key, snapshot: next };
      setSnapshot(next);
    });
    return () => {
      active = false;
    };
  }, [key, projectId, rootPath]);

  return snapshot;
}
