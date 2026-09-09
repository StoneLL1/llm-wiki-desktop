import { act, renderHook } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

import { resetProjectFactsStoreForTests } from "../../stores/projectFactsStore";
import { useProjectStore } from "../../stores/projectStore";
import { useLintGitPreflight } from "./useLintGitPreflight";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);
const ready = { enabled: true, git: { isRepository: true, head: null, branch: null, hasChanges: true }, gitVersion: "git version test" };

beforeEach(() => {
  invokeMock.mockReset();
  resetProjectFactsStoreForTests();
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
  useProjectStore.setState({ currentProject: { projectId: "p", rootPath: "/知识库" }, authority: null } as never);
});

it("refreshes protection before every attempt and permits private history without HEAD", async () => {
  invokeMock.mockResolvedValue(ready);
  const { result } = renderHook(() => useLintGitPreflight("p", "/知识库"));
  await act(async () => { expect(await result.current.check()).toBe(true); });
  invokeMock.mockResolvedValue({ ...ready, enabled: false });
  await act(async () => { expect(await result.current.check()).toBe(false); });
  expect(result.current.error?.code).toBe("VERSION_NOT_ENABLED");
  expect(invokeMock.mock.calls.map(([command]) => command)).toEqual(["get_version_history_status", "get_version_history_status"]);
});

it("drops an old project's result and suppresses duplicate clicks while checking", async () => {
  let finish!: (value: unknown) => void;
  invokeMock.mockImplementation(() => new Promise((resolve) => { finish = resolve; }));
  const { result, rerender } = renderHook(({ id, root }) => useLintGitPreflight(id, root), {
    initialProps: { id: "p", root: "/知识库" },
  });
  let pending!: Promise<boolean>;
  await act(async () => {
    pending = result.current.check();
    expect(await result.current.check()).toBe(false);
  });
  expect(result.current.checking).toBe(true);
  useProjectStore.setState({ currentProject: { projectId: "other", rootPath: "/other" } } as never);
  rerender({ id: "other", root: "/other" });
  await act(async () => {
    finish(ready);
    expect(await pending).toBe(false);
  });
  expect(result.current.checking).toBe(false);
  expect(result.current.error).toBeNull();
  expect(invokeMock).toHaveBeenCalledTimes(1);
});

it("does not resume an action when project identity changes during enablement", async () => {
  let finish!: (value: unknown) => void;
  invokeMock.mockImplementation((command) => command === "prepare_version_action"
    ? new Promise((resolve) => { finish = resolve; }) : Promise.resolve(undefined));
  const { result } = renderHook(() => useLintGitPreflight("p", "/知识库"));
  let pending!: Promise<boolean>;
  await act(async () => { pending = result.current.enable(); });
  useProjectStore.setState({ authority: { canonicalIdentityKey: "replaced", identityRevision: "2" } } as never);
  await act(async () => {
    finish({ id: "stale-enable" });
    expect(await pending).toBe(false);
  });
  expect(invokeMock).toHaveBeenCalledWith("confirm_version_action", { request: { actionId: "stale-enable", status: "cancelled" } });
});
