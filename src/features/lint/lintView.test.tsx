import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import "../../i18n";
import { useLintStore } from "../../stores/lintStore";
import { useProjectStore } from "../../stores/projectStore";
import { LintView } from "./LintView";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

const PROJECT = {
  projectId: "p",
  rootPath: "/x",
  name: "Test",
  agentRoute: "agent",
};

describe("LintView", () => {
  it("renders the empty state and toolbar before any lint run", () => {
    useLintStore.getState().reset();
    useProjectStore.setState({ currentProject: PROJECT } as never);
    render(<LintView />);
    expect(screen.getByText(/Run local lint/i)).toBeInTheDocument();
    expect(screen.getByText(/Run a lint pass to see findings/i)).toBeInTheDocument();
  });

  it("renders grouped local findings with their type label", () => {
    useLintStore.getState().reset();
    useLintStore.setState({
      localReport: {
        issues: [
          {
            id: "dead_link:wiki/a.md:ghost",
            source: "local",
            severity: "warning",
            issueType: "dead_link",
            path: "wiki/a.md",
            message: "Unresolved",
            target: "ghost",
            fixability: "high_risk",
          },
        ],
        generatedAt: "2026-06-20T00:00:00Z",
        scannedPages: 1,
      },
    });
    useProjectStore.setState({ currentProject: PROJECT } as never);
    render(<LintView />);
    expect(screen.getByText(/Dead link/i)).toBeInTheDocument();
    expect(screen.getByText("wiki/a.md")).toBeInTheDocument();
  });
});
