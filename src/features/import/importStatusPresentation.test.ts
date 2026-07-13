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
  it.each([
    ["queued", "importV2.itemStatus.queued", "queue", "none", ["start", "cancel"], false],
    ["inspecting", "importV2.itemStatus.inspecting", "scan", "indeterminate", ["cancel"], false],
    ["waiting_capability", "importV2.itemStatus.waitingCapability", "capability", "none", ["view_capability", "cancel"], false],
    ["waiting_login", "importV2.itemStatus.waitingLogin", "login", "none", ["begin_login", "cancel"], false],
    ["extracting", "importV2.itemStatus.extracting", "scan", "indeterminate", ["cancel"], false],
    ["validating", "importV2.itemStatus.validating", "shield", "indeterminate", ["cancel"], false],
    ["preview_ready", "importV2.itemStatus.previewReady", "ready", "none", ["inspect", "preview_markdown"], true],
    ["needs_merge", "importV2.itemStatus.needsMerge", "merge", "none", ["compare_candidate", "resolve_merge", "discard_candidate"], true],
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
});
