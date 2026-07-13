import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import {
  activateImportV2,
  getImportBackendActivation,
} from "./importV2ActivationApi";

describe("Import V2 activation API", () => {
  beforeEach(() => invoke.mockReset());

  it("uses the explicit activation confirmation envelope", async () => {
    const request = {
      projectId: "project-1",
      projectRootPath: "D:/Wiki/中文项目",
      report: {} as never,
      readiness: {} as never,
      releaseVersion: "1.0.0",
      confirmation: {
        reportFingerprint: "report",
        token: "token",
        acknowledgeNoGitRollback: true,
      },
    };
    invoke.mockResolvedValueOnce({ record: { activeBackend: "v2" } });
    await activateImportV2(request);
    expect(invoke).toHaveBeenLastCalledWith("activate_import_v2", { request });
  });

  it("reads activation state without exposing a mutation switch", async () => {
    const request = { projectId: "project-1", projectRootPath: "D:/Wiki/中文项目" };
    invoke.mockResolvedValueOnce(null);
    await getImportBackendActivation(request);
    expect(invoke).toHaveBeenLastCalledWith("get_import_backend_activation", { request });
  });
});
