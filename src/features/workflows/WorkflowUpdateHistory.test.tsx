import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({ history: vi.fn(), undo: vi.fn() }));
const wiki = vi.hoisted(() => ({
  mode: "read", selectedPath: "wiki/当前.md", tree: { pages: [{ path: "wiki/当前.md" }] },
  scan: vi.fn(), openPage: vi.fn(),
}));
const invalidate = vi.hoisted(() => vi.fn());
vi.mock("../../services/workflowApi", () => ({ getWorkflowHistoryState: api.history, undoWorkflowUpdate: api.undo }));
vi.mock("../wiki/wikiStore", () => ({ useWikiStore: { getState: () => wiki } }));
vi.mock("../../stores/projectScope", async (importOriginal) => ({
  ...await importOriginal<typeof import("../../stores/projectScope")>(),
  invalidateProjectResources: invalidate,
}));
vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (key: string) => key }) }));

import { useProjectStore } from "../../stores/projectStore";
import { makeBaselineRun } from "./workflowBaselineFixtures";
import { WorkflowUpdateHistory } from "./WorkflowUpdateHistory";

const run = makeBaselineRun(0, { displayStatus: "completed", revision: "10" });
const history = { available: true, undone: false, recovery: false, undoInProgress: false, checkpointHash: "before", finalCommit: "after" };

beforeEach(() => {
  api.history.mockReset().mockResolvedValue(history);
  api.undo.mockReset().mockResolvedValue({ ...history, available: false, undone: true });
  wiki.mode = "read";
  wiki.selectedPath = "wiki/当前.md";
  wiki.scan.mockReset().mockResolvedValue(undefined);
  wiki.openPage.mockReset().mockResolvedValue(undefined);
  invalidate.mockClear();
  useProjectStore.setState({
    currentProject: { ...useProjectStore.getState().currentProject, projectId: run.projectId, rootPath: "D:/知识库" },
    authority: null,
  });
});

