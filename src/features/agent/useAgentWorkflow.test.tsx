import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { AiCapabilitiesWorkflow } from "../../hooks/useAiCapabilities";
import type { TaskLauncher } from "../../hooks/useTaskLauncher";
import { useNavigationStore } from "../../stores/navigationStore";
import { defaultProject } from "../../stores/projectStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { useToastStore } from "../../stores/toastStore";
import type { AgentInfo } from "../../types/agent";
import type { Settings } from "../../types/settings";
import type { BackendTask } from "../../types/task";
import type { AgentSkill, RunAgentOptions } from "./RunAgentDialog";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import { useAgentWorkflow } from "./useAgentWorkflow";

const project = {
  ...defaultProject,
  projectId: "project-a",
  name: "Project A",
  rootPath: "D:/知识库/project-a",
};
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
const task: BackendTask = {
  id: "task-1",
  taskType: "wiki_compile",
  projectId: project.projectId,
  title: "Task",
  status: "queued",
  progress: null,
  startedAt: "2026-07-10T00:00:00Z",
  updatedAt: "2026-07-10T00:00:00Z",
  completedAt: null,
  cancellable: true,
  logPath: null,
  result: null,
  error: null,
};

let capabilities: AiCapabilitiesWorkflow;
let taskLauncher: TaskLauncher;
let refresh: ReturnType<typeof vi.fn<() => Promise<void>>>;
let loadSettings: ReturnType<
  typeof vi.fn<(projectId: string, rootPath: string) => Promise<Settings>>
>;

const options = (skill: AgentSkill): RunAgentOptions => ({
  skill,
  route: "agent",
  agent: "claude",
  provider: null,
  checkpoint: true,
  background: true,
});

beforeEach(() => {
  invokeMock.mockReset();
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: {},
    configurable: true,
  });
  refresh = vi.fn(async () => undefined);
  capabilities = {
    agents: [installedAgent],
    providers: [],
    refreshing: false,
    refresh,
  };
  taskLauncher = {
    startCompile: vi.fn().mockResolvedValue(task),
    startDeepLint: vi.fn().mockResolvedValue({ ...task, taskType: "deep_lint" }),
    startExport: vi.fn().mockResolvedValue({ ...task, taskType: "export" }),
    cancel: vi.fn(),
  };
  loadSettings = vi.fn(async () => useSettingsStore.getState().settings);
  useSettingsStore.setState({ loadSettings });
  useNavigationStore.setState({ activeView: "dashboard" });
  useToastStore.setState({ toasts: [] });
});

