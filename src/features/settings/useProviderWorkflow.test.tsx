import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { AiCapabilitiesWorkflow } from "../../hooks/useAiCapabilities";
import { defaultProject } from "../../stores/projectStore";
import {
  ensureProjectFacts,
  projectFactsKey,
  resetProjectFactsStoreForTests,
  useProjectFactsStore,
} from "../../stores/projectFactsStore";
import type { LlmProviderConfig, ProviderStatus } from "../../types/llm";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import { useProviderWorkflow } from "./useProviderWorkflow";

const project = {
  ...defaultProject,
  projectId: "project-a",
  rootPath: "D:/知识库/project-a",
};
const config: LlmProviderConfig = {
  provider: "anthropic",
  model: "claude-sonnet",
  baseUrl: "https://api.anthropic.com",
  contextWindow: 200000,
  enabled: true,
};
const status: ProviderStatus = {
  config,
  hasSecret: false,
  secretMask: null,
};

let refresh: ReturnType<typeof vi.fn<(forceRefresh?: boolean) => Promise<void>>>;
let capabilities: AiCapabilitiesWorkflow;

beforeEach(() => {
  invokeMock.mockReset();
  resetProjectFactsStoreForTests();
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: {},
    configurable: true,
  });
  refresh = vi.fn(async () => undefined);
  capabilities = {
    agents: [],
    providers: [status],
    refreshing: false,
    refresh,
  };
});

describe("useProviderWorkflow", () => {
  it("invalidates the original project without refreshing it after a project switch", async () => {
    invokeMock.mockResolvedValueOnce([status]);
    await ensureProjectFacts(
      { projectId: project.projectId, rootPath: project.rootPath },
      ["providers"],
    );
    let resolve!: (value: void) => void;
    invokeMock.mockReturnValue(new Promise<void>((next) => { resolve = next; }));
    const projectB = { ...project, projectId: "project-b", rootPath: "D:/wiki/project-b" };
    const { result, rerender } = renderHook(({ current }) => useProviderWorkflow(current, capabilities), {
      initialProps: { current: project },
    });
    const pending = result.current.saveProvider(config);
    rerender({ current: projectB });
    resolve();
    await act(async () => pending);
    expect(refresh).not.toHaveBeenCalled();
    expect(
      useProjectFactsStore.getState().entries[
        projectFactsKey({ projectId: project.projectId, rootPath: project.rootPath })
      ]?.providers.status,
    ).toBe("stale");
  });

  it("saves config and refreshes capabilities exactly once", async () => {
    invokeMock.mockResolvedValue(undefined);
    const { result } = renderHook(() =>
      useProviderWorkflow(project, capabilities),
    );

    await act(async () => result.current.saveProvider(config));

    expect(invokeMock).toHaveBeenCalledWith("save_llm_provider", {
      request: {
        projectId: project.projectId,
        projectRootPath: project.rootPath,
        config,
      },
    });
    expect(refresh).toHaveBeenCalledTimes(1);
    expect(refresh).toHaveBeenCalledWith(true);
    expect(result.current.providers).toEqual([status]);
  });

  it("stores and deletes a secret without retaining it in workflow state", async () => {
    const secret = "ephemeral-test-secret";
    invokeMock.mockResolvedValue(undefined);
    const { result } = renderHook(() =>
      useProviderWorkflow(project, capabilities),
    );

    await act(async () => result.current.saveSecret("anthropic", secret));
    await act(async () => result.current.deleteSecret("anthropic"));

    expect(invokeMock).toHaveBeenNthCalledWith(1, "store_provider_secret", {
      request: { provider: "anthropic", secret },
    });
    expect(invokeMock).toHaveBeenNthCalledWith(2, "delete_provider_secret", {
      request: { provider: "anthropic", secret: null },
    });
    expect(refresh).toHaveBeenNthCalledWith(1, true);
    expect(refresh).toHaveBeenNthCalledWith(2, true);
    expect(result.current).not.toHaveProperty("secret");
  });

  it("invalidates retained provider facts for every project after a global secret change", async () => {
    const projectB = { ...project, projectId: "project-b", rootPath: "D:/wiki/project-b" };
    invokeMock.mockResolvedValueOnce([status]).mockResolvedValueOnce([status]);
    await ensureProjectFacts(
      { projectId: project.projectId, rootPath: project.rootPath },
      ["providers"],
    );
    await ensureProjectFacts(
      { projectId: projectB.projectId, rootPath: projectB.rootPath },
      ["providers"],
    );
    invokeMock.mockResolvedValue(undefined);
    const { result } = renderHook(() => useProviderWorkflow(project, capabilities));

    await act(async () => result.current.saveSecret("anthropic", "global-secret"));

    const entries = useProjectFactsStore.getState().entries;
    expect(entries[projectFactsKey(project)]?.providers.status).toBe("stale");
    expect(entries[projectFactsKey(projectB)]?.providers.status).toBe("stale");
    expect(refresh).toHaveBeenCalledWith(true);
  });

  it("does not refresh or expose a failed mutation as raw user copy", async () => {
    invokeMock.mockRejectedValue(new Error("credential store unavailable"));
    const { result } = renderHook(() =>
      useProviderWorkflow(project, capabilities),
    );

    await act(async () => {
      await expect(result.current.saveSecret("anthropic", "secret")).rejects.toMatchObject({
        summaryKey: "backendError.summary.provider",
        technicalDetails: expect.stringContaining("credential store unavailable"),
        recoverable: true,
      });
    });

    expect(refresh).not.toHaveBeenCalled();
  });

  it("tests providers through typed IPC and falls back safely without Tauri", async () => {
    invokeMock.mockResolvedValue({ ok: true, message: "connected" });
    const { result } = renderHook(() =>
      useProviderWorkflow(project, capabilities),
    );

    await act(async () => {
      await expect(result.current.testProvider(config)).resolves.toEqual({
        ok: true,
        message: "connected",
      });
    });
    expect(invokeMock).toHaveBeenCalledWith("test_llm_provider", {
      request: {
        projectId: project.projectId,
        projectRootPath: project.rootPath,
        config,
      },
    });

    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
    await expect(result.current.testProvider(config)).resolves.toEqual({
      ok: false,
      message: expect.any(String),
    });
  });

  it("preserves a structured provider test rejection for code-mapped recovery", async () => {
    invokeMock.mockRejectedValueOnce({
      code: "LLM_AUTH_FAILED",
      message: "Authorization: Bearer provider-secret",
      details: { api_key: "provider-secret" },
      recoverable: true,
      userActionRequired: true,
    });
    const { result } = renderHook(() =>
      useProviderWorkflow(project, capabilities),
    );

    await act(async () => {
      await expect(result.current.testProvider(config)).rejects.toMatchObject({
        code: "LLM_AUTH_FAILED",
        actionKind: "reauthorize",
        summaryKey: "backendError.summary.provider",
        technicalDetails: expect.not.stringContaining("provider-secret"),
      });
    });
  });
});
