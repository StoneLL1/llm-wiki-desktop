import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const shell = readFileSync(
  resolve(process.cwd(), "src/components/app/AppShell.tsx"),
  "utf8",
);

describe("AppShell architecture", () => {
  it("keeps feature orchestration outside the layout shell", () => {
    expect(shell).not.toContain("@tauri-apps/api/core");
    expect(shell).not.toContain("useImportStore");
    expect(shell).not.toContain("waitForTaskTerminal");
    expect(shell).not.toContain("start_wiki_compile");
    expect(shell).not.toContain("save_llm_provider");
    expect(shell).not.toContain("start_deep_lint");
    expect(shell).not.toContain("start_export");
  });
});
