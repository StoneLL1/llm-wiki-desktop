import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { AiCapabilitiesWorkflow } from "../../hooks/useAiCapabilities";
import { defaultProject } from "../../stores/projectStore";
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

let refresh: ReturnType<typeof vi.fn<() => Promise<void>>>;
let capabilities: AiCapabilitiesWorkflow;

beforeEach(() => {
  invokeMock.mockReset();
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
    expect(refresh).toHaveBeenCalledTimes(2);
    expect(result.current).not.toHaveProperty("secret");
  });

  it("does not refresh or swallow a failed mutation", async () => {
    invokeMock.mockRejectedValue(new Error("credential store unavailable"));
    const { result } = renderHook(() =>
      useProviderWorkflow(project, capabilities),
    );

    await act(async () => {
      await expect(result.current.saveSecret("anthropic", "secret")).rejects.toThrow(
        "credential store unavailable",
      );
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
});
