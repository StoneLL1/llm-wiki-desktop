import { fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";

import "../../i18n";
import { useLintStore } from "../../stores/lintStore";
import { useProjectStore } from "../../stores/projectStore";
import { LintIssueDetails } from "./LintIssueDetails";
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

  it("exposes a resizable lint details splitter", () => {
    useProjectStore.setState({ currentProject: PROJECT } as never);
    render(<LintView />);
    expect(screen.getByRole("separator", { name: "Resize lint issue details" })).toHaveAttribute("aria-valuenow", "320");
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
            scanHash: "hash-a",
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
            scanHash: "hash-a",
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

  it("renders a merged Health Check finding once while preserving both origins", () => {
    const issue = {
      id: "schema_mismatch:wiki/主题.md",
      source: "local" as const,
      severity: "error" as const,
      issueType: "schema_mismatch" as const,
      path: "wiki/主题.md",
      message: "Merged schema finding",
      evidence: "local and deep evidence",
      fixability: "none" as const,
    };
    useLintStore.setState({
      healthReport: {
        reportId: "health-1",
        taskId: "health-1",
        mode: "complete",
        route: {
          kind: "byok",
          provider: "ollama",
          model: "qwen-health",
          routeRevision: "route-1",
        },
        persistent: false,
        issues: [issue],
        findingOrigins: { [issue.id]: ["local", "agent"] },
        coverage: {
          scannedPages: 1,
          sourcePages: 0,
          wikiPages: 1,
          notApplicableRules: ["index_drift"],
        },
        errorCount: 1,
        warningCount: 0,
        infoCount: 0,
        findingsByType: { schema_mismatch: 1 },
        durationMs: 10,
        generatedAt: "2026-08-01T00:00:00Z",
      },
    });
    useProjectStore.setState({ currentProject: PROJECT } as never);

    render(<LintView />);

    expect(screen.getByRole("button", { name: "All 1" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Local 1" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Agent deep 1" })).toBeInTheDocument();
    expect(screen.getAllByText("wiki/主题.md")).toHaveLength(1);
    expect(screen.queryByText("index.md consistent")).not.toBeInTheDocument();
    fireEvent.click(screen.getByText("wiki/主题.md").closest("button")!);
    expect(screen.getAllByText("Local").length).toBeGreaterThanOrEqual(2);
    expect(screen.getAllByText("Agent").length).toBeGreaterThanOrEqual(2);
  });

  it("labels a memory-only Health Check history entry as not saved", () => {
    useLintStore.setState({
      history: [
        {
          id: "health-memory",
          kind: "health_check",
          createdAt: "2026-08-01T00:00:00Z",
          issueCount: 0,
          errorCount: 0,
          warningCount: 0,
          infoCount: 0,
          persistent: false,
        },
      ],
    });
    useProjectStore.setState({ currentProject: PROJECT } as never);

    render(<LintView />);

    expect(screen.getByText("Not saved")).toBeInTheDocument();
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

  it("offers a restore action for persisted ignores and refreshes local lint", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "list_lint_ignores") {
        return Promise.resolve({
          ignored: [{ path: "wiki/a.md", rule: "dead_link", createdAt: "2026-07-04T00:00:00Z" }],
        });
      }
      if (command === "list_lint_history") return Promise.resolve({ version: 1, entries: [] });
      if (command === "remove_lint_ignore") return Promise.resolve({ ignored: [] });
      if (command === "run_local_lint") {
        return Promise.resolve({ issues: [], generatedAt: "2026-07-04T00:00:00Z", scannedPages: 1 });
      }
      return Promise.resolve({ ignored: [] });
    });
    useProjectStore.setState({ currentProject: PROJECT } as never);

    render(<LintView />);

    const restore = await screen.findByRole("button", { name: "Restore" });
    fireEvent.click(restore);
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledWith("remove_lint_ignore", expect.anything()));
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledWith("run_local_lint", expect.anything()));
  });

  it("defaults a historical fixable issue without a scan hash to ignore", () => {
    render(
      <LintIssueDetails
        issue={{
          id: "missing_frontmatter:wiki/a.md",
          source: "local",
          severity: "warning",
          issueType: "missing_frontmatter",
          path: "wiki/a.md",
          message: "No frontmatter",
          fixability: "safe",
          scanHash: null,
        }}
        fixStatus="idle"
        fixConfirm={null}
        ignoring={false}
        safetyPrefs={{ checkpoint: true, commitAfter: true, recompile: false }}
        onSafetyPrefsChange={vi.fn()}
        onApplyFix={vi.fn()}
        onConfirmHighRisk={vi.fn()}
        onCancelHighRisk={vi.fn()}
        onIgnore={vi.fn()}
      />,
    );
    expect(screen.getByRole("radio", { name: /Apply fix/i })).toBeDisabled();
    expect(screen.getByRole("radio", { name: /Ignore this time/i })).toBeChecked();
  });
});
