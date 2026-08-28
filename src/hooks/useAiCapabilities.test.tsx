import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { AgentInfo } from "../types/agent";
import type { ProviderStatus } from "../types/llm";
import type { ProjectSummary } from "../types/project";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import { resetProjectFactsStoreForTests } from "../stores/projectFactsStore";
import { defaultProject, useProjectStore } from "../stores/projectStore";
import { useAiCapabilities } from "./useAiCapabilities";

const projectA: ProjectSummary = {
  ...defaultProject,
  projectId: "project-a",
  name: "Project A",
  rootPath: "D:/知识库/project-a",
};

const projectB: ProjectSummary = {
  ...projectA,
  projectId: "project-b",
  name: "Project B",
  rootPath: "D:/知识库/project-b",
};
const targetIt = process.env.LLM_WIKI_PROJECT_FACTS_TARGET === "1" ? it : it.skip;

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

function provider(
  kind: ProviderStatus["config"]["provider"],
  enabled: boolean,
  hasSecret: boolean,
): ProviderStatus {
  return {
    config: {
      provider: kind,
      model: "model",
      baseUrl: "https://example.test",
      contextWindow: 8192,
      enabled,
    },
    hasSecret,
    secretMask: hasSecret ? "••••" : null,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((next) => {
    resolve = next;
  });
  return { promise, resolve };
}

beforeEach(() => {
  invokeMock.mockReset();
  resetProjectFactsStoreForTests();
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: {},
    configurable: true,
  });
  useProjectStore.setState({ currentProject: projectA });
});

afterEach(() => {
  vi.useRealTimers();
});

