import { fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";

import "../../i18n";
import { useLintStore } from "../../stores/lintStore";
import { useProjectStore } from "../../stores/projectStore";
import { LintView } from "./LintView";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue({ ignored: [] }),
}));

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

const PROJECT = {
  projectId: "p",
  rootPath: "/x",
  name: "Test",
  agentRoute: "agent",
};

describe("LintView", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue({ ignored: [] });
    useLintStore.getState().reset();
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });
  });

  it("renders the empty state and toolbar before any lint run", () => {
    useProjectStore.setState({ currentProject: PROJECT } as never);
    render(<LintView />);
    expect(screen.getByText(/Run local lint/i)).toBeInTheDocument();
    expect(screen.getByText(/Run a lint pass to see findings/i)).toBeInTheDocument();
  });

  it("exposes a resizable lint issue list splitter", () => {
    useProjectStore.setState({ currentProject: PROJECT } as never);
    render(<LintView />);
    expect(screen.getByRole("separator", { name: "Resize lint issue list" })).toHaveAttribute("aria-valuenow", "360");
  });

  it("renders grouped local findings with their type label and tags", () => {
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

  it("loads lint history and opens the latest report on mount", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "list_lint_ignores") return Promise.resolve({ ignored: [] });
      if (command === "list_lint_history") {
        return Promise.resolve({
          version: 1,
          entries: [
            {
              id: "local-1",
              kind: "local",
              createdAt: "2026-07-04T00:00:00Z",
              issueCount: 1,
              errorCount: 1,
              warningCount: 0,
              infoCount: 0,
              scannedPages: 3,
              taskId: null,
              route: null,
            },
          ],
        });
      }
      if (command === "read_lint_history_report") {
        return Promise.resolve({
          entry: {
            id: "local-1",
            kind: "local",
            createdAt: "2026-07-04T00:00:00Z",
            issueCount: 1,
            errorCount: 1,
            warningCount: 0,
            infoCount: 0,
          },
          localReport: {
            issues: [],
            generatedAt: "2026-07-04T00:00:00Z",
            scannedPages: 3,
          },
          deepReport: null,
        });
      }
      return Promise.resolve({ ignored: [] });
    });
    useProjectStore.setState({ currentProject: PROJECT } as never);

    render(<LintView />);

    expect(await screen.findByRole("button", { name: /Local lint/i })).toBeInTheDocument();
    await vi.waitFor(() =>
      expect(useLintStore.getState().localReport?.scannedPages).toBe(3),
    );
  });

  it("keeps the history list visible when one history report cannot be opened", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "list_lint_ignores") return Promise.resolve({ ignored: [] });
      if (command === "list_lint_history") {
        return Promise.resolve({
          version: 1,
          entries: [
            {
              id: "bad",
              kind: "local",
              createdAt: "2026-07-04T00:00:00Z",
              issueCount: 1,
              errorCount: 1,
              warningCount: 0,
              infoCount: 0,
            },
          ],
        });
      }
      if (command === "read_lint_history_report") {
        return Promise.reject({ message: "bad json" });
      }
      return Promise.resolve({ ignored: [] });
    });

    useProjectStore.setState({ currentProject: PROJECT } as never);
    render(<LintView />);

    const row = await screen.findByRole("button", { name: /Local lint/i });
    fireEvent.click(row);
    expect(await screen.findByRole("status")).toHaveTextContent("bad json");
  });
});
