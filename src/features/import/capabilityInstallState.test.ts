import { describe, expect, it } from "vitest";

import type { BackendTask } from "../../types/task";
import { capabilityInstallState } from "./capabilityInstallState";

function task(overrides: Partial<BackendTask>): BackendTask {
  return {
    id: "capability-task",
    taskType: "import",
    projectId: "project-1",
    title: "Install browser-runtime",
    status: "running",
    progress: { current: 25, total: 100, label: "capability.downloading" },
    startedAt: "2026-08-20T00:00:00Z",
    updatedAt: "2026-08-20T00:00:00Z",
    completedAt: null,
    cancellable: true,
    logPath: null,
    result: null,
    error: null,
    ...overrides,
  };
}

describe("capabilityInstallState", () => {
  it("maps durable task phases and progress instead of dialog-local loading", () => {
    expect(capabilityInstallState(task({}), true, false)).toMatchObject({
      kind: "downloading",
      downloadedBytes: 25,
      totalBytes: 100,
    });
    expect(capabilityInstallState(task({ progress: { current: 100, total: 100, label: "capability.verifying" } }), true, false).kind).toBe("verifying");
    expect(capabilityInstallState(task({ progress: { current: 100, total: 100, label: "capability.installing" } }), true, false).kind).toBe("installing");
  });

  it("distinguishes interrupted resume, failed health, unavailable release, and installed", () => {
    expect(capabilityInstallState(task({ status: "interrupted" }), true, false).kind).toBe("paused");
    expect(capabilityInstallState(task({ status: "interrupted" }), true, true).kind).toBe("paused");
    expect(capabilityInstallState(task({ status: "cancelled" }), true, false).kind).toBe("not_installed");
    expect(capabilityInstallState(task({ status: "failed", error: { code: "IMPORT_V2_CAPABILITY_HEALTH_CHECK_FAILED", message: "failed", details: null, recoverable: true, userActionRequired: false } }), true, false).kind).toBe("health_check_failed");
    expect(capabilityInstallState(null, false, false).kind).toBe("signed_release_unavailable");
    expect(capabilityInstallState(null, false, false, "catalog_unavailable").kind).toBe("catalog_unavailable");
    expect(capabilityInstallState(null, false, true).kind).toBe("installed");
    expect(capabilityInstallState(task({ status: "cancelled" }), false, false).kind).toBe("signed_release_unavailable");
    expect(capabilityInstallState(task({ status: "succeeded" }), true, false).kind).toBe("installed");
  });
});
