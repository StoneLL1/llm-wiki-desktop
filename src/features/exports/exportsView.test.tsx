import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import "../../i18n";
import { useExportStore } from "../../stores/exportStore";
import { useProjectStore } from "../../stores/projectStore";
import { ExportsView } from "./ExportsView";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

const PROJECT = {
  projectId: "p",
  rootPath: "/x",
  name: "Test",
  agentRoute: "agent",
};

describe("ExportsView", () => {
  it("renders the empty state and generate control before any export", () => {
    useExportStore.getState().reset();
    useProjectStore.setState({ currentProject: PROJECT } as never);
    render(<ExportsView />);
    expect(screen.getByRole("button", { name: /^Generate$/ })).toBeInTheDocument();
    expect(screen.getByText(/Run an export/i)).toBeInTheDocument();
  });

  it("lists prior exports with their output path", () => {
    useExportStore.getState().reset();
    useExportStore.setState({
      records: [
        {
          id: "export-1",
          exportType: "beautiful_read",
          title: "Agent",
          sourcePath: "wiki/concepts/agent.md",
          outputPath: "exports/html/agent-1.html",
          createdAt: "2026-06-20T10:00:00Z",
          route: "byok",
          status: "succeeded",
        },
      ],
    });
    useProjectStore.setState({ currentProject: PROJECT } as never);
    render(<ExportsView />);
    expect(screen.getByText("Agent")).toBeInTheDocument();
    expect(screen.getByText("exports/html/agent-1.html")).toBeInTheDocument();
  });
});
