import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";

import "../../i18n";
import { useLintStore } from "../../stores/lintStore";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { resetProjectFactsStoreForTests } from "../../stores/projectFactsStore";
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
    invokeMock.mockImplementation((command: string) => Promise.resolve(command === "get_version_history_status"
      ? { enabled: true, git: { isRepository: true, head: "head-a", branch: "main", hasChanges: false } }
      : { ignored: [] }));
    resetProjectFactsStoreForTests();
    useLintStore.getState().reset();
    useNavigationStore.setState({
      activeView: "lint",
      workflowLaunchIntent: null,
    });
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });
  });

  it("renders the empty state and toolbar before any lint run", async () => {
    useProjectStore.setState({ currentProject: PROJECT } as never);
    render(<LintView />);
    expect(screen.getByText(/Quick check/i)).toBeInTheDocument();
    expect(await screen.findByText(/Run a lint pass to see findings/i)).toBeInTheDocument();
  });

  it("searches localized issue labels and paths, scopes batch fixes, and clears a hidden detail", () => {
    useProjectStore.setState({ currentProject: PROJECT } as never);
    useLintStore.setState({ localReport: { generatedAt: "2026-09-09T00:00:00Z", scannedPages: 2,
      issues: ["中文", "English"].map((name) => ({ id: name, source: "local", severity: "warning",
        issueType: "missing_frontmatter", path: `wiki/${name}.md`, message: "Missing metadata", fixability: "safe", scanHash: "hash" })),
    }, selectedIssueId: "English" });
    const { container } = render(<LintView />);
    const search = screen.getByRole("searchbox", { name: "Search issues or paths" });
    fireEvent.change(search, { target: { value: "中文" } });
    expect(container.querySelectorAll(".issue-card")).toHaveLength(1);
    expect(screen.getByRole("button", { name: "Auto-fix (1)" })).toBeEnabled();
    expect(container.querySelector(".lint-view-layout")).not.toHaveClass("has-selection");
    fireEvent.change(search, { target: { value: "not present" } });
    expect(screen.getByText("No issues match this filter.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Auto-fix (0)" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Clear search" }));
    expect(container.querySelectorAll(".issue-card")).toHaveLength(2);
  });

  it("does not count an ignored rule or an unscanned report as passed", () => {
    useProjectStore.setState({ currentProject: PROJECT } as never);
    useLintStore.setState({ localReport: { generatedAt: "2026-09-09T00:00:00Z", scannedPages: 1, issues: [] },
      ignores: [{ path: "wiki/a.md", rule: "missing_frontmatter", createdAt: "2026-09-09" }] });
    const { container } = render(<LintView />);
    expect(container.querySelector(".lint-summary")?.textContent).toContain("Passed4");
    expect(screen.queryByText("Frontmatter complete")).not.toBeInTheDocument();
  });

  it("distinguishes a clean report from not having run a check", () => {
    useProjectStore.setState({ currentProject: PROJECT } as never);
    useLintStore.setState({ localReport: { generatedAt: "2026-09-09T00:00:00Z", scannedPages: 0, issues: [] } });
    const { container } = render(<LintView />);
    expect(screen.getByText("No issues found in this report.")).toBeInTheDocument();
    expect(container.querySelector(".lint-summary")?.textContent).toContain("Passed0");
    expect(container.querySelector(".lint-passed")).toBeNull();
  });

  it("exposes a resizable lint details splitter", () => {
    useProjectStore.setState({ currentProject: PROJECT } as never);
    render(<LintView />);
    expect(screen.getByRole("separator", { name: "Resize lint issue details" })).toHaveAttribute("aria-valuenow", "320");
  });

  it.each([[-60, 380], [40, 280]])("moves the right-hand details boundary with a pointer delta of %i", (delta, expectedWidth) => {
    useProjectStore.setState({ currentProject: PROJECT } as never);
    useNavigationStore.getState().resetPaneSize("lintDetails");
    const { container } = render(<LintView />);
    const splitter = screen.getByRole("separator", { name: "Resize lint issue details" });
    const pointer = (target: Element | Document, type: string, clientX: number) => {
      const event = new Event(type, { bubbles: true, cancelable: true });
      Object.defineProperties(event, { clientX: { value: clientX }, pointerId: { value: 1 } });
      fireEvent(target, event);
    };
    pointer(splitter, "pointerdown", 600);
    pointer(document, "pointermove", 600 + delta);
    pointer(document, "pointerup", 600 + delta);
    expect(useNavigationStore.getState().paneSizes.lintDetails).toBe(expectedWidth);
    expect(container.querySelector<HTMLElement>(".lint-view-layout")?.style.getPropertyValue("--lint-details-w-current")).toBe(`${expectedWidth}px`);
    fireEvent.doubleClick(splitter);
    expect(splitter).toHaveAttribute("aria-valuenow", "320");
  });

  it("moves the details boundary in the arrow key direction", () => {
    useProjectStore.setState({ currentProject: PROJECT } as never);
    useNavigationStore.getState().resetPaneSize("lintDetails");
    render(<LintView />);
    const splitter = screen.getByRole("separator", { name: "Resize lint issue details" });
    fireEvent.keyDown(splitter, { key: "ArrowLeft" });
    expect(splitter).toHaveAttribute("aria-valuenow", "332");
    fireEvent.keyDown(splitter, { key: "ArrowRight" });
    expect(splitter).toHaveAttribute("aria-valuenow", "320");
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

  it("opens Update Wiki preparation after a requested post-fix recompile", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_version_history_status") return Promise.resolve({ enabled: true, git: { isRepository: true, head: null, branch: "main", hasChanges: true } });
      if (command === "list_lint_ignores") return Promise.resolve({ ignored: [] });
      if (command === "list_lint_history") {
        return Promise.resolve({ version: 1, entries: [] });
      }
      if (command === "apply_lint_fix") {
        return Promise.resolve({
          kind: "applied",
          affectedPaths: ["wiki/a.md"],
          checkpoint: "checkpoint-a",
        });
      }
      if (command === "run_local_lint") {
        return Promise.resolve({
          issues: [],
          generatedAt: "2026-08-02T00:00:00Z",
          scannedPages: 1,
        });
      }
      return Promise.resolve(null);
    });
    useLintStore.setState({
      safetyPrefs: { checkpoint: true, commitAfter: true, recompile: true },
      localReport: {
        issues: [{
          id: "missing_frontmatter:wiki/a.md",
          source: "local",
          severity: "warning",
          issueType: "missing_frontmatter",
          path: "wiki/a.md",
          message: "No frontmatter",
          fixability: "safe",
          scanHash: "hash-a",
        }],
        generatedAt: "2026-08-01T00:00:00Z",
        scannedPages: 1,
      },
    });
    useProjectStore.setState({ currentProject: PROJECT } as never);

    render(<LintView />);
    fireEvent.click(screen.getByRole("button", { name: "Fix" }));

    await waitFor(() => expect(useNavigationStore.getState()).toMatchObject({
      activeView: "workflows",
      workflowLaunchIntent: {
        projectId: PROJECT.projectId,
        projectRootPath: PROJECT.rootPath,
        kind: "update_wiki",
        origin: "lint",
        scopePreset: null,
      },
    }));
    expect(invokeMock).not.toHaveBeenCalledWith(
      "start_workflow",
      expect.anything(),
    );
  });

  it.each([false, true])("enables private history once and resumes batch confirmation (repository=%s)", async (isRepository) => {
    let ready = false;
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_version_history_status") return Promise.resolve({ enabled: ready, git: { isRepository: ready || isRepository, head: null, branch: "main", hasChanges: false } });
      if (command === "prepare_version_action") return Promise.resolve({ id: "enable-1" });
      if (command === "confirm_version_action") { ready = true; return Promise.resolve(); }
      return Promise.resolve({ ignored: [], entries: [] });
    });
    useProjectStore.setState({ currentProject: PROJECT } as never);
    useLintStore.setState({ localReport: {
      issues: [{ id: "missing_frontmatter:wiki/a.md", source: "local", severity: "warning", issueType: "missing_frontmatter", path: "wiki/a.md", message: "No frontmatter", fixability: "safe", scanHash: "hash-a" }],
      generatedAt: "2026-09-09T00:00:00Z", scannedPages: 1,
    } });
    render(<LintView />);
    fireEvent.click(screen.getByRole("button", { name: /Auto-fix \(1\)/i }));
    expect(await screen.findByText(/Save local recovery versions/)).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(invokeMock.mock.calls.some(([command]) => command === "apply_lint_fixes")).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Enable and continue fixing" }));
    expect(await screen.findByRole("dialog", { name: "Auto-fix 1 issues?" })).toBeInTheDocument();
    expect(invokeMock.mock.calls.some(([command]) => command === "apply_lint_fixes")).toBe(false);
  });

  it("keeps a Git process failure actionable without invoking the fix", async () => {
    invokeMock.mockImplementation((command: string) => command === "get_version_history_status"
      ? Promise.reject({ code: "GIT_COMMAND_FAILED", message: "Git is unavailable" })
      : Promise.resolve({ ignored: [], entries: [] }));
    useProjectStore.setState({ currentProject: PROJECT } as never);
    useLintStore.setState({ localReport: {
      issues: [{ id: "missing_frontmatter:wiki/a.md", source: "local", severity: "warning", issueType: "missing_frontmatter", path: "wiki/a.md", message: "No frontmatter", fixability: "safe", scanHash: "hash-a" }],
      generatedAt: "2026-09-09T00:00:00Z", scannedPages: 1,
    } });
    render(<LintView />);
    fireEvent.click(screen.getByRole("button", { name: "Fix" }));
    expect(await screen.findByText(/Git could not prepare a recovery checkpoint/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Check Git again" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "Enable and continue fixing" })).not.toBeInTheDocument();
    expect(invokeMock.mock.calls.some(([command]) => command === "apply_lint_fix")).toBe(false);
  });

  it("shows which ignored files prevented the checkpoint in technical details", async () => {
    useProjectStore.setState({ currentProject: PROJECT } as never);
    useLintStore.setState({ error: "Cannot protect ignored files", errorCode: "GIT_CHECKPOINT_PATH_IGNORED", errorDetails: { paths: ["wiki/未跟踪.md"] } });
    render(<LintView />);
    expect(await screen.findByText(/Some affected files are excluded from Git history/)).toBeInTheDocument();
    fireEvent.click(screen.getByText("Technical details"));
    expect(screen.getByText(/wiki\/未跟踪.md/)).toBeVisible();
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
    expect(screen.getByRole("button", { name: "Deep 1" })).toBeInTheDocument();
    expect(screen.getAllByText("wiki/主题.md")).toHaveLength(1);
    expect(screen.queryByText("index.md consistent")).not.toBeInTheDocument();
    fireEvent.click(screen.getByText("wiki/主题.md").closest("button")!);
    expect(screen.getAllByText("Local").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("Agent").length).toBeGreaterThanOrEqual(1);
  });

  it("selects eligible Agent findings and queues one bounded repair batch", async () => {
    const issue = {
      id: "duplicate_topic:wiki/重复.md",
      source: "agent" as const,
      severity: "warning" as const,
      issueType: "duplicate_topic" as const,
      path: "wiki/重复.md",
      message: "Duplicate topic",
      evidence: "semantic evidence",
      fixability: "none" as const,
    };
    const healthReport = {
      reportId: "health-repair-ui",
      taskId: "health-repair-ui",
      mode: "complete" as const,
      route: { kind: "agent" as const, agent: "codex" as const, model: "gpt-5", routeRevision: "route-ui" },
      persistent: true,
      issues: [issue],
      findingOrigins: { [issue.id]: ["agent" as const] },
      coverage: { scannedPages: 1, sourcePages: 0, wikiPages: 1, notApplicableRules: [] },
      errorCount: 0,
      warningCount: 1,
      infoCount: 0,
      findingsByType: { duplicate_topic: 1 },
      durationMs: 10,
      generatedAt: "2026-08-10T00:00:00Z",
    };
    const preparation = {
      preparationId: "prep-ui",
      preparationRevision: "prep-ui-revision",
      reportId: healthReport.reportId,
      selectionRevision: "selection-ui",
      selectedFindingIds: [issue.id],
      route: healthReport.route,
      skill: { id: "builtin.wiki-lint", version: "2026-08-12.1", sha256: "skill" },
      authorizedPaths: [issue.path],
      authorizedPathHashes: { [issue.path]: "hash-ui" },
      baselineFingerprint: "baseline-ui",
      expectedGitHead: "head-ui",
      pendingAction: {
        id: "action-ui",
        actionType: "agent_auto_fix",
        title: "Repair",
        message: "Repair",
        riskLevel: "high",
        affectedPaths: [issue.path],
        preview: null,
        expiresAt: null,
      },
    };
    const run = { taskId: "run-ui", updatedAt: "2026-08-10T00:01:00Z" };
    invokeMock.mockImplementation((command: string) => {
      if (command === "list_lint_ignores") return Promise.resolve({ ignored: [] });
      if (command === "list_lint_history") return Promise.resolve({ version: 1, entries: [] });
      if (command === "prepare_agent_lint_repair") return Promise.resolve(preparation);
      if (command === "confirm_agent_lint_repair_start") return Promise.resolve({ kind: "created", run });
      return Promise.resolve({ ignored: [] });
    });
    useLintStore.setState({ healthReport });
    useProjectStore.setState({ currentProject: PROJECT, authority: null } as never);

    render(<LintView />);
    const checkbox = screen.getByRole("checkbox", { name: "Select finding in wiki/重复.md for Agent repair" });
    fireEvent.click(checkbox);
    expect(checkbox).toBeChecked();
    fireEvent.click(screen.getByRole("button", { name: "Prepare repair (1)" }));
    expect(await screen.findByText("Confirm one Agent repair batch")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Approve and queue" }));
    await waitFor(() => expect(useNavigationStore.getState().activeView).toBe("workflows"));
    expect(invokeMock).toHaveBeenCalledWith("confirm_agent_lint_repair_start", expect.anything());
  });

  it("keeps management content out of the results and returns focus when closed", async () => {
    useProjectStore.setState({ currentProject: PROJECT } as never);
    const { container } = render(<LintView />);
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
    const trigger = screen.getByRole("button", { name: "Ignored issues" });
    fireEvent.click(trigger);
    expect(screen.getByRole("complementary", { name: "Ignored issues" })).toBeInTheDocument();
    expect(screen.getByText("No ignored issues.")).toBeInTheDocument();
    expect(container.querySelector(".lint-view-layout")).toHaveClass("has-selection");
    fireEvent.keyDown(screen.getByRole("button", { name: "Back to results" }), { key: "Escape" });
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
    expect(container.querySelector(".lint-view-layout")).not.toHaveClass("has-selection");
    await waitFor(() => expect(trigger).toHaveFocus());
  });

  it("returns to the selected report with the previous search cleared", async () => {
    const entry = { id: "selected-report", kind: "local", createdAt: "2026-09-08T00:00:00Z", issueCount: 0, errorCount: 0, warningCount: 0, infoCount: 0 };
    invokeMock.mockImplementation((command: string) => {
      if (command === "list_lint_history") return Promise.resolve({ version: 1, entries: [entry] });
      if (command === "read_lint_history_report") return Promise.resolve({ entry, localReport: { issues: [], generatedAt: entry.createdAt, scannedPages: 2 } });
      return Promise.resolve({ ignored: [] });
    });
    useProjectStore.setState({ currentProject: PROJECT } as never);
    useLintStore.setState({ localReport: { issues: [], generatedAt: "2026-09-09T00:00:00Z", scannedPages: 5 } });
    render(<LintView />);
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "old search" } });
    fireEvent.click(screen.getByRole("button", { name: "Check history" }));
    fireEvent.click(await screen.findByRole("button", { name: /Local lint/ }));
    await waitFor(() => expect(useLintStore.getState().activeHistoryId).toBe(entry.id));
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
    expect(screen.getByRole("searchbox")).toHaveValue("");
    expect(useLintStore.getState().localReport?.scannedPages).toBe(2);
  });

  it("discards a pending history choice when the user switches management panels", async () => {
    const entry = { id: "old-report", kind: "local", createdAt: "2026-09-08T00:00:00Z", issueCount: 0, errorCount: 0, warningCount: 0, infoCount: 0 };
    let finish: ((value: unknown) => void) | undefined;
    invokeMock.mockImplementation((command: string) => {
      if (command === "list_lint_history") return Promise.resolve({ version: 1, entries: [entry] });
      if (command === "read_lint_history_report") return new Promise((resolve) => { finish = resolve; });
      return Promise.resolve({ ignored: [] });
    });
    useProjectStore.setState({ currentProject: PROJECT } as never);
    useLintStore.setState({ localReport: { issues: [], generatedAt: "2026-09-09T00:00:00Z", scannedPages: 5 } });
    render(<LintView />);
    fireEvent.click(screen.getByRole("button", { name: "Check history" }));
    fireEvent.click(await screen.findByRole("button", { name: /Local lint/ }));
    await waitFor(() => expect(finish).toBeDefined());
    fireEvent.click(screen.getByRole("button", { name: "Ignored issues" }));
    await act(async () => { finish?.({ entry, localReport: { issues: [], generatedAt: entry.createdAt, scannedPages: 2 } }); });
    expect(useLintStore.getState().localReport?.scannedPages).toBe(5);
    expect(useLintStore.getState().activeHistoryId).toBeNull();
    expect(screen.getByRole("complementary", { name: "Ignored issues" })).toBeInTheDocument();
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

    fireEvent.click(screen.getByRole("button", { name: "Check history" }));
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

    fireEvent.click(screen.getByRole("button", { name: "Check history" }));
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

    fireEvent.click(screen.getByRole("button", { name: "Check history" }));
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

    fireEvent.click(screen.getByRole("button", { name: "Ignored issues" }));
    const restore = await screen.findByRole("button", { name: "Restore check" });
    fireEvent.click(restore);
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledWith("remove_lint_ignore", expect.anything()));
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledWith("run_local_lint", expect.anything()));
  });

  it("explains why a historical issue needs a rescan and keeps ignore available", () => {
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
    expect(screen.queryByRole("button", { name: "Apply fix" })).not.toBeInTheDocument();
    expect(screen.getByText("Rescan required before applying this fix.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Ignore this issue" })).toBeEnabled();
  });
});
