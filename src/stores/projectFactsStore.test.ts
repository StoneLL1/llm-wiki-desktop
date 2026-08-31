import { beforeEach, describe, expect, it, vi } from "vitest";

import type { AgentInfo } from "../types/agent";
import type { ProviderStatus } from "../types/llm";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import {
  bindProjectFactsAuthority,
  ensureProjectFacts,
  invalidateProjectFacts,
  projectFactsKey,
  pruneProjectFacts,
  refreshProjectFacts,
  resetProjectFactsStoreForTests,
  useProjectFactsStore,
} from "./projectFactsStore";

const scopeA = { projectId: "project-a", rootPath: "D:/知识库/project-a" };
const scopeB = { projectId: "project-b", rootPath: "D:/知识库/project-b" };

const installedAgent: AgentInfo = {
  kind: "claude",
  command: "claude",
  state: "installed",
  version: "1.0.0",
  executablePath: "C:/bin/claude.cmd",
  isDefault: true,
  installGuidance: "",
  error: null,
};

const ollamaProvider: ProviderStatus = {
  config: {
    provider: "ollama",
    model: "qwen3",
    baseUrl: "http://localhost:11434",
    contextWindow: 32_000,
    enabled: true,
  },
  hasSecret: false,
  secretMask: null,
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((next, fail) => {
    resolve = next;
    reject = fail;
  });
  return { promise, resolve, reject };
}

function entryFor(scope = scopeA) {
  return useProjectFactsStore.getState().entries[projectFactsKey(scope)];
}

beforeEach(() => {
  invokeMock.mockReset();
  resetProjectFactsStoreForTests();
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: {},
    configurable: true,
  });
});

