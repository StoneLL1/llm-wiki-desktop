import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("react-i18next", () => ({
  initReactI18next: { type: "3rdParty", init: vi.fn() },
  useTranslation: () => ({
    t: (key: string) => ({
      "settings.compatibility.review": "Review compatibility",
      "importV2.migration.scan": "Check older data",
      "importV2.migration.plan": "Preview changes",
      "importV2.migration.confirmFingerprint": "I reviewed this exact change preview.",
      "importV2.migration.noGitAck": "I understand this project has no saved restore point.",
      "importV2.migration.apply": "Update compatibility data",
      "importV2.migration.resume": "Continue update",
      "importV2.migration.close": "Close",
    } as Record<string, string>)[key] ?? key,
  }),
}));

import type { ImportWorkflow } from "../import/importWorkflow";
import { useTaskStore } from "../../stores/taskStore";
import type {
  LegacyInventory,
  MigrationConfirmation,
  MigrationPlan,
  MigrationReport,
} from "../../types/importV2Migration";
import { ImportCompatibilitySettings } from "./ImportCompatibilitySettings";
import type { BackendTask } from "../../types/task";

const inventory: LegacyInventory = {
  schemaVersion: 1,
  projectIdentity: "project-identity",
  fingerprint: "inventory-fingerprint",
  records: [],
  warnings: [],
  scannedFiles: [],
};
const plan: MigrationPlan = {
  planVersion: 1,
  v2IndexFingerprint: "MISSING",
  inventoryFingerprint: inventory.fingerprint,
  candidates: [],
  summary: {
    total: 0,
    automaticLinks: 0,
    proposedRecords: 0,
    conflicts: 0,
    legacyUnmanaged: 0,
    warnings: 0,
  },
};
const confirmation: MigrationConfirmation = {
  planFingerprint: "plan-fingerprint",
  token: "backend-issued-token",
  acknowledgeNoGitRollback: false,
};
const report: MigrationReport = {
  reportVersion: 1,
  planVersion: 1,
  planFingerprint: confirmation.planFingerprint,
  inventoryFingerprint: inventory.fingerprint,
  status: "dry_run_ready",
  summary: plan.summary,
  automaticLinks: [],
  proposedRecords: [],
  conflicts: [],
  legacyUnmanaged: [],
  warnings: [],
  affectedMetadataPaths: [".app/source-index-v2.json"],
  untouchedContentPaths: ["raw/", "wiki/"],
  rollbackStatement: "Restore the saved Git checkpoint.",
  requiredConfirmation: true,
};

function workflow(overrides: Partial<ImportWorkflow> = {}): ImportWorkflow {
  return {
    projectKey: "project-a\u0000D:/Wiki/项目",
    readiness: null,
    readinessWarning: null,
    getMigrationStatus: vi.fn().mockResolvedValue({ status: "not_scanned", report: null }),
    scanMigration: vi.fn().mockResolvedValue(inventory),
    planMigration: vi.fn().mockResolvedValue({ plan, report, confirmation }),
    applyMigration: vi.fn().mockResolvedValue(null),
    resumeMigration: vi.fn().mockResolvedValue(null),
    ...overrides,
  } as ImportWorkflow;
}

function migrationTask(status: BackendTask["status"]): BackendTask {
  return {
    id: "migration-task",
    taskType: "import",
    projectId: "project-a",
    title: "Update compatibility data",
    status,
    progress: null,
    startedAt: "2026-07-25T00:00:00Z",
    updatedAt: status === "queued" ? "2026-07-25T00:00:00Z" : "2026-07-25T00:01:00Z",
    completedAt: status === "queued" ? null : "2026-07-25T00:01:00Z",
    cancellable: true,
    logPath: null,
    result: null,
    error: status === "failed" ? {
      code: "MIGRATION_FAILED",
      message: "fixture failure",
      details: null,
      recoverable: true,
      userActionRequired: true,
    } : null,
  };
}

beforeEach(() => {
  useTaskStore.setState({ tasks: [] });
});

describe("Import compatibility settings", () => {
  it("keeps scan busy until completion and applies a backend-bound preparation", async () => {
    const scanControl: { finish?: (value: LegacyInventory) => void } = {};
    const scanMigration = vi.fn(() => new Promise<LegacyInventory>((resolve) => {
      scanControl.finish = resolve;
    }));
    const current = workflow({ scanMigration });
    render(<ImportCompatibilitySettings workflow={current} />);

    fireEvent.click(screen.getByRole("button", { name: "Review compatibility" }));
    const scanButton = await screen.findByRole("button", { name: "Check older data" });
    fireEvent.click(scanButton);
    await waitFor(() => expect(
      screen.queryByRole("button", { name: "Check older data" }),
    ).not.toBeInTheDocument());
    expect(scanMigration).toHaveBeenCalledTimes(1);

    scanControl.finish?.(inventory);
    const planButton = await screen.findByRole("button", { name: "Preview changes" });
    fireEvent.click(planButton);

    const fingerprint = await screen.findByRole("checkbox", {
      name: "I reviewed this exact change preview.",
    });
    const noGit = screen.getByRole("checkbox", {
      name: "I understand this project has no saved restore point.",
    });
    fireEvent.click(fingerprint);
    fireEvent.click(noGit);
    fireEvent.click(screen.getByRole("button", { name: "Update compatibility data" }));

    await waitFor(() => expect(current.applyMigration).toHaveBeenCalledWith(
      plan,
      { ...confirmation, acknowledgeNoGitRollback: true },
    ));
  });

  it("consumes a rejected scan after the workflow reports it", async () => {
    const current = workflow({
      scanMigration: vi.fn().mockRejectedValue(new Error("scan failed")),
    });
    render(<ImportCompatibilitySettings workflow={current} />);

    fireEvent.click(screen.getByRole("button", { name: "Review compatibility" }));
    const scanButton = await screen.findByRole("button", { name: "Check older data" });
    fireEvent.click(scanButton);

    await waitFor(() => expect(scanButton).not.toBeDisabled());
    expect(current.scanMigration).toHaveBeenCalledTimes(1);
  });

  it("offers resume with the same backend-bound plan after a background task fails", async () => {
    const current = workflow({
      applyMigration: vi.fn().mockResolvedValue(migrationTask("queued")),
      resumeMigration: vi.fn().mockResolvedValue(null),
    });
    render(<ImportCompatibilitySettings workflow={current} />);

    fireEvent.click(screen.getByRole("button", { name: "Review compatibility" }));
    fireEvent.click(await screen.findByRole("button", { name: "Check older data" }));
    fireEvent.click(await screen.findByRole("button", { name: "Preview changes" }));
    fireEvent.click(await screen.findByRole("checkbox", {
      name: "I reviewed this exact change preview.",
    }));
    fireEvent.click(screen.getByRole("checkbox", {
      name: "I understand this project has no saved restore point.",
    }));
    fireEvent.click(screen.getByRole("button", { name: "Update compatibility data" }));
    await waitFor(() => expect(current.applyMigration).toHaveBeenCalled());

    useTaskStore.getState().upsertTask(migrationTask("failed"));
    fireEvent.click(await screen.findByRole("button", { name: "Continue update" }));

    await waitFor(() => expect(current.resumeMigration).toHaveBeenCalledWith(
      plan,
      { ...confirmation, acknowledgeNoGitRollback: true },
    ));
  });
});
