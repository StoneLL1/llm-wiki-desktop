import { fireEvent, render, screen, waitFor, cleanup } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { UpdateWikiForm } from "./UpdateWikiForm";
import { useWorkflowStore } from "../../stores/workflowStore";
import { useNavigationStore } from "../../stores/navigationStore";
import type { UpdateWikiOptions, UpdateWikiSourcePage } from "../../types/workflow";
const api = vi.hoisted(() => ({ options: vi.fn(), sources: vi.fn() }));
vi.mock("../../services/workflowApi", () => ({ getUpdateWikiOptions: api.options, listUpdateWikiSources: api.sources }));
vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
const project = { projectId: "a", rootPath: "/项目/知识库" };
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>((done) => { resolve = done; }); return { promise, resolve }; }
const page: UpdateWikiSourcePage = { sources: [{ sourceId: "a", versionId: "v1", title: "资料 A", consumed: false }], total: 1, nextOffset: null, unavailable: 1 };
beforeEach(() => { useWorkflowStore.getState().reset(); useNavigationStore.setState({ settingsOpen: false }); api.options.mockReset().mockResolvedValue({ routes: [], defaultRoute: null }); api.sources.mockReset().mockResolvedValue(page); });
afterEach(cleanup);
describe("Update Wiki intent form", () => {
  it("keeps remote consent visible while execution settings are collapsed", async () => {
    api.options.mockResolvedValue({ routes: [{ kind: "byok", provider: "openai" }], defaultRoute: { kind: "byok", provider: "openai" } });
    const start = vi.fn().mockResolvedValue(undefined);
    render(<UpdateWikiForm project={project} onBack={vi.fn()} onStart={start} />);
    const consent = await screen.findByRole("checkbox", { name: "workflows.update.remote" });
    expect(consent).toBeVisible();
    expect(screen.getByText("workflows.preparation.executionDetails").closest("details")).not.toHaveAttribute("open");
    fireEvent.click(consent);
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.start" }));
    expect(start).toHaveBeenCalledWith(expect.objectContaining({ acknowledgeRemoteProvider: true }));
  });
  it("allows editing and submitting while route discovery is unresolved, without reading sources", () => {
    api.options.mockReturnValue(new Promise(() => {}));
    const start = vi.fn().mockResolvedValue(undefined);
    render(<UpdateWikiForm project={project} onBack={vi.fn()} onStart={start} />);
    fireEvent.click(screen.getByRole("radio", { name: "workflows.mode.fullRecompile" }));
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.start" }));
    expect(start).toHaveBeenCalledWith(expect.objectContaining({ mode: "full_recompile", selection: { kind: "automatic" } }));
    expect(api.sources).not.toHaveBeenCalled();
  });
  it("keeps automatic intent after remount, instead of pinning last run's source versions", () => {
    const renderForm = () => <UpdateWikiForm project={project} onBack={vi.fn()} onStart={vi.fn()} />;
    const first = render(renderForm()); first.unmount(); render(renderForm());
    expect(screen.getByRole("radio", { name: "workflows.update.automatic" })).toBeChecked();
    expect(api.sources).not.toHaveBeenCalled();
  });
  it("treats explicitly empty selection as empty and does not let a late list overwrite edits", async () => {
    const listing = deferred<UpdateWikiSourcePage>(); api.sources.mockReturnValue(listing.promise);
    const start = vi.fn(); render(<UpdateWikiForm project={project} onBack={vi.fn()} onStart={start} />);
    fireEvent.click(screen.getByRole("radio", { name: "workflows.update.manual" }));
    expect(screen.getByRole("button", { name: "workflows.action.start" })).toBeDisabled();
    fireEvent.click(screen.getByRole("radio", { name: "workflows.mode.fullRecompile" }));
    listing.resolve(page);
    await screen.findByText("资料 A");
    expect(screen.getByRole("radio", { name: "workflows.mode.fullRecompile" })).toBeChecked();
    fireEvent.click(screen.getByRole("checkbox", { name: "资料 A" }));
    fireEvent.click(screen.getByRole("button", { name: "workflows.action.start" }));
    expect(start).toHaveBeenCalledWith(expect.objectContaining({ selection: { kind: "selected", sourceVersions: [{ sourceId: "a", versionId: "v1" }] } }));
    fireEvent.click(screen.getByRole("button", { name: "workflows.update.clear" }));
    expect(screen.getByRole("button", { name: "workflows.action.start" })).toBeDisabled();
  });
  it("reuses the same request ID after an uncertain submission, and changes it for changed choices", () => {
    const start = vi.fn().mockResolvedValue(undefined); render(<UpdateWikiForm project={project} onBack={vi.fn()} onStart={start} />);
    const button = screen.getByRole("button", { name: "workflows.action.start" });
    fireEvent.click(button); fireEvent.click(button);
    expect(start.mock.calls[1]?.[0].requestId).toEqual(start.mock.calls[0]?.[0].requestId);
    fireEvent.click(screen.getByRole("radio", { name: "workflows.mode.fullRecompile" })); fireEvent.click(button);
    expect(start.mock.calls[2]?.[0].requestId).not.toEqual(start.mock.calls[0]?.[0].requestId);
  });
  it("ignores a directory response after leaving manual selection", async () => {
    const listing = deferred<UpdateWikiSourcePage>(); api.sources.mockReturnValue(listing.promise);
    render(<UpdateWikiForm project={project} onBack={vi.fn()} onStart={vi.fn()} />);
    fireEvent.click(screen.getByRole("radio", { name: "workflows.update.manual" }));
    fireEvent.click(screen.getByRole("radio", { name: "workflows.update.automatic" }));
    listing.resolve(page);
    await waitFor(() => expect(screen.queryByText("资料 A")).not.toBeInTheDocument());
    expect(useWorkflowStore.getState().updateDraft.selection).toEqual({ kind: "automatic" });
  });
  it("does not reset the draft when environment facts arrive", async () => {
    const options = deferred<UpdateWikiOptions>(); api.options.mockReturnValue(options.promise);
    render(<UpdateWikiForm project={project} onBack={vi.fn()} onStart={vi.fn()} />);
    fireEvent.click(screen.getByRole("radio", { name: "workflows.mode.fullRecompile" }));
    options.resolve({ routes: [{ kind: "agent", agent: "codex" }], defaultRoute: { kind: "agent", agent: "codex" } });
    fireEvent.click(screen.getByText("workflows.preparation.executionDetails"));
    await screen.findByRole("option", { name: "Codex · workflows.route.agent" });
    expect(screen.getByRole("radio", { name: "workflows.mode.fullRecompile" })).toBeChecked();
  });
});
