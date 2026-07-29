import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const forbiddenLegacyCommands = [
  "preview_import",
  "get_import_preview",
  "fetch_import_url",
  "preview_text_import",
  "confirm_import_preview",
  "extract_text_preview",
  "validate_import_url",
] as const;

const retiredSourceSurfaceTokens = [
  "list_imported_sources",
  "request_delete_source",
  "request_replace_source",
  "apply_source_delete",
  "apply_source_replace",
  "ImportedSource",
] as const;

const containsIdentifier = (source: string, identifier: string) =>
  new RegExp(
    `(^|[^A-Za-z0-9_])${identifier.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}([^A-Za-z0-9_]|$)`,
  ).test(source);

describe("Import V2 frontend architecture", () => {
  it("keeps legacy mutation command names out of active import source code", () => {
    const featureRoot = join(process.cwd(), "src", "features", "import");
    const source = readdirSync(featureRoot)
      .filter((name) => /\.(ts|tsx)$/.test(name) && !name.endsWith(".test.ts") && !name.endsWith(".test.tsx"))
      .map((name) => readFileSync(join(featureRoot, name), "utf8"))
      .join("\n");

    for (const command of forbiddenLegacyCommands) expect(source).not.toContain(command);
  });

  it("removes retired legacy Import and Source commands from every callable surface", () => {
    const surfaceFiles = [
      "src-tauri/src/lib.rs",
      "src-tauri/src/commands/mod.rs",
      "src-tauri/src/commands/file_commands.rs",
      "src-tauri/src/app_state.rs",
      "src-tauri/src/services/mod.rs",
      "src-tauri/src/models/confirmation.rs",
      "src/types/backend.ts",
      "src/services/importV2Api.ts",
      "src/components/app/ProjectConfirmationController.tsx",
    ];
    const surfaces = surfaceFiles
      .map((file) => `${file}\n${readFileSync(join(process.cwd(), file), "utf8")}`)
      .join("\n");

    for (const token of forbiddenLegacyCommands) {
      expect(containsIdentifier(surfaces, token), token).toBe(false);
    }
    for (const token of retiredSourceSurfaceTokens) {
      expect(containsIdentifier(surfaces, token), token).toBe(false);
    }
    for (const token of ["ImportService", "ExtractionService", "create_import_checkpoint"]) {
      expect(containsIdentifier(surfaces, token), token).toBe(false);
    }
    for (const file of [
      "src-tauri/src/commands/import_commands.rs",
      "src-tauri/src/models/import.rs",
      "src-tauri/src/services/extraction_service.rs",
      "src-tauri/src/services/import_service/mod.rs",
      "src-tauri/src/services/import_service/source_actions.rs",
      "src-tauri/src/services/import_service/source_catalog.rs",
      "src-tauri/src/services/import_v2/legacy_route.rs",
      "src/types/import.ts",
    ]) {
      expect(existsSync(join(process.cwd(), file)), file).toBe(false);
    }
  });
});
