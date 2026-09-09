import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { WorkflowDraftForm } from "./WorkflowDraftForm";
import { useWorkflowStore } from "../../stores/workflowStore";
import { useNavigationStore } from "../../stores/navigationStore";
import type { WorkflowFormCatalog } from "../../types/workflow";
const picker = vi.hoisted(() => vi.fn().mockResolvedValue(null));
vi.mock("./workflowOutputPicker", () => ({ pickWorkflowOutputPath: picker }));
const api = vi.hoisted(() => ({ catalog: vi.fn() }));
vi.mock("../../services/workflowApi", () => ({ getWorkflowFormCatalog: api.catalog }));
vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (key: string) => key, i18n: { resolvedLanguage: "zh-CN" } }) }));
const project = { projectId: "project", rootPath: "/中文/知识库" };
const catalog: WorkflowFormCatalog = { kind: "generate_content", routes: [{ kind: "agent", agent: "codex" }], defaultRoute: null, wikiPages: ["wiki/中文.md", "wiki/Café.md"], rememberedDraft: null };
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>((done) => { resolve = done; }); return { promise, resolve }; }
const props = { project, onStart: vi.fn().mockResolvedValue(undefined), onBack: vi.fn(), onOpenLastHealth: vi.fn() };
beforeEach(() => { useWorkflowStore.getState().reset(); useNavigationStore.setState({ settingsOpen: false }); api.catalog.mockReset().mockResolvedValue(catalog); props.onStart.mockClear(); });
afterEach(cleanup);
describe("Workflow editable drafts", () => {
  it("uses single selection for a reading page and preserves the exact Unicode path", async () => {
    render(<WorkflowDraftForm {...props} kind="generate_content" />);
    const first = await screen.findByRole("radio", { name: "wiki/中文.md" });
    const second = screen.getByRole("radio", { name: "wiki/Café.md" });
    expect(screen.queryByRole("button", { name: "workflows.draft.selectFiltered" })).not.toBeInTheDocument();
    fireEvent.click(first);
    fireEvent.click(second);
    expect(first).not.toBeChecked();
    expect(second).toBeChecked();
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.start" }));
    expect(props.onStart).toHaveBeenCalledWith(expect.objectContaining({ scope: expect.objectContaining({ pagePaths: ["wiki/Café.md"] }) }));
  });
  it("can choose and submit Health before catalog loading finishes", () => {
    api.catalog.mockReturnValue(new Promise(() => {}));
    render(<WorkflowDraftForm {...props} kind="health_check" />);
    expect(screen.getByRole("radio", { name: "workflows.mode.localQuick" })).toBeEnabled();
    fireEvent.click(screen.getByRole("radio", { name: "workflows.mode.complete" }));
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.start" }));
    expect(props.onStart).toHaveBeenCalledWith({ scope: { kind: "health_check", mode: "complete" }, routeSelection: null });
  });
  it("does not replace edited mode with late remembered preferences", async () => {
    const pending = deferred<WorkflowFormCatalog>(); api.catalog.mockReturnValue(pending.promise);
    render(<WorkflowDraftForm {...props} kind="health_check" />);
    fireEvent.click(screen.getByRole("radio", { name: "workflows.mode.complete" }));
    await act(async () => pending.resolve({ ...catalog, kind: "health_check", rememberedDraft: { scope: { kind: "health_check", mode: "local_quick" }, routeSelection: null } }));
    expect(screen.getByRole("radio", { name: "workflows.mode.complete" })).toBeChecked();
  });
  it("restores remembered choices before any edits and retains them on remount", async () => {
    api.catalog.mockResolvedValue({ ...catalog, kind: "health_check", rememberedDraft: { scope: { kind: "health_check", mode: "complete" }, routeSelection: null } });
    const view = render(<WorkflowDraftForm {...props} kind="health_check" />);
    await act(async () => {});
    expect(screen.getByRole("radio", { name: "workflows.mode.complete" })).toBeChecked();
    view.unmount(); api.catalog.mockReturnValue(new Promise(() => {}));
    render(<WorkflowDraftForm {...props} kind="health_check" />);
    expect(screen.getByRole("radio", { name: "workflows.mode.complete" })).toBeChecked();
  });
  it("lets Generate switch type and save mode during directory loading", async () => {
    const pending = deferred<WorkflowFormCatalog>(); api.catalog.mockReturnValue(pending.promise);
    render(<WorkflowDraftForm {...props} kind="generate_content" />);
    expect(screen.getByRole("button", { name: "workflows.action.start" })).toBeDisabled();
    fireEvent.click(screen.getByRole("radio", { name: "workflows.artifact.projectReport" }));
    expect(screen.getByRole("button", { name: "workflows.action.start" })).toBeEnabled();
    fireEvent.click(screen.getByRole("radio", { name: "workflows.preparation.explicitTarget" }));
    picker.mockResolvedValueOnce("exports/报告.html");
    fireEvent.click(screen.getByRole("button", { name: "workflows.preparation.outputPath" }));
    await screen.findByText("exports/报告.html");
    await act(async () => pending.resolve(catalog));
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.start" }));
    expect(props.onStart).toHaveBeenCalledWith(expect.objectContaining({ scope: { kind: "generate_content", artifactType: "project_report", pagePaths: [], outputPath: "exports/报告.html" } }));
  });
  it("preserves exact selected pages and explicitly empty selection", async () => {
    render(<WorkflowDraftForm {...props} kind="generate_content" />);
    await screen.findByRole("radio", { name: "wiki/中文.md" });
    fireEvent.click(screen.getByRole("radio", { name: "workflows.artifact.knowledgeCard" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "wiki/中文.md" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "wiki/Café.md" }));
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.start" }));
    expect(props.onStart).toHaveBeenCalledWith(expect.objectContaining({ scope: expect.objectContaining({ pagePaths: catalog.wikiPages }) }));
    fireEvent.click(screen.getByRole("button", { name: "workflows.update.clear" }));
    expect(screen.getByRole("button", { name: "workflows.action.start" })).toBeDisabled();
  });
  it("keeps Local Quick usable when optional catalog fails", async () => {
    api.catalog.mockRejectedValue(new Error("unavailable"));
    render(<WorkflowDraftForm {...props} kind="health_check" />);
    await screen.findByText("workflows.draft.catalogError");
    expect(screen.getByRole("button", { name: "workflows.action.start" })).toBeEnabled();
  });
  it("ignores a late response after the form is unmounted", async () => {
    const pending = deferred<WorkflowFormCatalog>(); api.catalog.mockReturnValue(pending.promise);
    const view = render(<WorkflowDraftForm {...props} kind="health_check" />); view.unmount();
    await act(async () => pending.resolve({ ...catalog, rememberedDraft: { scope: { kind: "health_check", mode: "complete" }, routeSelection: null } }));
    expect(useWorkflowStore.getState().drafts.health_check).toBeUndefined();
  });
});
