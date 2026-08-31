import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import {
  cancelAppCapabilityInstall,
  installAppCapability,
  listAppCapabilities,
  resumeAppCapabilityInstall,
} from "./appCapabilityApi";

beforeEach(() => invokeMock.mockReset());

describe("appCapabilityApi", () => {
  it("uses app-global versioned commands without project context", async () => {
    invokeMock.mockResolvedValue([]);
    await listAppCapabilities();
    expect(invokeMock).toHaveBeenCalledWith("list_app_capabilities_v1");

    const install = {
      capabilityId: "browser-runtime",
      expectedVersion: "1.4.0",
      acknowledgementVersion: "ack-v1",
    };
    await installAppCapability(install);
    expect(invokeMock).toHaveBeenCalledWith("install_app_capability_v1", { request: install });

    const control = { taskId: "task-a", taskRevision: "revision-a", scope: "app_global" as const };
    await resumeAppCapabilityInstall(control);
    await cancelAppCapabilityInstall(control);
    expect(invokeMock).toHaveBeenCalledWith("resume_app_capability_install_v1", { request: control });
    expect(invokeMock).toHaveBeenCalledWith("cancel_app_capability_install_v1", { request: control });
  });
});