describe("useAgentWorkflow", () => {
  it("does not navigate or toast when an old-project task starts after project switch", async () => {
    let resolve!: (value: BackendTask) => void;
    vi.mocked(taskLauncher.startDeepLint).mockReturnValue(new Promise((next) => { resolve = next; }));
    const projectB = { ...project, projectId: "project-b", rootPath: "D:/wiki/project-b" };
    const { result, rerender } = renderHook(({ current }) =>
      useAgentWorkflow(current, capabilities, taskLauncher), { initialProps: { current: project } });
    const pending = result.current.runAgent(options("wiki-lint"));
    rerender({ current: projectB });
    resolve({ ...task, taskType: "deep_lint" });
    await act(async () => pending);
    expect(useNavigationStore.getState().activeView).toBe("dashboard");
    expect(useToastStore.getState().toasts).toEqual([]);
  });

  it("does not refresh or toast after an old-project default-Agent request settles", async () => {
    let reject!: (reason: Error) => void;
    invokeMock.mockReturnValue(new Promise((_, next) => { reject = next; }));
    const projectB = { ...project, projectId: "project-b", rootPath: "D:/wiki/project-b" };
    const { result, rerender } = renderHook(({ current }) =>
      useAgentWorkflow(current, capabilities, taskLauncher), { initialProps: { current: project } });
    const pending = result.current.setDefaultAgent("codex");
    rerender({ current: projectB });
    reject(new Error("old project failure"));
    await act(async () => pending);
    expect(refresh).not.toHaveBeenCalled();
    expect(useToastStore.getState().toasts).toEqual([]);
  });

  it("owns dialog preset state and derives the installed default Agent", () => {
    const { result } = renderHook(() =>
      useAgentWorkflow(project, capabilities, taskLauncher),
    );

    expect(result.current.defaultAgentKind).toBe("claude");
    act(() => result.current.openRunDialog("wiki-lint"));
    expect(result.current).toMatchObject({
      dialogOpen: true,
      dialogPreset: "wiki-lint",
    });
    act(() => result.current.closeRunDialog());
    expect(result.current.dialogOpen).toBe(false);
  });

  it("closes project-scoped Agent dialog state when the project changes", () => {
    const projectB = { ...project, projectId: "project-b", rootPath: "D:/wiki/project-b" };
    const { result, rerender } = renderHook(({ current }) =>
      useAgentWorkflow(current, capabilities, taskLauncher), { initialProps: { current: project } });
    act(() => result.current.openRunDialog("wiki-lint"));
    rerender({ current: projectB });
    expect(result.current.dialogOpen).toBe(false);
    expect(result.current.dialogPreset).toBeUndefined();
  });

  it("sets the default Agent then reloads settings and capabilities", async () => {
    invokeMock.mockResolvedValue(undefined);
    const { result } = renderHook(() =>
      useAgentWorkflow(project, capabilities, taskLauncher),
    );

    await act(async () => result.current.setDefaultAgent("codex"));

    expect(invokeMock).toHaveBeenCalledWith("set_default_agent", {
      request: {
        projectId: project.projectId,
        projectRootPath: project.rootPath,
        agent: "codex",
      },
    });
    expect(loadSettings).toHaveBeenCalledWith(project.projectId, project.rootPath);
    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it("reports a default-Agent failure without pretending to refresh", async () => {
    invokeMock.mockRejectedValue(new Error("agent config is locked"));
    const { result } = renderHook(() =>
      useAgentWorkflow(project, capabilities, taskLauncher),
    );

    await act(async () => result.current.setDefaultAgent("codex"));

    expect(refresh).not.toHaveBeenCalled();
    expect(useToastStore.getState().toasts[0]).toEqual(
      expect.objectContaining({
        tone: "error",
        message: "agent config is locked",
      }),
    );
  });

  it("routes wiki-ingest through the shared compile launcher", async () => {
    const { result } = renderHook(() =>
      useAgentWorkflow(project, capabilities, taskLauncher),
    );

    await act(async () => result.current.runAgent(options("wiki-ingest")));

    expect(taskLauncher.startCompile).toHaveBeenCalledWith({
      route: "agent",
      agent: "claude",
      provider: null,
    });
    expect(useToastStore.getState().toasts[0]).toEqual(
      expect.objectContaining({ tone: "info", message: expect.stringContaining("wiki-ingest") }),
    );
  });

  it("routes wiki-lint through deep lint and opens the Lint view", async () => {
    const { result } = renderHook(() =>
      useAgentWorkflow(project, capabilities, taskLauncher),
    );

    await act(async () => result.current.runAgent(options("wiki-lint")));

    expect(taskLauncher.startDeepLint).toHaveBeenCalledWith({
      route: "agent",
      agent: "claude",
      provider: null,
    });
    expect(useNavigationStore.getState().activeView).toBe("lint");
  });

  it("routes wiki-query to Chat without starting a task", async () => {
    const { result } = renderHook(() =>
      useAgentWorkflow(project, capabilities, taskLauncher),
    );

    await act(async () => result.current.runAgent(options("wiki-query")));

    expect(useNavigationStore.getState().activeView).toBe("chat");
    expect(taskLauncher.startCompile).not.toHaveBeenCalled();
    expect(taskLauncher.startDeepLint).not.toHaveBeenCalled();
    expect(taskLauncher.startExport).not.toHaveBeenCalled();
    expect(useToastStore.getState().toasts).toEqual([
      expect.objectContaining({ tone: "info", message: expect.any(String) }),
    ]);
  });

  it.each([
    "html-beautiful-read",
    "html-knowledge-card",
    "html-concept-map",
  ] as const)("routes single-page %s to Exports without starting a task", async (skill) => {
    const { result } = renderHook(() =>
      useAgentWorkflow(project, capabilities, taskLauncher),
    );

    await act(async () => result.current.runAgent(options(skill)));

    expect(useNavigationStore.getState().activeView).toBe("exports");
    expect(taskLauncher.startExport).not.toHaveBeenCalled();
  });

  it("starts the explicit project report export and opens Exports", async () => {
    const { result } = renderHook(() =>
      useAgentWorkflow(project, capabilities, taskLauncher),
    );

    await act(async () =>
      result.current.runAgent(options("html-project-report")),
    );

    expect(taskLauncher.startExport).toHaveBeenCalledWith(
      "project_report",
      null,
      { route: "agent", agent: "claude", provider: null },
    );
    expect(useNavigationStore.getState().activeView).toBe("exports");
  });
});
