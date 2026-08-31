import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const shell = readFileSync(
  resolve(process.cwd(), "src/components/app/AppShell.tsx"),
  "utf8",
);
const workspaceController = readFileSync(
  resolve(process.cwd(), "src/components/app/WorkspaceController.tsx"),
  "utf8",
);

const importV2EntryPoints = [
  "ImportView.tsx",
  "ImportV2Dialogs.tsx",
  "ImportSourceMethods.tsx",
  "ImportQueue.tsx",
  "useImportWorkflow.ts",
].map((file) => readFileSync(resolve(process.cwd(), "src/features/import", file), "utf8")).join("\n");

describe("AppShell architecture", () => {
  it("keeps feature orchestration outside the layout shell", () => {
    expect(shell).not.toContain("@tauri-apps/api/core");
    expect(shell).not.toContain("useImportStore");
    expect(shell).not.toContain("waitForTaskTerminal");
    expect(shell).not.toContain("start_wiki_compile");
    expect(shell).not.toContain("save_llm_provider");
    expect(shell).not.toContain("start_deep_lint");
    expect(shell).not.toContain("start_export");
    expect(shell).toContain("<UpdateController />");
    expect(shell).toContain("<AppCapabilityController />");
    expect(shell).not.toContain("useUpdateStore");
    expect(shell).not.toContain("useAppCapabilityStore");
    expect(workspaceController).not.toContain("AppCapabilityController");
  });

  it("keeps the active Import V2 surface off legacy mutation commands and direct IPC", () => {
    expect(importV2EntryPoints).not.toContain("preview_import");
    expect(importV2EntryPoints).not.toContain("preview_text_import");
    expect(importV2EntryPoints).not.toContain("fetch_import_url");
    expect(importV2EntryPoints).not.toContain("confirm_import_preview");
    expect(importV2EntryPoints).not.toContain("@tauri-apps/api/core");
    expect(importV2EntryPoints).not.toMatch(/\binvoke\s*\(/);
  });
});
