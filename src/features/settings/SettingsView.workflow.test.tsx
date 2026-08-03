import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { defaultProject } from "../../stores/projectStore";
import type { ImportWorkflow } from "../import/importWorkflow";

const mocks = vi.hoisted(() => ({ saveSecret: vi.fn(), refresh: vi.fn() }));

vi.mock("./AiSettings", () => ({
  AiSettings: (props: { onSaveSecret: (provider: "anthropic", secret: string) => Promise<unknown> }) => (
    <button type="button" onClick={() => void props.onSaveSecret("anthropic", "never-rendered")}>Save workflow secret</button>
  ),
}));
vi.mock("./AppearanceSettings", () => ({ AppearanceSettings: () => null }));
vi.mock("./BackgroundTaskSettings", () => ({ BackgroundTaskSettings: () => null }));
vi.mock("./LanguageSettings", () => ({ LanguageSettings: () => null }));
vi.mock("./SecuritySettings", () => ({ SecuritySettings: () => null }));
vi.mock("./UpdateSettings", () => ({ UpdateSettings: () => null }));
vi.mock("./ImportCompatibilitySettings", () => ({
  ImportCompatibilitySettings: ({ workflow }: { workflow: ImportWorkflow | null }) => (
    <div>{workflow ? `Compatibility workflow ${workflow.projectKey}` : "Compatibility unavailable"}</div>
  ),
}));

import { SettingsView } from "./SettingsView";

beforeEach(() => {
  mocks.refresh.mockReset().mockResolvedValue(undefined);
  mocks.saveSecret.mockReset().mockImplementation(async () => { await mocks.refresh(); });
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
});

describe("SettingsView workflow ownership", () => {
  it("opens directly to the requested AI section", async () => {
    render(<SettingsView initialSection="ai" project={{ ...defaultProject, projectId: "p1", name: "P1", rootPath: "/p1" }} providers={[]} agents={[]}
      onRefreshCapabilities={mocks.refresh} onSaveProvider={vi.fn()} onSaveSecret={mocks.saveSecret}
      onDeleteSecret={vi.fn()} onTestProvider={vi.fn()} />);

    expect(await screen.findByRole("button", { name: "Save workflow secret" })).toBeInTheDocument();
  });

  it("lets the provider workflow own the single capability refresh after secret save", async () => {
    render(<SettingsView project={{ ...defaultProject, projectId: "p1", name: "P1", rootPath: "/p1" }} providers={[]} agents={[]}
      onRefreshCapabilities={mocks.refresh} onSaveProvider={vi.fn()} onSaveSecret={mocks.saveSecret}
      onDeleteSecret={vi.fn()} onTestProvider={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /AI/i }));
    fireEvent.click(await screen.findByRole("button", { name: "Save workflow secret" }));
    await waitFor(() => expect(mocks.saveSecret).toHaveBeenCalledTimes(1));
    expect(mocks.refresh).toHaveBeenCalledTimes(1);
  });

  it("exposes legacy import review only through the Settings compatibility section", () => {
    const importWorkflow = { projectKey: "p1\u0000/p1" } as ImportWorkflow;
    render(<SettingsView project={{ ...defaultProject, projectId: "p1", name: "P1", rootPath: "/p1" }} providers={[]} agents={[]}
      onRefreshCapabilities={mocks.refresh} onSaveProvider={vi.fn()} onSaveSecret={mocks.saveSecret}
      onDeleteSecret={vi.fn()} onTestProvider={vi.fn()} importWorkflow={importWorkflow} />);

    expect(screen.queryByText(/Compatibility workflow/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Compatibility/i }));
    expect(screen.getByText("Compatibility workflow p1\u0000/p1")).toBeInTheDocument();
  });
});
