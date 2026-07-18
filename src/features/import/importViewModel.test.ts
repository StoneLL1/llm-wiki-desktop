import { describe, expect, it } from "vitest";

import type { ImportItem, ImportSession } from "../../types/importV2";
import {
  selectCommittableItems,
  selectQueueCounts,
  selectSessionProgress,
  selectVisibleItems,
} from "./importViewModel";

function makeItem(itemId: string, status: ImportItem["status"], selected = true): ImportItem {
  return {
    itemId,
    input: { kind: "file", displayName: itemId, locator: itemId, normalizedLocator: null },
    status,
    selected,
    taskId: null,
    progress: null,
    attempts: [],
    preview: null,
    issue: null,
  };
}

function makeSession(items: ImportItem[]): ImportSession {
  return {
    schemaVersion: 2,
    sessionId: "session",
    projectId: "project",
    status: "processing",
    resourceMode: "balanced",
    createdAt: "2026-07-13T00:00:00Z",
    updatedAt: "2026-07-13T00:00:00Z",
    items,
  };
}

describe("Import V2 queue selectors", () => {
  it("keeps required actions distinct from generic failures", () => {
    const action = makeItem("action", "waiting_login");
    const failed = makeItem("failed", "failed");
    const ready = makeItem("ready", "preview_ready");
    const session = makeSession([action, failed, ready]);

    expect(selectVisibleItems(session, "needs_action").map(({ itemId }) => itemId)).toEqual(["action"]);
    expect(selectVisibleItems(session, "failed").map(({ itemId }) => itemId)).toEqual(["failed"]);
    expect(selectCommittableItems(session).map(({ itemId }) => itemId)).toEqual(["ready"]);
    expect(selectQueueCounts(session)).toMatchObject({ all: 3, needsAction: 1, failed: 1, ready: 1 });
  });

  it("reports bounded progress from item statuses", () => {
    const session = makeSession([
      makeItem("queued", "queued"),
      makeItem("active", "extracting"),
      makeItem("ready", "preview_ready"),
      makeItem("done", "completed"),
    ]);

    expect(selectSessionProgress(session)).toEqual({ completed: 1, total: 4, active: 2, processed: 2, failed: 0, cancelled: 0, needsAction: 0 });
  });
});
