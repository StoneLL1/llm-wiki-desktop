import { describe, expect, it } from "vitest";

import type { ImportItem, ImportItemStatus, ImportUserState } from "../../types/importV2";
import { presentImportItem, type ImportItemAction } from "./importStatusPresentation";

function item(status: ImportItemStatus, overrides: Partial<ImportItem> = {}): ImportItem {
  return {
    itemId: `item-${status}`,
    input: {
      kind: "file",
      displayName: `${status}.md`,
      locator: `C:\\sources\\${status}.md`,
      normalizedLocator: null,
    },
    status,
    selected: false,
    taskId: null,
    progress: null,
    attempts: [],
    preview: null,
    issue: null,
    ...overrides,
  };
}

describe("presentImportItem", () => {
  it.each([
    ["queued", "discovering", "importV2.userState.discovering", "start", false],
    ["inspecting", "processing", "importV2.userState.processing", null, false],
    ["waiting_capability", "needs_action", "importV2.userState.needsAction", null, false],
    ["waiting_login", "needs_action", "importV2.userState.needsAction", null, false],
    ["waiting_authorization", "needs_action", "importV2.userState.needsAction", null, false],
    ["extracting", "processing", "importV2.userState.processing", null, false],
    ["validating", "processing", "importV2.userState.processing", null, false],
    ["preview_ready", "ready", "importV2.userState.ready", "preview_markdown", true],
    ["needs_merge", "needs_action", "importV2.userState.needsAction", "resolve_merge", false],
    ["committing", "committing", "importV2.userState.committing", null, false],
    ["completed", "committed", "importV2.userState.committed", "preview_markdown", false],
    ["paused", "needs_action", "importV2.userState.needsAction", "retry", false],
    ["cancelled", "failed", "importV2.userState.failed", "retry", false],
    ["skipped", "committed", "importV2.userState.committed", "retry", false],
    ["failed", "failed", "importV2.userState.failed", "retry", false],
  ] satisfies readonly [
    ImportItemStatus,
    ImportUserState,
    string,
    ImportItemAction | null,
    boolean,
  ][])(
    "maps internal status %s to one of the seven user states",
    (status, userState, labelKey, primaryAction, committable) => {
      const presentation = presentImportItem(item(status));

      expect(presentation.userState).toBe(userState);
      expect(presentation.labelKey).toBe(labelKey);
      expect(presentation.detailLabelKey).toBe(
        `importV2.itemStatus.${status.replace(/_([a-z])/g, (_, letter: string) => letter.toUpperCase())}`,
      );
      expect(presentation.primaryAction).toBe(primaryAction);
      expect(presentation.selectable).toBe(committable);
      expect(presentation.committable).toBe(committable);
    },
  );

  it("uses measured progress only when the backend provides a bounded total", () => {
    const presentation = presentImportItem(item("extracting", {
      progress: { current: 3, total: 10, label: "page 3" },
    }));

    expect(presentation.progressMode).toBe("measured");
    expect(presentation.progressValue).toBe(30);
    expect(presentation.progressLabel).toBe("page 3");
  });

  it("marks a persisted per-item merge decision ready without exposing the binding", () => {
    const presentation = presentImportItem(item("needs_merge", {
      selected: true,
      preview: {
        title: "notes",
        markdown: { kind: "markdown", relativePath: "candidate.md", sha256: "candidate-hash", sizeBytes: 10 },
        assets: [],
        sourceSnapshot: { kind: "source_snapshot", relativePath: "source.json", sha256: "source-hash", sizeBytes: 10 },
        quality: { level: "pass", metrics: [], warnings: [] },
        resolution: {
          kind: "needs_three_way_merge",
          binding: {
            sourceId: "source-a",
            candidateHash: "candidate-hash",
            currentHash: "current-hash",
            targetVersionId: "version-a",
          },
          defaultResolution: {
            kind: "keep_current_source",
            sourceId: "source-a",
            candidateHash: "candidate-hash",
            currentHash: "current-hash",
            targetVersionId: "version-a",
          },
        },
      },
    }));

    expect(presentation).toMatchObject({
      userState: "ready",
      detailLabelKey: "importV2.itemStatus.mergeResolved",
      selectable: true,
      committable: true,
      userIssue: null,
    });
  });

  it("keeps an exact duplicate out of commit selection", () => {
    const presentation = presentImportItem(item("preview_ready", {
      selected: true,
      preview: {
        title: "duplicate",
        markdown: { kind: "markdown", relativePath: "candidate.md", sha256: "same-hash", sizeBytes: 10 },
        assets: [],
        sourceSnapshot: { kind: "source_snapshot", relativePath: "source.json", sha256: "source-hash", sizeBytes: 10 },
        quality: { level: "pass", metrics: [], warnings: [] },
        resolution: { kind: "exact_duplicate" },
      },
    }));

    expect(presentation).toMatchObject({
      userState: "committed",
      selectable: false,
      committable: false,
    });
  });

  it("routes a restricted exact duplicate through confirmation instead of presenting it as committed", () => {
    const presentation = presentImportItem(item("preview_ready", {
      selected: true,
      restrictedContent: true,
      preview: {
        title: "restricted duplicate",
        markdown: { kind: "markdown", relativePath: "candidate.md", sha256: "same-hash", sizeBytes: 10 },
        assets: [],
        sourceSnapshot: { kind: "source_snapshot", relativePath: "source.json", sha256: "source-hash", sizeBytes: 10 },
        quality: { level: "pass", metrics: [], warnings: [] },
        resolution: { kind: "exact_duplicate" },
      },
    }));

    expect(presentation).toMatchObject({
      userState: "ready",
      selectable: false,
      committable: true,
      userIssue: null,
    });
  });

  it("surfaces an exact-duplicate commit failure as failed and retryable", () => {
    const presentation = presentImportItem(item("failed", {
      preview: {
        title: "duplicate",
        markdown: { kind: "markdown", relativePath: "candidate.md", sha256: "same-hash", sizeBytes: 10 },
        assets: [],
        sourceSnapshot: { kind: "source_snapshot", relativePath: "source.json", sha256: "source-hash", sizeBytes: 10 },
        quality: { level: "pass", metrics: [], warnings: [] },
        resolution: { kind: "exact_duplicate" },
      },
      issue: {
        code: "IMPORT_V2_COMMIT_FAILED",
        message: "commit failed",
        stage: "commit",
        retryable: true,
        userActionRequired: true,
        recoveryActions: ["retry", "view_log"],
        availableActions: [],
      },
    }));

    expect(presentation).toMatchObject({
      userState: "failed",
      committable: false,
      primaryAction: "retry",
    });
    expect(presentation.userIssue?.detail?.technicalCode).toBe("IMPORT_V2_COMMIT_FAILED");
  });

  it("presents streamed ASR progress as a real percentage", () => {
    const presentation = presentImportItem(item("extracting", {
      progress: { current: 48, total: 100, label: "asr.recognizing" },
    }));

    expect(presentation.progressMode).toBe("measured");
    expect(presentation.progressValue).toBe(48);
    expect(presentation.progressLabel).toBe("asr.recognizing");
  });

  it("derives the primary action only from typed backend recovery actions", () => {
    const presentation = presentImportItem(item("waiting_authorization", {
      input: {
        kind: "url",
        displayName: "video",
        locator: "https://example.com/video/1",
        normalizedLocator: "https://example.com/video/1",
      },
      issue: {
        code: "IMPORT_WEB_SUBTITLE_UNAVAILABLE",
        message: "subtitle missing",
        stage: "extract",
        retryable: true,
        userActionRequired: true,
        recoveryActions: ["authorize_local_asr"],
        availableActions: [],
      },
    }));

    expect(presentation.primaryAction).toBe("authorize_local_asr");
    expect(presentation.secondaryActions).toEqual(["cancel"]);
    expect(presentation.userIssue).toMatchObject({
      title: "importV2.issue.asr.title",
      dataSafety: "importV2.issue.asr.dataSafety",
      primaryAction: "authorize_local_asr",
    });
  });

  it("keeps multiple typed recovery choices behind one primary action", () => {
    const presentation = presentImportItem(item("failed", {
      issue: {
        code: "IMPORT_FILE_QUALITY_FAILED",
        message: "quality failed",
        stage: "validate",
        retryable: true,
        userActionRequired: true,
        recoveryActions: ["enable_ocr", "switch_parser", "switch_route", "retry_route", "skip"],
        availableActions: [],
      },
    }));

    expect(presentation.primaryAction).toBe("enable_ocr");
    expect(presentation.secondaryActions).toEqual([
      "retry",
      "switch_parser",
      "switch_route",
      "retry_route",
      "skip",
    ]);
  });

  it("does not infer recovery from locator, extension, code, or raw message text", () => {
    const presentation = presentImportItem(item("failed", {
      input: {
        kind: "url",
        displayName: "video.pdf",
        locator: "https://www.bilibili.com/video/BV1test",
        normalizedLocator: "https://www.bilibili.com/video/BV1test",
      },
      issue: {
        code: "OCR_LOGIN_CAPABILITY_ASR",
        message: "install OCR, log in, and enable ASR",
        stage: "extract",
        retryable: false,
        userActionRequired: true,
        recoveryActions: [],
        availableActions: [],
      },
    }));

    expect(presentation.primaryAction).toBeNull();
    expect(presentation.actions).toEqual(["preserve_remote_media"]);
  });

  it("exposes technical diagnostics only through the UserIssue detail", () => {
    const presentation = presentImportItem(item("failed", {
      attempts: [{
        route: "native.file",
        engineId: "native",
        engineVersion: "1",
        stage: "extract",
        startedAt: "2026-07-27T00:00:00Z",
        completedAt: "2026-07-27T00:00:01Z",
        outcome: "failed",
        warnings: [],
      }],
      issue: {
        code: "FAILED",
        message: "raw backend message",
        stage: "extract",
        retryable: false,
        userActionRequired: false,
        recoveryActions: [],
        availableActions: [],
      },
    }));

    expect(presentation.userIssue?.title).toBe("importV2.issue.failed.title");
    expect(presentation.userIssue?.detail).toMatchObject({
      technicalCode: "FAILED",
      technicalMessage: "raw backend message",
      route: "native.file",
      engineId: "native 1",
    });
  });
});
