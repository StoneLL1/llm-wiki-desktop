import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { WorkflowOutputPathPicker } from "./WorkflowOutputPathPicker";
import { useWorkflowStore } from "../../stores/workflowStore";

const pick = vi.hoisted(() => vi.fn());
vi.mock("./workflowOutputPicker", () => ({ pickWorkflowOutputPath: pick }));
vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
const project = { projectId: "a", rootPath: "/知识库" };
function deferred() { let resolve!: (path: string | null) => void; const promise = new Promise<string | null>((done) => { resolve = done; }); return { promise, resolve }; }
beforeEach(() => { pick.mockReset().mockResolvedValue(null); useWorkflowStore.getState().reset(); });

describe("workflow output picker", () => {
  it("shows a clickable file location, preserves it on cancel and supports choosing again", async () => {
    const change = vi.fn();
    render(<WorkflowOutputPathPicker project={project} value="exports/html/旧报告.html" onChange={change} />);
    const button = screen.getByRole("button", { name: "workflows.preparation.outputPath" });
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    await act(async () => fireEvent.click(button));
    expect(change).not.toHaveBeenCalled();
    expect(button).toHaveTextContent("旧报告.html");
    pick.mockResolvedValueOnce("exports/html/新报告.html");
    await act(async () => fireEvent.click(button));
    expect(change).toHaveBeenCalledWith("exports/html/新报告.html");
  });

  it("makes failures recoverable without losing the selected path", async () => {
    pick.mockRejectedValueOnce(new Error("outsideExportRoot"));
    const change = vi.fn();
    render(<WorkflowOutputPathPicker project={project} value="exports/html/报告.html" onChange={change} />);
    const button = screen.getByRole("button", { name: "workflows.preparation.outputPath" });
    await act(async () => fireEvent.click(button));
    expect(screen.getByRole("alert")).toHaveTextContent("workflows.outputPicker.outsideExportRoot");
    expect(change).not.toHaveBeenCalled();
    expect(button).toBeEnabled();
  });

  it.each(["unmount", "project", "epoch"])("ignores a late selection after %s", async (reason) => {
    const pending = deferred(); pick.mockReturnValue(pending.promise);
    const change = vi.fn();
    const view = render(<WorkflowOutputPathPicker project={project} value="" onChange={change} />);
    fireEvent.click(screen.getByRole("button", { name: "workflows.preparation.outputPath" }));
    expect(screen.getByRole("button")).toBeDisabled();
    if (reason === "unmount") view.unmount();
    if (reason === "project") view.rerender(<WorkflowOutputPathPicker project={{ projectId: "b", rootPath: "/另一个知识库" }} value="" onChange={change} />);
    if (reason === "epoch") act(() => { useWorkflowStore.getState().activateProject("b\0/另一个知识库"); });
    await act(async () => pending.resolve("exports/html/报告.html"));
    expect(change).not.toHaveBeenCalled();
    if (reason !== "unmount") expect(screen.getByRole("button")).toBeEnabled();
  });
});
