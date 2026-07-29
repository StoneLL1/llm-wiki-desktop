import { describe, expect, it } from "vitest";

import contract from "../../test-fixtures/import-v2-batch0-contract.json";
import manifestContract from "../../tests/fixtures/import-v2/source-manifest-v3.json";
import type {
  ImportCompletion,
  ImportInputKind,
  ImportItemResolution,
  ImportPrimaryAction,
  ImportUserState,
  SourceFrontmatter,
  SourceManifest,
} from "./importV2";

describe("Import v2 Batch 0 shared Rust/TypeScript contract", () => {
  it("freezes every input kind, user state, and primary action", () => {
    const inputKinds = [
      "file",
      "folder",
      "url",
      "clipboard_text",
    ] as const satisfies readonly ImportInputKind[];
    const states = [
      "discovering",
      "processing",
      "needs_action",
      "ready",
      "committing",
      "committed",
      "failed",
    ] as const satisfies readonly ImportUserState[];
    const primaryActions = [
      "retry",
      "sign_in",
      "authorize",
      "install_capability",
      "enable_ocr",
      "authorize_local_asr",
      "invoke_local_agent",
      "review",
      "resolve",
      "resume",
    ] as const satisfies readonly ImportPrimaryAction[];

    expect(inputKinds).toEqual(contract.inputKinds);
    expect(states).toEqual(contract.userStates);
    expect(primaryActions).toEqual(contract.primaryActions);
  });

  it("freezes every per-item resolution with stale-decision bindings", () => {
    const binding = {
      sourceId: "src_a",
      candidateHash: "a".repeat(64),
      currentHash: "b".repeat(64),
      targetVersionId: "ver_b",
    };
    const resolutions = [
      { kind: "new_source" },
      { kind: "exact_duplicate_skip", ...binding },
      { kind: "same_source_new_version", ...binding },
      { kind: "keep_current_source", ...binding },
      { kind: "apply_import_candidate", ...binding },
      { kind: "manual_merge", ...binding, mergedHash: "c".repeat(64) },
    ] satisfies ImportItemResolution[];

    expect(resolutions).toEqual(contract.resolutions);
  });

  it("freezes the nested completion and secret-free Source frontmatter shapes", () => {
    const completion = {
      sessionId: "session-a",
      batchId: "batch-a",
      newSources: [{
        sourceId: "src_a",
        versionId: "ver_a",
        wikiPath: "wiki/sources/local/资料.md",
        contentHash: "d".repeat(64),
      }],
      updatedSources: [{
        sourceId: "src_b",
        versionId: "ver_b",
        wikiPath: "wiki/sources/web/example.com/访谈.md",
        contentHash: "e".repeat(64),
      }],
      duplicateSkips: [{
        itemId: "item-duplicate",
        sourceId: "src_a",
        versionId: "ver_a",
        contentHash: "d".repeat(64),
      }],
      warnings: [{
        code: "IMPORT_QUALITY_WARNING",
        title: "部分内容需要复核",
        dataSafety: "原始资料和已导入来源均未被覆盖。",
        primaryAction: "review",
        detail: {
          technicalCode: "IMPORT_QUALITY_WARNING",
          route: "file.native",
          engineId: "native-file",
          contentHash: "d".repeat(64),
        },
      }],
      failures: [{
        itemId: "item-failed",
        inputLabel: "录音.m4a",
        issue: {
          code: "IMPORT_ASR_ENGINE_UNAVAILABLE",
          title: "暂时无法生成文字稿",
          dataSafety: "没有写入原始资料或来源页。",
          primaryAction: "authorize_local_asr",
        },
      }],
    } satisfies ImportCompletion;
    const frontmatter = {
      type: "source",
      sourceId: "src_a",
      versionId: "ver_a",
      sourceKind: "web_media",
      title: "研发访谈",
      importedAt: "2026-07-25T00:00:00Z",
      contentHash: "f".repeat(64),
      platform: "example",
      canonicalUrl: "https://example.com/watch/1",
      platformContentId: "1",
      author: "Aletta",
      publishedAt: "2026-07-24T00:00:00Z",
      language: "zh-CN",
      quality: {
        level: "pass",
        metrics: [],
        warnings: [],
        sheetCountExact: 1,
        slideCountExact: 1,
        nonEmptyCellCoverage: 1,
        formulaValuePairs: 1,
        meaningfulImageCoverage: 1,
      },
      restricted: false,
    } satisfies SourceFrontmatter;

    expect(completion).toEqual(contract.completion);
    expect(frontmatter).toEqual(contract.sourceFrontmatter);
    expect(completion.newSources[0].wikiPath).toBe("wiki/sources/local/资料.md");
    expect(completion.warnings[0].primaryAction).toBe("review");
    expect(completion.failures[0].issue.primaryAction).toBe("authorize_local_asr");
    expect(frontmatter.type).toBe("source");
    expect(Object.keys(frontmatter)).not.toEqual(expect.arrayContaining([
      "cookie",
      "token",
      "stagingPath",
      "sessionId",
      "engineId",
    ]));
  });

  it("freezes the complete Source manifest v3 shape shared with Rust", () => {
    const manifest = manifestContract as unknown as SourceManifest;

    expect(manifest.schemaVersion).toBe(3);
    expect(Object.keys(manifest).sort()).toEqual([
      "aliases",
      "author",
      "canonicalUrl",
      "compiledConsumptions",
      "currentVersionId",
      "importedAt",
      "language",
      "origins",
      "platform",
      "platformContentId",
      "publishedAt",
      "restrictedContent",
      "schemaVersion",
      "sourceId",
      "sourceKind",
      "timeline",
      "title",
      "versions",
      "wikiPath",
    ].sort());
    expect(Object.keys(manifest.versions[0]).sort()).toEqual([
      "assets",
      "baselinePath",
      "candidate",
      "checkpoint",
      "contentHash",
      "createdAt",
      "humanEditHash",
      "provenance",
      "quality",
      "rawEvidence",
      "versionId",
    ].sort());
    expect(Object.keys(manifest.versions[0].candidate).sort()).toEqual([
      "author",
      "canonicalUrl",
      "language",
      "markdownHash",
      "platform",
      "platformContentId",
      "publishedAt",
      "sourceKind",
      "title",
    ].sort());
  });
});
