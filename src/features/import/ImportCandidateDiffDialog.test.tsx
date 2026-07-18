import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import type { AgentCandidateView } from "../../types/importV2Agent";
import { ImportCandidateDiffDialog } from "./ImportCandidateDiffDialog";

const view: AgentCandidateView = {
  projectId: "project-1",
  sessionId: "session-1",
  itemId: "item-1",
  candidate: {
    candidateId: "candidate-1",
    taskId: "task-1",
    auditId: "audit-1",
    trigger: "manual",
    agentKind: "codex",
    agentVersion: "1.0",
    promptTemplateVersion: "prompt-1",
    approvedCostMicros: null,
    toolCalls: [],
    markdown: { kind: "markdown", relativePath: ".app/staging/candidate.md", sha256: "a".repeat(64), sizeBytes: 12 },
    assets: [],
    quality: { level: "pass", metrics: [], warnings: [] },
    processingSummary: "Agent candidate",
    toolsUsed: [],
    uncertainties: [],
    warnings: [],
    sourceSnapshotSha256: "b".repeat(64),
    createdAt: "2026-07-13T00:00:00.000Z",
  },
  diff: {
    candidateId: "candidate-1",
    baselineMarkdown: "# Deterministic",
    currentMarkdown: "# Current Wiki",
    agentMarkdown: "# Agent",
    unifiedDiff: "@@ -1 +1 @@\n-# Current Wiki\n+# Agent",
    needsThreeWayMerge: true,
  },
};

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("ImportCandidateDiffDialog", () => {
  it("renders baseline/current/candidate evidence and emits review intent without writing", () => {
    const onAction = vi.fn();
    render(<ImportCandidateDiffDialog open view={view} onClose={vi.fn()} onAction={onAction} />);

    expect(screen.getByText("# Deterministic")).toBeInTheDocument();
    expect(screen.getByText("# Current Wiki")).toBeInTheDocument();
    expect(screen.getAllByText("# Agent").length).toBeGreaterThan(0);
    fireEvent.click(screen.getByRole("button", { name: /apply merged candidate/i }));
    expect(onAction).toHaveBeenCalledWith({ kind: "apply_merged", candidateId: "candidate-1", mergedMarkdown: "# Agent" });
  });

  it("lets the user edit the merged buffer before applying it", () => {
    const onAction = vi.fn();
    render(<ImportCandidateDiffDialog open view={view} onClose={vi.fn()} onAction={onAction} />);

    const editor = screen.getByRole("textbox", { name: /merged markdown/i });
    fireEvent.change(editor, { target: { value: "# Human merged" } });
    fireEvent.click(screen.getByRole("button", { name: /apply merged candidate/i }));
    expect(onAction).toHaveBeenCalledWith({ kind: "apply_merged", candidateId: "candidate-1", mergedMarkdown: "# Human merged" });
  });

  it("supports deterministic, agent, keep-current, create-new, and discard intents", () => {
    const onAction = vi.fn();
    render(<ImportCandidateDiffDialog open view={view} onClose={vi.fn()} onAction={onAction} />);

    fireEvent.click(screen.getByRole("button", { name: /choose deterministic/i }));
    fireEvent.click(screen.getByRole("button", { name: /choose agent candidate/i }));
    fireEvent.click(screen.getByRole("button", { name: /keep current wiki/i }));
    fireEvent.click(screen.getByRole("button", { name: /create new document/i }));
    fireEvent.click(screen.getByRole("button", { name: /discard agent candidate/i }));
    expect(onAction.mock.calls.map(([action]) => action.kind)).toEqual([
      "choose_deterministic",
      "choose_agent",
      "keep_current",
      "create_new",
      "discard",
    ]);
  });
});
