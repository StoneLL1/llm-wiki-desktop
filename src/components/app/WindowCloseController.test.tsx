import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import { useWikiStore } from "../../features/wiki/wikiStore";
import { setSettingsDraftDirty } from "../../lib/settingsDraftGuard";
import { useTaskStore } from "../../stores/taskStore";
import { useWorkflowStore } from "../../stores/workflowStore";
import type { BackendTask } from "../../types/task";

const mocks = vi.hoisted(() => ({
  getState: vi.fn(),
  resolve: vi.fn(),
  listen: vi.fn(),
}));
vi.mock("../../services/windowApi", () => ({ getWindowCloseState: mocks.getState, resolveWindowClose: mocks.resolve }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));

import { WindowCloseController } from "./WindowCloseController";

let closeRequested: () => void;
const text = (key: string) => String(i18next.t(key));
const task: BackendTask = {
  id: "task-1", taskType: "llm_request", projectId: "another-project", title: "Writing a summary",
  status: "running", progress: null, startedAt: "2026-10-11T00:00:00Z", updatedAt: "2026-10-11T00:00:00Z",
  completedAt: null, cancellable: true, logPath: null, result: null, error: null,
};

beforeEach(() => {
  vi.clearAllMocks();
  Object.defineProperty(window, "__TAURI_INTERNALS__", { configurable: true, value: {} });
  mocks.listen.mockImplementation(async (_channel: string, callback: () => void) => {
    closeRequested = callback;
    return vi.fn();
  });
  mocks.getState.mockResolvedValue({ intent: "ask", activeTasks: [] });
  mocks.resolve.mockResolvedValue(undefined);
  useWikiStore.getState().reset();
  useTaskStore.setState({ taskById: {}, tasks: [] });
  useWorkflowStore.setState({ runs: [] });
  setSettingsDraftDirty("test", false);
});

describe("WindowCloseController", () => {
  it("recovers an Ask request before mount, deduplicates requests, and cancels without quitting", async () => {
    render(<WindowCloseController />);
    expect(await screen.findByRole("dialog")).toBeVisible();
    await act(async () => { closeRequested(); closeRequested(); });
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
    expect(mocks.resolve).not.toHaveBeenCalled();
    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(mocks.resolve).toHaveBeenCalledWith(expect.objectContaining({ action: "cancel" }));
  });

  it("keeps unsaved settings and tasks when choosing background", async () => {
    setSettingsDraftDirty("test", true);
    mocks.getState.mockResolvedValue({ intent: "ask", activeTasks: [task] });
    render(<WindowCloseController />);
    expect(await screen.findByText(task.title)).toBeVisible();
    expect(screen.getByRole("button", { name: text("windowClose.quit") })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: text("windowClose.background") }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(mocks.resolve).toHaveBeenCalledWith({ action: "background", unsavedChanges: true, discardUnsavedChanges: false, editorSaving: false });
  });

  it("requires explicit discard before quitting and rechecks current dirty state", async () => {
    setSettingsDraftDirty("test", true);
    render(<WindowCloseController />);
    const quit = await screen.findByRole("button", { name: text("windowClose.quit") });
    expect(quit).toBeDisabled();
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(quit);
    await waitFor(() => expect(mocks.resolve).toHaveBeenCalledWith({ action: "quit", unsavedChanges: true, discardUnsavedChanges: true, editorSaving: false }));
  });

  it("prevents quitting during saving or a critical task but still allows background", async () => {
    useWikiStore.setState({ saveState: "saving" });
    mocks.getState.mockResolvedValue({ intent: "ask", activeTasks: [{ ...task, cancellable: false }] });
    render(<WindowCloseController />);
    expect(await screen.findByRole("button", { name: text("windowClose.quit") })).toBeDisabled();
    expect(screen.getByRole("button", { name: text("windowClose.background") })).toBeEnabled();
    await act(async () => {
      useWikiStore.setState({ saveState: "idle" });
      useTaskStore.setState({ taskById: { [task.id]: { ...task, status: "succeeded", updatedAt: "2026-10-11T00:01:00Z" } } });
    });
    expect(screen.getByRole("button", { name: text("windowClose.quit") })).toBeEnabled();
  });

  it("honors Quit when there is no work to review", async () => {
    mocks.getState.mockResolvedValue({ intent: "quit", activeTasks: [] });
    render(<WindowCloseController />);
    await waitFor(() => expect(mocks.resolve).toHaveBeenCalledWith({ action: "quit", unsavedChanges: false, discardUnsavedChanges: false, editorSaving: false }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("keeps the dialog open after a native guard rejection without automatic retries", async () => {
    mocks.getState.mockResolvedValue({ intent: "quit", activeTasks: [] });
    mocks.resolve.mockRejectedValueOnce({ code: "UPDATE_INSTALL_IN_PROGRESS" });
    render(<WindowCloseController />);
    expect(await screen.findByRole("alert")).toHaveTextContent(text("windowClose.waitForOperation"));
    await act(async () => { closeRequested(); });
    expect(mocks.resolve).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: text("windowClose.background") }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });
});
