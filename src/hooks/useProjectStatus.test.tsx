import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import {
  bindProjectFactsAuthority,
  ensureProjectFacts,
  resetProjectFactsStoreForTests,
} from "../stores/projectFactsStore";
import { defaultProject, useProjectStore } from "../stores/projectStore";
import { useProjectStatus } from "./useProjectStatus";

const projectId = "project-a";
const rootPath = "D:/知识库/project-a";

beforeEach(() => {
  invokeMock.mockReset();
  resetProjectFactsStoreForTests();
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: {},
    configurable: true,
  });
  useProjectStore.setState({
    currentProject: { ...defaultProject, projectId, rootPath },
    authority: null,
  });
});

afterEach(() => {
  vi.useRealTimers();
});

describe("useProjectStatus", () => {
  it("adapts independent facts to the legacy shell snapshot", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "git_status") return Promise.reject(new Error("not a repository"));
      if (command === "detect_agents") return Promise.resolve([]);
      if (command === "list_llm_providers") return Promise.resolve([]);
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });

    const { result } = renderHook(() => useProjectStatus(projectId, rootPath));

    await waitFor(() => expect(result.current).not.toBeNull());
    expect(result.current).toEqual({ git: null, agents: [], providers: [] });
  });

  it("requests only the resources named by a consumer", async () => {
    invokeMock.mockResolvedValue([]);

    const { result } = renderHook(() =>
      useProjectStatus(projectId, rootPath, true, ["agents"]),
    );

    await waitFor(() => expect(result.current).not.toBeNull());
    expect(invokeMock.mock.calls.map(([command]) => command)).toEqual(["detect_agents"]);
  });

  it("revalidates a continuously mounted consumer when its TTL expires", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000);
    invokeMock.mockResolvedValue([]);
    renderHook(() => useProjectStatus(projectId, rootPath, true, ["agents"]));
    await act(async () => Promise.resolve());
    expect(invokeMock).toHaveBeenCalledTimes(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_001);
    });

    expect(invokeMock).toHaveBeenCalledTimes(2);
  });

  it("fails closed instead of exposing facts from a different authority identity", async () => {
    const scope = { projectId, rootPath };
    bindProjectFactsAuthority(scope, "identity-a\0revision-1");
    invokeMock.mockResolvedValue([]);
    await ensureProjectFacts(scope, ["agents"]);
    useProjectStore.setState({
      authority: {
        projectId,
        canonicalIdentityKey: "identity-a",
        identityRevision: "revision-2",
      } as never,
    });

    const { result } = renderHook(() =>
      useProjectStatus(projectId, rootPath, true, ["agents"]),
    );

    expect(result.current).toBeNull();
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });
});
