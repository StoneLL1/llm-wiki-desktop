import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import type { MigrationConfirmation, MigrationPlan, MigrationReport } from "../../types/importV2Migration";
import { ImportMigrationDialog } from "./ImportMigrationDialog";

const plan: MigrationPlan = {
  planVersion: 2,
  v2IndexFingerprint: "index-fingerprint",
  inventoryFingerprint: "inventory-fingerprint",
  candidates: [],
  summary: { total: 4, automaticLinks: 1, proposedRecords: 1, conflicts: 1, legacyUnmanaged: 1, warnings: 1 },
};
const report: MigrationReport = {
  reportVersion: 1,
  planVersion: 2,
  planFingerprint: "plan-fingerprint",
  inventoryFingerprint: "inventory-fingerprint",
  status: "dry_run_ready",
  summary: plan.summary,
  automaticLinks: [],
  proposedRecords: [],
  conflicts: [],
  legacyUnmanaged: [],
  warnings: [{ code: "LEGACY_WARNING", message: "Review this record", relativePath: ".app/legacy.json", redacted: true }],
  affectedMetadataPaths: [".app/source-index-v2.json", ".app/import-v2-migration/report.json"],
  untouchedContentPaths: ["raw/", "wiki/", ".app/source-index.json"],
  rollbackStatement: "Rollback uses the previous application release and preserved V1 metadata.",
  requiredConfirmation: true,
};
const confirmation: MigrationConfirmation = { planFingerprint: "plan-fingerprint", token: "opaque-confirmation-token", acknowledgeNoGitRollback: false };

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("ImportMigrationDialog", () => {
  it("shows dry-run evidence, untouched content, checkpoint and rollback facts", () => {
    render(
      <ImportMigrationDialog
        open
        status="dry_run_ready"
        plan={plan}
        report={report}
        confirmation={confirmation}
        checkpoint={{ created: true, commitHash: "abc123", message: "Migration checkpoint", purpose: "high_risk_operation", affectedPaths: [".app/source-index-v2.json"] }}
        onScan={vi.fn()}
        onPlan={vi.fn()}
        onApply={vi.fn()}
        onResume={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText(/automatic links/i)).toBeInTheDocument();
    expect(screen.getByText(/proposed records/i)).toBeInTheDocument();
    expect(screen.getByText(/conflicts/i)).toBeInTheDocument();
    expect(screen.getByText(/raw\//i)).toBeInTheDocument();
    expect(screen.getByText(/Git checkpoint/i)).toBeInTheDocument();
    expect(screen.getByText(/Rollback uses/i)).toBeInTheDocument();
  });

  it("keeps Apply disabled until the report fingerprint is explicitly confirmed", async () => {
    const onApply = vi.fn().mockResolvedValue(undefined);
    render(
      <ImportMigrationDialog
        open
        status="awaiting_confirmation"
        plan={plan}
        report={report}
        confirmation={confirmation}
        checkpoint={null}
        onScan={vi.fn()}
        onPlan={vi.fn()}
        onApply={onApply}
        onResume={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    const apply = screen.getByRole("button", { name: /apply migration/i });
    expect(apply).toBeDisabled();
    fireEvent.click(screen.getByRole("checkbox", { name: /confirm.*report fingerprint/i }));
    fireEvent.click(screen.getByRole("checkbox", { name: /no Git rollback/i }));
    fireEvent.click(apply);
    await waitFor(() => expect(onApply).toHaveBeenCalledWith(plan, { ...confirmation, acknowledgeNoGitRollback: true }));
  });

  it("offers resume for an interrupted migration without cancelling on close", () => {
    const onResume = vi.fn();
    const onClose = vi.fn();
    render(
      <ImportMigrationDialog
        open
        status="applying"
        resumable
        plan={plan}
        report={report}
        confirmation={confirmation}
        checkpoint={null}
        onScan={vi.fn()}
        onPlan={vi.fn()}
        onApply={vi.fn()}
        onResume={onResume}
        onClose={onClose}
      />,
    );

    fireEvent.click(screen.getByRole("checkbox", { name: /confirm.*report fingerprint/i }));
    fireEvent.click(screen.getByRole("checkbox", { name: /no Git rollback/i }));
    fireEvent.click(screen.getByRole("button", { name: /resume migration/i }));
    fireEvent.click(screen.getAllByRole("button", { name: /close/i })[1]);
    expect(onResume).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });
});
