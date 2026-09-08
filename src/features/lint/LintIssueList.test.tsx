import { createRef } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import "../../i18n";
import type { LintIssue } from "../../types/lint";
import { LintIssueList } from "./LintIssueList";

function findings(count: number): LintIssue[] {
  return Array.from({ length: count }, (_, index) => ({
    id: `finding-${index}`,
    path: `wiki/中文长路径/English folder/第${index}篇材料.md`,
    source: "local",
    severity: "warning",
    issueType: "missing_frontmatter",
    message: "Missing frontmatter",
    fixability: "safe",
    scanHash: "current-hash",
  }));
}

const callbacks = () => ({ onSelect: vi.fn(), onApplyFix: vi.fn() });

describe("LintIssueList bounded rendering", () => {
  it("mounts at most 100 findings from a ten-thousand-finding report", () => {
    const { container } = render(
      <LintIssueList issues={findings(10_000)} selectedIssueId={null} {...callbacks()} />,
    );
    expect(container.querySelectorAll(".issue-card")).toHaveLength(100);
    expect(screen.getByRole("status")).toHaveTextContent("1–100 / 10000");
    expect(screen.getByText("Warnings · Local · 10000")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Previous" })).toBeDisabled();
    expect(screen.queryByText(/第100篇材料/)).not.toBeInTheDocument();
  });

  it("keeps stable severity groups, full group totals and a partial last page", () => {
    const issues = findings(103).map((issue, index) => ({
      ...issue,
      severity: index < 2 ? "info" as const : "error" as const,
    }));
    const { container } = render(<LintIssueList issues={issues} selectedIssueId={null} {...callbacks()} />);
    expect(container.querySelector(".issue-card__sub")).toHaveTextContent("第2篇材料");
    expect(screen.getByText("Errors · Local · 101")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    expect(container.querySelectorAll(".issue-card")).toHaveLength(3);
    expect(container.querySelector(".issue-card__sub")).toHaveTextContent("第102篇材料");
    expect(screen.getByText("Errors · Local · 101")).toBeInTheDocument();
    expect(screen.getByText("Info · Local · 2")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("101–103 / 103");
    expect(screen.getByRole("button", { name: "Next" })).toBeDisabled();
  });

  it("preserves external selection, repair checkboxes and focus while paging and fixing", () => {
    const issues = findings(201);
    const handlers = callbacks();
    const onToggleRepairSelection = vi.fn();
    const scrollRef = createRef<HTMLDivElement>();
    const props = {
      issues,
      selectedIssueId: issues[0]!.id,
      repairEligibleIds: new Set([issues[0]!.id, issues[100]!.id]),
      repairSelection: new Set([issues[0]!.id]),
      onToggleRepairSelection,
      scrollRef,
      ...handlers,
    };
    const { container, rerender } = render(<LintIssueList {...props} />);
    expect(screen.getByRole("checkbox")).toBeChecked();
    scrollRef.current!.scrollTop = 500;
    const next = screen.getByRole("button", { name: "Next" });
    next.focus();
    fireEvent.click(next);
    expect(next).toHaveFocus();
    expect(scrollRef.current).toBe(screen.getByTestId("lint-issue-list-scroll"));
    expect(scrollRef.current!.scrollTop).toBe(0);
    expect(screen.getByRole("checkbox")).not.toBeChecked();
    fireEvent.click(screen.getByRole("checkbox"));
    expect(onToggleRepairSelection).toHaveBeenCalledWith(issues[100]!.id, true);
    fireEvent.click(container.querySelector(".issue-card")!);
    expect(handlers.onSelect).toHaveBeenLastCalledWith(issues[100]!.id);
    fireEvent.click(screen.getAllByRole("button", { name: "Fix" })[0]!);
    expect(handlers.onApplyFix).toHaveBeenCalledWith(issues[100]);
    // Selecting a finding must not reset the user's current page.
    rerender(<LintIssueList {...props} selectedIssueId={issues[100]!.id} />);
    expect(screen.getByRole("status")).toHaveTextContent("101–200 / 201");
    expect(container.querySelector(".issue-card")).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(screen.getByRole("button", { name: "Previous" }));
    expect(screen.getByRole("checkbox")).toBeChecked();
  });

  it("resets to a populated first page when the report or filter changes", () => {
    const issues = findings(201);
    const props = { issues, selectedIssueId: null, ...callbacks() };
    const { container, rerender } = render(<LintIssueList {...props} />);
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    rerender(<LintIssueList {...props} issues={issues.slice(0, 101)} />);
    expect(screen.getByRole("status")).toHaveTextContent("1–100 / 101");
    expect(container.querySelectorAll(".issue-card")).toHaveLength(100);
    rerender(<LintIssueList {...props} issues={issues.slice(0, 2)} />);
    expect(container.querySelectorAll(".issue-card")).toHaveLength(2);
    expect(screen.queryByRole("button", { name: "Next" })).not.toBeInTheDocument();
    rerender(<LintIssueList {...props} issues={[]} />);
    expect(container.querySelectorAll(".issue-card")).toHaveLength(0);
  });
});
