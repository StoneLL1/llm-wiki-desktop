import { describe, expect, it } from "vitest";

import capability from "../../../src-tauri/capabilities/main.json";
import config from "../../../src-tauri/tauri.conf.json";

describe("native file import configuration", () => {
  it("explicitly enables native drag-drop on the main window", () => {
    const typedConfig = config as {
      app?: { windows?: Array<{ label?: string; dragDropEnabled?: boolean }> };
    };
    const mainWindow = typedConfig.app?.windows?.find((window) => window.label === "main");

    expect(mainWindow?.dragDropEnabled).toBe(true);
  });

  it("grants the main window permission to listen for drag-drop events", () => {
    const typedCapability = capability as {
      windows?: string[];
      permissions?: string[];
    };

    expect(typedCapability.windows).toContain("main");
    expect(typedCapability.permissions).toContain("core:event:allow-listen");
    expect(typedCapability.permissions).toContain("core:event:allow-unlisten");
    expect(typedCapability.permissions).toContain("dialog:allow-open");
  });
});
