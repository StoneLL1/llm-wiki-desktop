import { describe, expect, it } from "vitest";

import type { ImportItem, ImportItemStatus } from "../../types/importV2";
import { presentImportItem } from "./importStatusPresentation";

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
  it("offers a metadata-only preview when a transcript is unavailable", () => {
    const presentation = presentImportItem(item("waiting_authorization", {
      input: {
        kind: "url",
        displayName: "video",
        locator: "https://www.bilibili.com/video/BV1test",
        normalizedLocator: "https://www.bilibili.com/video/BV1test",
      },
      issue: {
        code: "IMPORT_WEB_SUBTITLE_UNAVAILABLE",
        message: "subtitle missing",
        stage: "extract",
        retryable: true,
        userActionRequired: true,
        recoveryActions: ["authorize_local_asr", "preview_without_transcript"],
        availableActions: [],
      },
    }));
    expect(presentation.actions).toContain("preview_without_transcript");
  });

  it.each([
    ["queued", "importV2.itemStatus.queued", "queue", "none", ["start", "cancel"], false],
    ["inspecting", "importV2.itemStatus.inspecting", "scan", "indeterminate", ["cancel"], false],
    ["waiting_capability", "importV2.itemStatus.waitingCapability", "capability", "none", ["view_capability", "cancel"], false],
    ["waiting_login", "importV2.itemStatus.waitingLogin", "login", "none", ["begin_login", "cancel"], false],
    ["waiting_authorization", "importV2.itemStatus.waitingAuthorization", "shield", "none", ["authorize_local_asr", "cancel"], false],
    ["extracting", "importV2.itemStatus.extracting", "scan", "indeterminate", ["cancel"], false],
    ["validating", "importV2.itemStatus.validating", "shield", "indeterminate", ["cancel"], false],
    ["preview_ready", "importV2.itemStatus.previewReady", "ready", "none", ["preview_markdown"], true],
    ["needs_merge", "importV2.itemStatus.needsMerge", "merge", "none", ["compare_candidate", "resolve_merge", "discard_candidate"], false],
    ["committing", "importV2.itemStatus.committing", "commit", "indeterminate", ["cancel"], false],
    ["completed", "importV2.itemStatus.completed", "completed", "none", ["open_result", "preview_markdown"], false],
    ["paused", "importV2.itemStatus.paused", "pause", "none", ["retry", "cancel"], false],
    ["cancelled", "importV2.itemStatus.cancelled", "cancelled", "none", ["retry"], false],
    ["skipped", "importV2.itemStatus.skipped", "skipped", "none", ["retry"], false],
    ["failed", "importV2.itemStatus.failed", "failed", "none", ["retry"], false],
  ] as const)("maps %s to its stable presentation contract", (status, labelKey, icon, progressMode, actions, committable) => {
    const presentation = presentImportItem(item(status));

    expect(presentation.labelKey).toBe(labelKey);
    expect(presentation.icon).toBe(icon);
    expect(presentation.progressMode).toBe(progressMode);
    expect(presentation.actions).toEqual(actions);
    expect(presentation.selectable).toBe(committable);
    expect(presentation.committable).toBe(committable);
  });

  it("uses measured progress only when the backend provides a bounded total", () => {
    const presentation = presentImportItem(item("extracting", {
      progress: { current: 3, total: 10, label: "page 3" },
    }));

    expect(presentation.progressMode).toBe("measured");
    expect(presentation.progressValue).toBe(30);
    expect(presentation.progressLabel).toBe("page 3");
  });

  it("adds only backend-authorized recovery actions", () => {
    const presentation = presentImportItem(item("failed", {
      issue: {
        code: "FAILED",
        message: "failed",
        stage: "extract",
        retryable: true,
        userActionRequired: true,
        recoveryActions: ["retry"],
        availableActions: ["invoke_local_agent", "request_byok"],
      },
    }));

    expect(presentation.actions).toEqual(["retry", "invoke_local_agent", "request_byok"]);
  });

  it("does not offer retry when the backend marks a failure non-retryable", () => {
    const presentation = presentImportItem(item("failed", {
      input: {
        kind: "file",
        displayName: "failed.pdf",
        locator: "C:\\sources\\failed.pdf",
        normalizedLocator: null,
      },
      issue: {
        code: "FAILED",
        message: "failed",
        stage: "extract",
        retryable: false,
        userActionRequired: true,
        recoveryActions: [],
        availableActions: [],
      },
    }));

    expect(presentation.actions).toEqual([]);
  });

  it("turns parser, OCR, route, and skip recovery codes into executable queue actions", () => {
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
      input: {
        kind: "file",
        displayName: "failed.pdf",
        locator: "C:\\sources\\failed.pdf",
        normalizedLocator: null,
      },
    }));

    expect(presentation.actions).toEqual([
      "retry",
      "retry_route",
      "switch_route",
      "switch_parser",
      "enable_ocr",
      "skip",
    ]);
  });

  it("exposes a task-log recovery action only when the item has a task", () => {
    const withTask = presentImportItem(item("failed", {
      taskId: "task-1",
      issue: {
        code: "FAILED",
        message: "failed",
        stage: "extract",
        retryable: false,
        userActionRequired: true,
        recoveryActions: ["view_log"],
        availableActions: [],
      },
    }));
    const withoutTask = presentImportItem(item("failed", {
      issue: {
        code: "FAILED",
        message: "failed",
        stage: "extract",
        retryable: false,
        userActionRequired: true,
        recoveryActions: ["view_log"],
        availableActions: [],
      },
    }));

    expect(withTask.actions).toContain("view_log");
    expect(withoutTask.actions).not.toContain("view_log");
  });

  it("offers explicit OCR for supported platform URL previews and failures", () => {
    const input = {
      kind: "url" as const,
      displayName: "XHS post",
      locator: "https://www.xiaohongshu.com/explore/note-1",
      normalizedLocator: "https://www.xiaohongshu.com/explore/note-1",
    };
    expect(presentImportItem(item("preview_ready", { input })).actions).toEqual([
      "preview_markdown",
      "enable_ocr",
    ]);
    expect(presentImportItem(item("failed", { input })).actions).toEqual(["retry", "enable_ocr"]);
    expect(presentImportItem(item("preview_ready", {
      input: { ...input, locator: "https://example.com/article", normalizedLocator: null },
    })).actions).not.toContain("enable_ocr");
  });
});
