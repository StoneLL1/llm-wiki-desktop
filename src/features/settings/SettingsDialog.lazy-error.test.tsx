import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { defaultProject } from "../../stores/projectStore";

vi.mock("./SettingsView", () => { throw new Error("settings chunk rejected"); });

import { SettingsDialog } from "./SettingsDialog";

describe("SettingsDialog lazy boundary", () => {
  it("contains a rejected Settings chunk inside the dialog", async () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    render(<SettingsDialog open onClose={vi.fn()} project={{ ...defaultProject, projectId: "p1", rootPath: "/p1" }}
      providers={[]} agents={[]} onRefreshCapabilities={vi.fn()} onSaveProvider={vi.fn()}
      onSaveSecret={vi.fn()} onDeleteSecret={vi.fn()} onTestProvider={vi.fn()} />);
    expect(await screen.findByRole("alert")).toBeInTheDocument();
  });
});