describe("projectFactsStore", () => {
  it("single-flights concurrent consumers per project and resource", async () => {
    const agents = deferred<AgentInfo[]>();
    invokeMock.mockReturnValue(agents.promise);

    const first = ensureProjectFacts(scopeA, ["agents"]);
    const second = ensureProjectFacts(scopeA, ["agents"]);

    expect(invokeMock).toHaveBeenCalledTimes(1);
    agents.resolve([installedAgent]);
    await Promise.all([first, second]);
    expect(entryFor()?.agents).toMatchObject({
      status: "ready",
      value: [installedAgent],
    });
  });

  it("keeps failures independent and retries after deterministic backoff", async () => {
    const now = vi.spyOn(Date, "now").mockReturnValue(1_000);
    let gitAttempts = 0;
    invokeMock.mockImplementation((command: string) => {
      if (command === "git_status") {
        gitAttempts += 1;
        return gitAttempts === 1
          ? Promise.reject(new Error("git unavailable"))
          : Promise.resolve({
              isRepository: true,
              branch: "main",
              head: "abc123",
              hasChanges: false,
            });
      }
      if (command === "detect_agents") return Promise.resolve([installedAgent]);
      if (command === "list_llm_providers") return Promise.resolve([ollamaProvider]);
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });

    await ensureProjectFacts(scopeA, ["git", "agents", "providers"]);
    await vi.waitFor(() => expect(entryFor()?.providers.status).toBe("ready"));
    expect(entryFor()?.git).toMatchObject({ status: "error", value: null });
    expect(entryFor()?.agents.value).toEqual([installedAgent]);
    expect(entryFor()?.providers.value).toEqual([ollamaProvider]);

    await ensureProjectFacts(scopeA, ["git"]);
    expect(gitAttempts).toBe(1);

    now.mockReturnValue(6_001);
    await ensureProjectFacts(scopeA, ["git"]);
    expect(gitAttempts).toBe(2);
    expect(entryFor()?.git).toMatchObject({
      status: "ready",
      value: expect.objectContaining({ branch: "main" }),
    });
    now.mockRestore();
  });

  it("lets an explicit force bypass error backoff without overlapping", async () => {
    const now = vi.spyOn(Date, "now").mockReturnValue(1_000);
    invokeMock
      .mockRejectedValueOnce(new Error("agent unavailable"))
      .mockResolvedValueOnce([installedAgent]);

    await ensureProjectFacts(scopeA, ["agents"]);
    await ensureProjectFacts(scopeA, ["agents"]);
    expect(invokeMock).toHaveBeenCalledTimes(1);

    await refreshProjectFacts(scopeA, ["agents"]);

    expect(invokeMock).toHaveBeenCalledTimes(2);
    expect(invokeMock).toHaveBeenLastCalledWith("detect_agents", {
      request: {
        projectId: scopeA.projectId,
        projectRootPath: scopeA.rootPath,
        forceRefresh: true,
      },
    });
    expect(entryFor()?.agents.value).toEqual([installedAgent]);
    now.mockRestore();
  });

  it("isolates pending project A results from project B", async () => {
    const agentsA = deferred<AgentInfo[]>();
    invokeMock
      .mockReturnValueOnce(agentsA.promise)
      .mockResolvedValueOnce([]);

    const requestA = ensureProjectFacts(scopeA, ["agents"]);
    await ensureProjectFacts(scopeB, ["agents"]);
    agentsA.resolve([installedAgent]);
    await requestA;

    expect(entryFor(scopeA)?.agents.value).toEqual([installedAgent]);
    expect(entryFor(scopeB)?.agents.value).toEqual([]);
  });

  it("rejects a pre-switch A response after an A to B to A visit cycle", async () => {
    const staleAgentsA = deferred<AgentInfo[]>();
    const currentAgentsA = deferred<AgentInfo[]>();
    invokeMock
      .mockReturnValueOnce(staleAgentsA.promise)
      .mockResolvedValueOnce([])
      .mockReturnValueOnce(currentAgentsA.promise);

    const keyA = projectFactsKey(scopeA);
    const keyB = projectFactsKey(scopeB);
    pruneProjectFacts(keyA);
    const staleRequest = ensureProjectFacts(scopeA, ["agents"]);
    pruneProjectFacts(keyB);
    await ensureProjectFacts(scopeB, ["agents"]);
    pruneProjectFacts(keyA);
    const currentRequest = ensureProjectFacts(scopeA, ["agents"]);

    staleAgentsA.resolve([installedAgent]);
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledTimes(3));
    expect(entryFor(scopeA)?.agents.value).toBeNull();

    currentAgentsA.resolve([]);
    await Promise.all([staleRequest, currentRequest]);
    expect(entryFor(scopeA)?.agents.value).toEqual([]);
    expect(invokeMock).toHaveBeenCalledTimes(3);
  });

  it("does not let an older ordinary response overwrite a force refresh", async () => {
    const ordinary = deferred<AgentInfo[]>();
    const forcedAgent = { ...installedAgent, kind: "codex" as const, command: "codex" };
    invokeMock
      .mockReturnValueOnce(ordinary.promise)
      .mockResolvedValueOnce([forcedAgent]);

    const first = ensureProjectFacts(scopeA, ["agents"]);
    const refresh = refreshProjectFacts(scopeA, ["agents"]);
    ordinary.resolve([installedAgent]);
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledTimes(2));
    await Promise.all([first, refresh]);

    expect(invokeMock).toHaveBeenNthCalledWith(2, "detect_agents", {
      request: {
        projectId: scopeA.projectId,
        projectRootPath: scopeA.rootPath,
        forceRefresh: true,
      },
    });
    expect(entryFor()?.agents.value).toEqual([forcedAgent]);
  });

  it("coalesces invalidate and force behind one active fact request", async () => {
    const beforeMutation = deferred<ProviderStatus[]>();
    const afterMutation = deferred<ProviderStatus[]>();
    let activeRequests = 0;
    let maximumActiveRequests = 0;
    invokeMock
      .mockImplementationOnce(() => {
        activeRequests += 1;
        maximumActiveRequests = Math.max(maximumActiveRequests, activeRequests);
        return beforeMutation.promise.finally(() => {
          activeRequests -= 1;
        });
      })
      .mockImplementationOnce(() => {
        activeRequests += 1;
        maximumActiveRequests = Math.max(maximumActiveRequests, activeRequests);
        return afterMutation.promise.finally(() => {
          activeRequests -= 1;
        });
      });

    const oldRefresh = refreshProjectFacts(scopeA, ["providers"]);
    invalidateProjectFacts(scopeA, ["providers"], "provider_saved");
    const newRefresh = refreshProjectFacts(scopeA, ["providers"]);
    beforeMutation.resolve([ollamaProvider]);
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledTimes(2));
    afterMutation.resolve([ollamaProvider]);
    await Promise.all([oldRefresh, newRefresh]);

    expect(maximumActiveRequests).toBe(1);
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });

  it("reuses fresh values, then single-flights stale revalidation", async () => {
    invokeMock.mockResolvedValue([installedAgent]);
    await ensureProjectFacts(scopeA, ["agents"]);
    await ensureProjectFacts(scopeA, ["agents"]);
    expect(invokeMock).toHaveBeenCalledTimes(1);

    invalidateProjectFacts(scopeA, ["agents"], "window_focus");
    const first = ensureProjectFacts(scopeA, ["agents"]);
    const second = ensureProjectFacts(scopeA, ["agents"]);
    await Promise.all([first, second]);
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });

  it("keeps the last successful value visible during stale revalidation", async () => {
    const revalidation = deferred<AgentInfo[]>();
    invokeMock
      .mockResolvedValueOnce([installedAgent])
      .mockReturnValueOnce(revalidation.promise);
    await ensureProjectFacts(scopeA, ["agents"]);

    invalidateProjectFacts(scopeA, ["agents"], "agent_saved");
    const refresh = refreshProjectFacts(scopeA, ["agents"]);

    expect(entryFor()?.agents).toMatchObject({
      status: "stale",
      value: [installedAgent],
    });
    revalidation.resolve([]);
    await refresh;
    expect(entryFor()?.agents).toMatchObject({ status: "ready", value: [] });
  });

  it("revalidates after the centralized TTL expires", async () => {
    const now = vi.spyOn(Date, "now").mockReturnValue(1_000);
    invokeMock.mockResolvedValue([installedAgent]);
    await ensureProjectFacts(scopeA, ["agents"]);

    now.mockReturnValue(1_000 + 30_001);
    await ensureProjectFacts(scopeA, ["agents"]);

    expect(invokeMock).toHaveBeenCalledTimes(2);
    now.mockRestore();
  });

  it("keeps at most the three most recently used project entries", async () => {
    invokeMock.mockResolvedValue([]);
    const scopes = ["a", "b", "c", "d"].map((suffix) => ({
      projectId: `project-${suffix}`,
      rootPath: `D:/wiki/${suffix}`,
    }));
    for (const scope of scopes) {
      await ensureProjectFacts(scope, ["agents"]);
    }

    expect(Object.keys(useProjectFactsStore.getState().entries)).toEqual(
      scopes.slice(1).map(projectFactsKey),
    );
  });

  it("retains an active pruned control until it settles, then reclaims it", async () => {
    const oldest = deferred<AgentInfo[]>();
    invokeMock
      .mockReturnValueOnce(oldest.promise)
      .mockResolvedValue([]);
    const scopes = ["a", "b", "c", "d"].map((suffix) => ({
      projectId: `project-${suffix}`,
      rootPath: `D:/wiki/${suffix}`,
    }));

    const oldestRequest = ensureProjectFacts(scopes[0], ["agents"]);
    for (const scope of scopes.slice(1)) await ensureProjectFacts(scope, ["agents"]);
    expect(entryFor(scopes[0])).toBeDefined();

    oldest.resolve([installedAgent]);
    await oldestRequest;

    expect(entryFor(scopes[0])).toBeUndefined();
    expect(Object.keys(useProjectFactsStore.getState().entries)).toHaveLength(3);
  });

  it("clears sensitive facts when an authority identity changes", async () => {
    invokeMock.mockResolvedValue([ollamaProvider]);
    bindProjectFactsAuthority(scopeA, "identity-a\0revision-1");
    await ensureProjectFacts(scopeA, ["providers"]);

    bindProjectFactsAuthority(scopeA, "identity-a\0revision-2");

    expect(entryFor()?.providers).toMatchObject({
      status: "idle",
      value: null,
    });
  });

  it("rejects a late response when only the authority revision changes", async () => {
    const stale = deferred<ProviderStatus[]>();
    const current = deferred<ProviderStatus[]>();
    invokeMock
      .mockReturnValueOnce(stale.promise)
      .mockReturnValueOnce(current.promise);
    bindProjectFactsAuthority(scopeA, "identity-a\0identity-revision-a\0authority-a");
    const request = ensureProjectFacts(scopeA, ["providers"]);

    bindProjectFactsAuthority(scopeA, "identity-a\0identity-revision-a\0authority-b");
    stale.resolve([ollamaProvider]);
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledTimes(2));
    expect(entryFor()?.providers.value).toBeNull();

    current.resolve([]);
    await request;
    expect(entryFor()?.providers).toMatchObject({ status: "ready", value: [] });
  });

  it("supersedes an unbound request when the first authority arrives", async () => {
    const unbound = deferred<ProviderStatus[]>();
    const current = deferred<ProviderStatus[]>();
    invokeMock
      .mockReturnValueOnce(unbound.promise)
      .mockReturnValueOnce(current.promise);
    const request = ensureProjectFacts(scopeA, ["providers"]);

    bindProjectFactsAuthority(
      scopeA,
      "identity-a\0identity-revision-a\0authority-a",
    );
    unbound.resolve([ollamaProvider]);
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledTimes(2));
    expect(entryFor()?.providers.value).toBeNull();

    current.resolve([]);
    await request;
    expect(entryFor()).toMatchObject({
      authorityIdentityKey: "identity-a\0identity-revision-a\0authority-a",
      providers: { status: "ready", value: [] },
    });
  });

  it("does not revive a pruned active control when a stale invalidation arrives", async () => {
    const oldest = deferred<AgentInfo[]>();
    invokeMock.mockReturnValueOnce(oldest.promise).mockResolvedValue([]);
    const scopes = ["a", "b", "c", "d"].map((suffix) => ({
      projectId: `project-${suffix}`,
      rootPath: `D:/wiki/${suffix}`,
    }));
    const request = ensureProjectFacts(scopes[0], ["agents"]);
    for (const scope of scopes.slice(1)) await ensureProjectFacts(scope, ["agents"]);

    invalidateProjectFacts(scopes[0], ["agents"], "late_owner_callback");
    oldest.resolve([installedAgent]);
    await request;

    expect(entryFor(scopes[0])).toBeUndefined();
    expect(invokeMock).toHaveBeenCalledTimes(4);
  });
});
