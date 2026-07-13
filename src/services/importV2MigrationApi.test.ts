import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import {
  applyImportV2Migration,
  getImportV2MigrationStatus,
  planImportV2Migration,
  resumeImportV2Migration,
  scanImportV2Migration,
} from "./importV2MigrationApi";

describe("Import V2 migration API", () => {
  beforeEach(() => invoke.mockReset());

  it("uses stable command names and request envelopes", async () => {
    const request = { projectId: "project-1", projectRootPath: "D:/Wiki/项目" };
    invoke.mockResolvedValueOnce({ schemaVersion: 1 });
    await scanImportV2Migration(request);
    expect(invoke).toHaveBeenLastCalledWith("scan_import_v2_migration", { request });

    invoke.mockResolvedValueOnce({ planVersion: 1 });
    const planRequest = { ...request, inventory: { schemaVersion: 1 } } as never;
    await planImportV2Migration(planRequest);
    expect(invoke).toHaveBeenLastCalledWith("plan_import_v2_migration", { request: planRequest });
  });

  it("keeps confirmation only on apply and exposes resumable task results", async () => {
    const request = {
      projectId: "project-1",
      projectRootPath: "D:/Wiki/项目",
      plan: {} as never,
      confirmation: { planFingerprint: "plan", token: "token", acknowledgeNoGitRollback: true },
    };
    invoke.mockResolvedValueOnce({ id: "task-1" });
    await applyImportV2Migration(request);
    expect(invoke).toHaveBeenLastCalledWith("apply_import_v2_migration", { request });

    invoke.mockResolvedValueOnce({ status: "dry_run_ready" });
    await getImportV2MigrationStatus({ projectId: "project-1", projectRootPath: "D:/Wiki/项目" });
    expect(invoke).toHaveBeenLastCalledWith("get_import_v2_migration_status", {
      request: { projectId: "project-1", projectRootPath: "D:/Wiki/项目" },
    });

    invoke.mockResolvedValueOnce({ id: "task-2" });
    await resumeImportV2Migration(request);
    expect(invoke).toHaveBeenLastCalledWith("resume_import_v2_migration", { request });
  });
});