describe("useAiCapabilities", () => {
  it("freezes the red baseline of Agent and Provider polling over 60 idle seconds", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000);
    invokeMock.mockResolvedValue([]);

    renderHook(() => useAiCapabilities(projectA, false));
    await act(async () => Promise.resolve());
    expect(invokeMock.mock.calls.map(([command]) => command)).toEqual([
      "detect_agents",
      "list_llm_providers",
    ]);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(60_100);
    });

    expect(invokeMock.mock.calls.filter(([command]) => command === "detect_agents")).toHaveLength(3);
    expect(invokeMock.mock.calls.filter(([command]) => command === "list_llm_providers")).toHaveLength(3);
  });

  targetIt("target: does not poll Agent or Provider after 60 idle seconds", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000);
    invokeMock.mockResolvedValue([]);
    renderHook(() => useAiCapabilities(projectA, false));
    await act(async () => Promise.resolve());

    await act(async () => {
      await vi.advanceTimersByTimeAsync(60_100);
    });

    expect(invokeMock.mock.calls.filter(([command]) => command === "detect_agents")).toHaveLength(1);
    expect(invokeMock.mock.calls.filter(([command]) => command === "list_llm_providers")).toHaveLength(1);
  });

  it("marks only explicit user refreshes as force refreshes", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "detect_agents") return Promise.resolve([]);
      if (command === "list_llm_providers") return Promise.resolve([]);
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });

    const { result } = renderHook(() => useAiCapabilities(projectA, false));
    await waitFor(() => expect(result.current.refreshing).toBe(false));
    expect(invokeMock.mock.calls.find(([command]) => command === "detect_agents")?.[1]).toEqual({
      request: {
        projectId: projectA.projectId,
        projectRootPath: projectA.rootPath,
        forceRefresh: false,
      },
    });

    await act(async () => {
      await result.current.refresh(true);
    });
    const detectRequests = invokeMock.mock.calls.filter(
      ([command]) => command === "detect_agents",
    );
    expect(detectRequests.at(-1)?.[1]).toEqual({
      request: {
        projectId: projectA.projectId,
        projectRootPath: projectA.rootPath,
        forceRefresh: true,
      },
    });
  });

  it("probes agents and providers in parallel and prefers an installed default agent", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "detect_agents") return Promise.resolve([installedAgent]);
      if (command === "list_llm_providers") {
        return Promise.resolve([provider("anthropic", true, true)]);
      }
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });

    const { result } = renderHook(() => useAiCapabilities(projectA, false));

    await waitFor(() => expect(result.current.refreshing).toBe(false));
    expect(result.current.agents).toEqual([installedAgent]);
    expect(result.current.providers).toEqual([provider("anthropic", true, true)]);
    expect(invokeMock.mock.calls.map(([command]) => command)).toEqual([
      "detect_agents",
      "list_llm_providers",
    ]);
    expect(useProjectStore.getState().currentProject.agentRoute).toBe("agent");
  });

  it.each([
    ["a secret-enabled provider", provider("anthropic", true, true)],
    ["enabled Ollama without a secret", provider("ollama", true, false)],
  ])("falls back to BYOK for %s", async (_label, status) => {
    invokeMock
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([status]);

    const { result } = renderHook(() => useAiCapabilities(projectA, false));

    await waitFor(() => expect(result.current.refreshing).toBe(false));
    expect(useProjectStore.getState().currentProject.agentRoute).toBe("byok");
  });

  it("marks the project unconfigured when neither route is available", async () => {
    useProjectStore.setState({
      currentProject: { ...projectA, agentRoute: "agent" },
    });
    invokeMock
      .mockResolvedValueOnce([{ ...installedAgent, isDefault: false, state: "missing" }])
      .mockResolvedValueOnce([provider("anthropic", true, false)]);

    const { result } = renderHook(() => useAiCapabilities(projectA, false));

    await waitFor(() => expect(result.current.refreshing).toBe(false));
    expect(useProjectStore.getState().currentProject.agentRoute).toBe("unconfigured");
  });

  it("keeps an explicitly installed Agent route when provider probing fails", async () => {
    invokeMock
      .mockResolvedValueOnce([installedAgent])
      .mockRejectedValueOnce(new Error("provider read failed"));

    renderHook(() => useAiCapabilities(projectA, false));

    await waitFor(() => {
      expect(useProjectStore.getState().currentProject.agentRoute).toBe("agent");
    });
  });

  it("does not let a slow project A probe overwrite project B", async () => {
    const agentsA = deferred<AgentInfo[]>();
    const providersA = deferred<ProviderStatus[]>();
    invokeMock
      .mockReturnValueOnce(agentsA.promise)
      .mockReturnValueOnce(providersA.promise)
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([provider("ollama", true, false)]);

    const { result, rerender } = renderHook(
      ({ project }) => useAiCapabilities(project, false),
      { initialProps: { project: projectA } },
    );

    useProjectStore.getState().setCurrentProject(projectB);
    rerender({ project: projectB });
    await waitFor(() => expect(result.current.providers).toEqual([provider("ollama", true, false)]));
    expect(useProjectStore.getState().currentProject.agentRoute).toBe("byok");

    await act(async () => {
      agentsA.resolve([installedAgent]);
      providersA.resolve([]);
      await Promise.all([agentsA.promise, providersA.promise]);
    });

    expect(result.current.agents).toEqual([]);
    expect(result.current.providers).toEqual([provider("ollama", true, false)]);
    expect(useProjectStore.getState().currentProject).toMatchObject({
      projectId: projectB.projectId,
      rootPath: projectB.rootPath,
      agentRoute: "byok",
    });
  });

  it("keeps only the latest same-project forced capability refresh", async () => {
    const firstAgents = deferred<AgentInfo[]>();
    const firstProviders = deferred<ProviderStatus[]>();
    invokeMock
      .mockReturnValueOnce(firstAgents.promise)
      .mockReturnValueOnce(firstProviders.promise)
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([provider("ollama", true, false)]);
    const { result } = renderHook(() => useAiCapabilities(projectA, false));

    await act(async () => result.current.refresh(true));
    await waitFor(() => expect(result.current.providers).toEqual([provider("ollama", true, false)]));
    act(() => {
      firstAgents.resolve([installedAgent]);
      firstProviders.resolve([]);
    });
    await act(async () => Promise.all([firstAgents.promise, firstProviders.promise]));

    expect(result.current.agents).toEqual([]);
    expect(result.current.providers).toEqual([provider("ollama", true, false)]);
    expect(useProjectStore.getState().currentProject.agentRoute).toBe("byok");
  });

  it("reuses fresh capability facts when capability management becomes visible", async () => {
    invokeMock.mockResolvedValue([]);
    const { result, rerender } = renderHook(
      ({ visible }) => useAiCapabilities(projectA, visible),
      { initialProps: { visible: false } },
    );
    await waitFor(() => expect(result.current.refreshing).toBe(false));
    expect(invokeMock).toHaveBeenCalledTimes(2);

    rerender({ visible: true });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledTimes(2));
    rerender({ visible: true });

    expect(invokeMock).toHaveBeenCalledTimes(2);
  });
});
