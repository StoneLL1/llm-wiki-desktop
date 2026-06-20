import { describe, expect, it } from "vitest";

import {
  CLIPBOARD_MARKDOWN,
  FORMAT_FIXTURES,
  assertEveryFormatCovered,
  fixtureExtractResults,
  fixtureImportPreview,
} from "./project-fixtures";

describe("project-fixtures (Task 14 Step 1)", () => {
  it("covers every SourceFileType so parser gaps stay explicit", () => {
    expect(() => assertEveryFormatCovered()).not.toThrow();
  });

  it("marks every format without a real MVP parser as an explicit partial result", () => {
    for (const fixture of FORMAT_FIXTURES) {
      if (fixture.parserGap) {
        expect(
          fixture.status === "unsupported" || fixture.status === "failed",
          `${fixture.label}: parser-gap formats must surface as unsupported/failed, got ${fixture.status}`,
        ).toBe(true);
      } else {
        expect(fixture.status).toBe("extracted");
      }
    }
  });

  it("builds extract results aligned with the fixture matrix", () => {
    const results = fixtureExtractResults();
    expect(results).toHaveLength(FORMAT_FIXTURES.length);
    for (const result of results) {
      if (result.status === "extracted") {
        expect(result.extractedTextPath).not.toBeNull();
        expect(result.error).toBeNull();
      } else {
        // Unsupported formats must NOT pretend to have extracted text.
        expect(result.extractedTextPath).toBeNull();
      }
    }
  });

  it("builds an import preview that archives every fixture without hiding failures", () => {
    const preview = fixtureImportPreview();
    expect(preview.files).toHaveLength(FORMAT_FIXTURES.length);
    expect(preview.summary.archivedFiles).toBe(FORMAT_FIXTURES.length);
    expect(preview.summary.failedFiles).toBe(
      FORMAT_FIXTURES.filter((f) => f.status === "failed").length,
    );
  });

  it("exposes a clipboard markdown sample", () => {
    expect(CLIPBOARD_MARKDOWN).toContain("# Pasted");
    expect(CLIPBOARD_MARKDOWN).toContain("type: concept");
  });
});
