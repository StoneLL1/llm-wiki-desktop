import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import "../../i18n";
import { isAgentLintRepairEligible, type HealthCheckReport, type LintIssue } from "../../types/lint";
import { HealthCheckReportSummary } from "./HealthCheckReportSummary";

const issue: LintIssue = { id: "duplicate", source: "agent", severity: "warning", issueType: "duplicate_topic", path: "wiki/主题.md", message: "Duplicate", fixability: "none" };
const report: HealthCheckReport = {
  reportId: "report", taskId: "task", mode: "complete", route: { kind: "agent", agent: "codex", routeRevision: "1", model: null },
  persistent: true, issues: [issue], findingOrigins: { duplicate: ["agent"] },
  coverage: { scannedPages: 3, sourcePages: 1, wikiPages: 2, deepCoveredPages: 2, deepTruncated: true, notApplicableRules: [] },
  errorCount: 0, warningCount: 1, infoCount: 0, findingsByType: { duplicate_topic: 1 }, durationMs: 100, generatedAt: "2026-09-07T00:00:00Z",
  execution: { inputFingerprint: "input", scannedAt: "2026-09-06T23:59:00Z", freshness: "current", deepStatus: "completed" },
};

describe("HealthCheckReportSummary", () => {
  it("keeps legacy reports readable with unknown freshness", () => {
    render(<HealthCheckReportSummary report={{ ...report, execution: undefined }} />);
    expect(screen.getByText("Freshness unknown")).toBeInTheDocument();
    expect(screen.getByText(/Deep completion unknown/)).toBeInTheDocument();
    expect(screen.getByText(/Saved in the project/)).toBeInTheDocument();
  });

  it("separates stale content, incomplete deep coverage and session lifetime", () => {
    render(<HealthCheckReportSummary report={{ ...report, persistent: false, execution: { ...report.execution!, freshness: "stale", deepStatus: "failed" } }} />);
    expect(screen.getByText("Content has changed")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Local results remain available");
    expect(screen.getByText(/Locally checked 3 pages: 1 Source · 2 Wiki/)).toBeInTheDocument();
    expect(screen.getByText(/did not cover the full text/)).toBeInTheDocument();
    expect(screen.getByText(/Available only in this app session/)).toBeInTheDocument();
  });

  it.each(["pending", "failed", "not_requested"] as const)("does not authorize repair for %s deep coverage", (deepStatus) => {
    expect(isAgentLintRepairEligible(issue, { ...report, execution: { ...report.execution!, deepStatus } })).toBe(false);
  });

  it.each(["stale", "unknown"] as const)("does not authorize repair for %s content", (freshness) => {
    expect(isAgentLintRepairEligible(issue, { ...report, execution: { ...report.execution!, freshness } })).toBe(false);
  });

  it("keeps successful Agent repair eligibility and legacy report compatibility", () => {
    expect(isAgentLintRepairEligible(issue, report)).toBe(true);
    expect(isAgentLintRepairEligible(issue, { ...report, execution: undefined })).toBe(true);
  });
});
