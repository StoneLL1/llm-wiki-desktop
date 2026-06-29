import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import "../../i18n";
import { useLintStore } from "../../stores/lintStore";
import { useProjectStore } from "../../stores/projectStore";
import { LintView } from "./LintView";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue({ ignored: [] }),
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

  it("renders grouped local findings with their type label and tags", () => {
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
    // Type label appears both as the card title and as a tag badge.
    expect(screen.getAllByText(/Dead link/i).length).toBeGreaterThan(0);
    // The path is rendered inside the combined sub-line.
    expect(screen.getByText(/wiki\/a\.md/)).toBeInTheDocument();
    // High-risk issues surface a "Details" inline action, not a direct "Fix".
    expect(screen.getByText("Details")).toBeInTheDocument();
  });

  it("renders the auto-fix CTA with a count of fixable issues", () => {
    useLintStore.getState().reset();
    useLintStore.setState({
      localReport: {
        issues: [
          {
            id: "missing_frontmatter:wiki/a.md",
            source: "local",
            severity: "warning",
            issueType: "missing_frontmatter",
            path: "wiki/a.md",
            message: "No frontmatter",
            fixability: "safe",
          },
        ],
        generatedAt: "2026-06-20T00:00:00Z",
        scannedPages: 1,
      },
    });
    useProjectStore.setState({ currentProject: PROJECT } as never);
    render(<LintView />);
    expect(screen.getByRole("button", { name: /Auto-fix \(1\)/i })).toBeEnabled();
  });

  it("renders the four-up summary cards and passed checks when a report exists", () => {
    useLintStore.getState().reset();
    useLintStore.setState({
      localReport: {
        issues: [
          {
            id: "dead_link:wiki/a.md:ghost",
            source: "local",
            severity: "error",
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
    // "Errors" doubles as the summary label and the severity badge on the card.
    expect(screen.getAllByText("Errors").length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("Warnings")).toBeInTheDocument();
    expect(screen.getByText("Passed")).toBeInTheDocument();
    expect(screen.getByText(/Passed checks/i)).toBeInTheDocument();
  });
});
