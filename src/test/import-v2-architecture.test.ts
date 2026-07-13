import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const forbiddenLegacyCommands = [
  "preview_import",
  "fetch_import_url",
  "preview_text_import",
  "confirm_import_preview",
] as const;

describe("Import V2 frontend architecture", () => {
  it("keeps legacy mutation command names out of active import source code", () => {
    const featureRoot = join(process.cwd(), "src", "features", "import");
    const source = readdirSync(featureRoot)
      .filter((name) => /\.(ts|tsx)$/.test(name) && !name.endsWith(".test.ts") && !name.endsWith(".test.tsx"))
      .map((name) => readFileSync(join(featureRoot, name), "utf8"))
      .join("\n");

    for (const command of forbiddenLegacyCommands) expect(source).not.toContain(command);
  });
});
