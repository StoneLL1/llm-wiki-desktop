import { describe, expect, it } from "vitest";

import type { ImportItem, ImportSession } from "../../types/importV2";
import {
  selectCommittableItems,
  selectImportViewModel,
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
    const authorization = makeItem("authorization", "waiting_authorization");
    const failed = makeItem("failed", "failed");
    const ready = makeItem("ready", "preview_ready");
    const session = makeSession([action, authorization, failed, ready]);

    expect(selectVisibleItems(session, "needs_action").map(({ itemId }) => itemId)).toEqual(["action", "authorization"]);
    expect(selectVisibleItems(session, "failed").map(({ itemId }) => itemId)).toEqual(["failed"]);
    expect(selectCommittableItems(session).map(({ itemId }) => itemId)).toEqual(["ready"]);
    expect(selectQueueCounts(session)).toMatchObject({ all: 4, needsAction: 2, failed: 1, ready: 1 });
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

  it("derives visible items, queue counts, and progress in one snapshot", () => {
    const session = makeSession([
      makeItem("login", "waiting_login"),
      makeItem("authorization", "waiting_authorization"),
      makeItem("failed", "failed"),
      makeItem("ready", "preview_ready"),
      makeItem("completed", "completed"),
      makeItem("skipped", "skipped"),
      makeItem("cancelled", "cancelled"),
      makeItem("active", "extracting"),
    ]);

    expect(selectImportViewModel(session, "needs_action")).toEqual({
      visibleItems: [session.items[0], session.items[1]],
      counts: {
        all: 6,
        active: 1,
        ready: 1,
        needsAction: 2,
        failed: 1,
        completed: 1,
        waiting: 2,
      },
      progress: {
        completed: 1,
        total: 8,
        active: 1,
        processed: 5,
        failed: 1,
        cancelled: 1,
        needsAction: 2,
      },
    });
  });

  it("counts an auto-finalizing exact duplicate preview as processed", () => {
    const duplicate = {
      ...makeItem("duplicate", "preview_ready"),
      preview: {
        resolution: { kind: "exact_duplicate" },
      } as NonNullable<ImportItem["preview"]>,
    };

    expect(selectImportViewModel(makeSession([duplicate]), "all").progress.processed).toBe(1);
  });
});
