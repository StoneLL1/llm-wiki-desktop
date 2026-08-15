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

  it("keeps resource failures independent and retries failed resources", async () => {
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
    expect(gitAttempts).toBe(2);
    expect(entryFor()?.git).toMatchObject({
      status: "ready",
      value: expect.objectContaining({ branch: "main" }),
    });
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
    await staleRequest;
    expect(entryFor(scopeA)?.agents.value).toBeNull();

    currentAgentsA.resolve([]);
    await currentRequest;
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
    await refreshProjectFacts(scopeA, ["agents"]);
    ordinary.resolve([installedAgent]);
    await first;

    expect(invokeMock).toHaveBeenNthCalledWith(2, "detect_agents", {
      request: {
        projectId: scopeA.projectId,
        projectRootPath: scopeA.rootPath,
        forceRefresh: true,
      },
    });
    expect(entryFor()?.agents.value).toEqual([forcedAgent]);
  });

  it("starts a post-invalidation force request instead of joining an older force", async () => {
    const beforeMutation = deferred<ProviderStatus[]>();
    const afterMutation = deferred<ProviderStatus[]>();
    const freshProvider = {
      ...ollamaProvider,
      config: { ...ollamaProvider.config, model: "qwen3-fresh" },
    };
    invokeMock
      .mockReturnValueOnce(beforeMutation.promise)
      .mockReturnValueOnce(afterMutation.promise);

    const oldRefresh = refreshProjectFacts(scopeA, ["providers"]);
    invalidateProjectFacts(scopeA, ["providers"], "provider_saved");
    const newRefresh = refreshProjectFacts(scopeA, ["providers"]);
    afterMutation.resolve([freshProvider]);
    await newRefresh;
    beforeMutation.resolve([ollamaProvider]);
    await oldRefresh;

    expect(invokeMock).toHaveBeenCalledTimes(2);
    expect(entryFor()?.providers.value).toEqual([freshProvider]);
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
});