describe("Workflow update history", () => {
  it("offers recovery after an interrupted update without a final commit", async () => {
    api.history.mockResolvedValueOnce({ ...history, recovery: true, finalCommit: null });
    api.undo.mockResolvedValueOnce({ ...history, available: false, undone: true, recovery: true, finalCommit: null });
    render(<WorkflowUpdateHistory run={{ ...run, displayStatus: "interrupted", result: null }} onChanged={vi.fn()} />);
    const button = await screen.findByRole("button", { name: "workflows.updateHistory.restore" });
    expect(screen.getByText("workflows.updateHistory.recoveryNotice")).toBeInTheDocument();
    expect(api.undo).not.toHaveBeenCalled();
    fireEvent.click(button);
    expect(await screen.findByRole("status")).toHaveTextContent("workflows.updateHistory.restored");
    await waitFor(() => expect(wiki.scan).toHaveBeenCalledOnce());
  });

  it("continues a partially applied recovery using the same guarded mutation", async () => {
    api.history.mockResolvedValueOnce({ ...history, recovery: true, undoInProgress: true, finalCommit: null });
    let resolve!: (value: typeof history) => void;
    api.undo.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
    render(<WorkflowUpdateHistory run={{ ...run, displayStatus: "failed", result: null }} onChanged={vi.fn()} />);
    const button = await screen.findByRole("button", { name: "workflows.updateHistory.resume" });
    expect(screen.getByText("workflows.updateHistory.resumeNotice")).toBeInTheDocument();
    fireEvent.click(button);
    expect(screen.getByRole("button", { name: "workflows.updateHistory.recovering" })).toBeDisabled();
    expect(api.undo).toHaveBeenCalledExactlyOnceWith({ projectId: run.projectId, projectRootPath: "D:/知识库", taskId: run.taskId });
    await act(async () => resolve({ ...history, available: false, undone: true, recovery: true }));
    expect(screen.getByRole("status")).toHaveTextContent("workflows.updateHistory.restored");
  });

  it("reports recovery failures without claiming that the content was restored", async () => {
    api.history.mockResolvedValueOnce({ ...history, recovery: true, finalCommit: null });
    api.undo.mockRejectedValueOnce(new Error("write failed"));
    render(<WorkflowUpdateHistory run={{ ...run, displayStatus: "failed", result: null }} onChanged={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "workflows.updateHistory.restore" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("workflows.updateHistory.recoveryFailed");
    expect(screen.queryByText("workflows.updateHistory.restored")).not.toBeInTheDocument();
    expect(invalidate).not.toHaveBeenCalled();
  });

  it("loads once per task revision and undoes only after the user clicks", async () => {
    const onChanged = vi.fn();
    const view = render(<WorkflowUpdateHistory run={run} onChanged={onChanged} />);
    const button = await screen.findByRole("button", { name: "workflows.updateHistory.undo" });
    expect(api.undo).not.toHaveBeenCalled();
    view.rerender(<WorkflowUpdateHistory run={{ ...run }} onChanged={onChanged} />);
    expect(api.history).toHaveBeenCalledOnce();
    fireEvent.click(button);
    expect(await screen.findByRole("status")).toHaveTextContent("workflows.updateHistory.undone");
    expect(api.undo).toHaveBeenCalledExactlyOnceWith({ projectId: run.projectId, projectRootPath: "D:/知识库", taskId: run.taskId });
    expect(onChanged).toHaveBeenCalledOnce();
    await waitFor(() => expect(wiki.openPage).toHaveBeenCalledOnce());
    expect(invalidate).toHaveBeenCalledExactlyOnceWith({ projectId: run.projectId, rootPath: "D:/知识库" }, ["wiki", "graph"]);
    expect(wiki.scan).toHaveBeenCalledWith(run.projectId, "D:/知识库", expect.any(Function));
    expect(wiki.openPage).toHaveBeenCalledWith(run.projectId, "D:/知识库", "wiki/当前.md", expect.any(Function));
    expect(screen.queryByRole("button", { name: "workflows.updateHistory.undo" })).not.toBeInTheDocument();
  });

  it("reports a current-file conflict without overwriting or retrying automatically", async () => {
    api.undo.mockRejectedValueOnce({ code: "WORKFLOW_UNDO_CONFLICT", message: "changed" });
    const onChanged = vi.fn();
    render(<WorkflowUpdateHistory run={run} onChanged={onChanged} />);
    fireEvent.click(await screen.findByRole("button", { name: "workflows.updateHistory.undo" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("workflows.updateHistory.conflict");
    expect(api.undo).toHaveBeenCalledOnce();
    expect(onChanged).not.toHaveBeenCalled();
    expect(invalidate).not.toHaveBeenCalled();
    expect(wiki.scan).not.toHaveBeenCalled();
  });

  it("reloads durable recovery state after a partial undo fails", async () => {
    api.history.mockResolvedValueOnce(history).mockResolvedValueOnce({ ...history, undoInProgress: true });
    api.undo.mockRejectedValueOnce({ code: "WORKFLOW_UNDO_CONFLICT", message: "late edit" });
    render(<WorkflowUpdateHistory run={run} onChanged={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "workflows.updateHistory.undo" }));
    expect(await screen.findByRole("button", { name: "workflows.updateHistory.resume" })).toBeEnabled();
    expect(screen.getByRole("alert")).toHaveTextContent("workflows.updateHistory.conflict");
    expect(api.undo).toHaveBeenCalledOnce();
  });

  it("ignores delayed history from the previous project", async () => {
    let resolve!: (value: typeof history) => void;
    api.history.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
    render(<WorkflowUpdateHistory run={run} onChanged={vi.fn()} />);
    await waitFor(() => expect(api.history).toHaveBeenCalledOnce());
    act(() => useProjectStore.setState({ currentProject: { ...useProjectStore.getState().currentProject, projectId: "other", rootPath: "D:/other" } }));
    await act(async () => resolve(history));
    expect(screen.queryByRole("button", { name: "workflows.updateHistory.undo" })).not.toBeInTheDocument();
    expect(api.undo).not.toHaveBeenCalled();
  });

  it("coalesces repeated undo clicks while the first mutation is pending", async () => {
    let resolve!: (value: typeof history) => void;
    api.undo.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
    render(<WorkflowUpdateHistory run={run} onChanged={vi.fn()} />);
    const button = await screen.findByRole("button", { name: "workflows.updateHistory.undo" });
    fireEvent.click(button);
    fireEvent.click(button);
    expect(api.undo).toHaveBeenCalledOnce();
    expect(button).toBeDisabled();
    await act(async () => resolve({ ...history, available: false, undone: true }));
  });

  it("invalidates the Wiki without replacing a hidden editor's unsaved draft", async () => {
    wiki.mode = "edit";
    render(<WorkflowUpdateHistory run={run} onChanged={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "workflows.updateHistory.undo" }));
    await waitFor(() => expect(invalidate).toHaveBeenCalledOnce());
    expect(wiki.scan).not.toHaveBeenCalled();
    expect(wiki.openPage).not.toHaveBeenCalled();
  });

  it("does not refresh another project after a delayed undo completes", async () => {
    let resolve!: (value: typeof history) => void;
    api.undo.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
    const onChanged = vi.fn();
    render(<WorkflowUpdateHistory run={run} onChanged={onChanged} />);
    fireEvent.click(await screen.findByRole("button", { name: "workflows.updateHistory.undo" }));
    act(() => useProjectStore.setState({ currentProject: { ...useProjectStore.getState().currentProject, projectId: "other", rootPath: "D:/other" } }));
    await act(async () => resolve({ ...history, available: false, undone: true }));
    expect(onChanged).not.toHaveBeenCalled();
    expect(invalidate).not.toHaveBeenCalled();
    expect(wiki.scan).not.toHaveBeenCalled();
  });

  it("still refreshes the same project's files when the task detail was closed during undo", async () => {
    let resolve!: (value: typeof history) => void;
    api.undo.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
    const onChanged = vi.fn();
    const view = render(<WorkflowUpdateHistory run={run} onChanged={onChanged} />);
    fireEvent.click(await screen.findByRole("button", { name: "workflows.updateHistory.undo" }));
    view.unmount();
    await act(async () => resolve({ ...history, available: false, undone: true }));
    await waitFor(() => expect(wiki.scan).toHaveBeenCalledOnce());
    expect(onChanged).not.toHaveBeenCalled();
  });
});
