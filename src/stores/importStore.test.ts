import { beforeEach, describe, expect, it } from "vitest";

import type { ImportItem, ImportItemPage, ImportSession, ImportSessionOverview } from "../types/importV2";
import {
  useImportStore,
  type ImportQueueFilter,
} from "./importStore";

const projectA = "project-a\0D:/wiki/a";
const projectB = "project-b\0D:/wiki/b";

function item(itemId: string, status: ImportItem["status"] = "queued", selected = true): ImportItem {
  return {
    itemId,
    input: {
      kind: "file",
      displayName: `${itemId}.md`,
      locator: `D:/sources/${itemId}.md`,
      normalizedLocator: null,
    },
    status,
    selected,
    taskId: null,
    progress: null,
    attempts: [],
    preview: null,
    issue: null,
  };
}

function session(items: ImportItem[]): ImportSession {
  return {
    schemaVersion: 2,
    sessionId: "session-a",
    projectId: "project-a",
    status: "processing",
    resourceMode: "balanced",
    createdAt: "2026-07-13T00:00:00Z",
    updatedAt: "2026-07-13T00:00:00Z",
    items,
  };
}

function overview(itemCount: number): ImportSessionOverview {
  return {
    schemaVersion: 2,
    sessionId: "session-a",
    projectId: "project-a",
    status: "processing",
    resourceMode: "balanced",
    createdAt: "2026-07-13T00:00:00Z",
    updatedAt: "2026-07-13T00:00:00Z",
    itemCount,
    semanticRevision: 7,
    selectionRevision: 3,
    confirmationDigest: "digest",
    counts: { all: itemCount, active: itemCount, ready: 0, needsAction: 0, failed: 0, completed: 0, waiting: 0, processed: 0, cancelled: 0 },
    statusCounts: { queued: itemCount, inspecting: 0, waitingCapability: 0, waitingLogin: 0, waitingAuthorization: 0, extracting: 0, validating: 0, previewReady: 0, needsMerge: 0, committing: 0, completed: 0, paused: 0, cancelled: 0, skipped: 0, failed: 0 },
    selection: { selected: 0, newSources: 0, updates: 0, warnings: 0, pending: 0, restricted: 0 },
    indexState: "ready",
  };
}

function page(start: number, total = 10_000, nextCursor: string | null = null): ImportItemPage {
  return {
    sessionId: "session-a",
    snapshotRevision: 7,
    items: Array.from({ length: 200 }, (_, offset) => item(`item-${start + offset}`)),
    nextCursor,
    total,
  };
}

beforeEach(() => {
  useImportStore.getState().reset();
});

describe("Import V2 session store", () => {
  it("attaches a recovered session for the current project", () => {
    const first = session([item("one")]);
    useImportStore.getState().attachSession(projectA, first);

    expect(useImportStore.getState().session?.items.map(({ itemId }) => itemId)).toEqual(["one"]);
  });

  it("replaces a refreshed item by identity and clears a disappeared selection", () => {
    useImportStore.getState().attachSession(projectA, session([item("one"), item("two")]));
    useImportStore.getState().selectItem("two");
    useImportStore.getState().replaceItem(projectA, item("one", "preview_ready"));
    useImportStore.getState().replaceSession(projectA, session([item("one", "completed")]));

    expect(useImportStore.getState().session?.items[0].status).toBe("completed");
    expect(useImportStore.getState().selectedItemId).toBeNull();
  });

  it("resets presentation on project change without owning or erasing task facts", () => {
    useImportStore.getState().attachSession(projectA, session([item("one")]));
    useImportStore.getState().openPreview("one");
    useImportStore.getState().setIsConfirming(true);
    useImportStore.getState().resetProjectPresentation(projectB);

    expect(useImportStore.getState()).toMatchObject({
      projectKey: projectB,
      session: null,
      selectedItemId: null,
      isConfirming: false,
      previewItemId: null,
      capabilityItemId: null,
      loginItemId: null,
    });
  });

  it("rejects a stale epoch after a newer session read starts", () => {
    useImportStore.getState().resetProjectPresentation(projectA);
    const oldEpoch = useImportStore.getState().beginSessionEpoch(projectA);
    const newEpoch = useImportStore.getState().beginSessionEpoch(projectA);

    expect(useImportStore.getState().attachSession(projectA, session([item("old")]), oldEpoch)).toBe(false);
    expect(useImportStore.getState().attachSession(projectA, session([item("new")]), newEpoch)).toBe(true);
    expect(useImportStore.getState().session?.items[0].itemId).toBe("new");
  });

  it("keeps a terminal item update scoped to the active project key", () => {
    useImportStore.getState().attachSession(projectA, session([item("one")]));
    expect(useImportStore.getState().replaceItem(projectB, item("one", "completed"))).toBe(false);
    expect(useImportStore.getState().session?.items[0].status).toBe("queued");
  });

  it("applies a cohort patch in one publication and rejects a stale epoch", () => {
    useImportStore.getState().attachSession(projectA, session([item("one"), item("two")]));
    const epoch = useImportStore.getState().sessionEpoch;
    let publications = 0;
    const unsubscribe = useImportStore.subscribe(() => { publications += 1; });

    expect(useImportStore.getState().patchItems(projectA, [
      item("one", "preview_ready"),
      item("two", "failed"),
      item("three", "queued"),
    ], epoch)).toBe(true);
    expect(useImportStore.getState().patchItems(projectA, [item("one", "completed")], epoch + 1)).toBe(false);
    unsubscribe();

    expect(publications).toBe(1);
    expect(Object.values(useImportStore.getState().itemById).map(({ status }) => status)).toEqual([
      "preview_ready", "failed",
    ]);
    expect(useImportStore.getState().session?.items.map(({ status }) => status)).toEqual(["preview_ready", "failed"]);
  });

  it("keeps the highest item revision when patches arrive out of order", () => {
    useImportStore.getState().attachSession(projectA, session([
      { ...item("one", "extracting"), itemRevision: 2 },
    ]));

    useImportStore.getState().patchItems(projectA, [
      { ...item("one", "completed"), itemRevision: 4 },
    ]);
    useImportStore.getState().patchItems(projectA, [
      { ...item("one", "validating"), itemRevision: 3 },
    ]);

    expect(useImportStore.getState().itemById.one).toMatchObject({
      status: "completed",
      itemRevision: 4,
    });
  });

  it("normalizes bounded pages, keeps only a three-page item window, and preserves overview counts", () => {
    useImportStore.getState().attachSessionWindow(projectA, overview(10_000), page(0, 10_000, "cursor-200"));
    useImportStore.getState().appendItemPage(projectA, page(200, 10_000, "cursor-400"));
    useImportStore.getState().appendItemPage(projectA, page(400, 10_000, "cursor-600"));
    useImportStore.getState().appendItemPage(projectA, page(600, 10_000, "cursor-800"));

    const state = useImportStore.getState();
    expect(state.loadedPages).toHaveLength(3);
    expect(Object.keys(state.orderedItemIdsByPage)).toHaveLength(3);
    expect(Object.keys(state.itemById)).toHaveLength(600);
    expect(state.session?.items).toHaveLength(600);
    expect(state.loadedItemStartIndex).toBe(200);
    expect(state.counts.all).toBe(10_000);
    expect(state.progress.total).toBe(10_000);
    expect(state.orderedItemIdsByPage[state.loadedPages[0] ?? ""]?.[0]).toBe("item-200");
  });

  it("patches normalized items by identity without replacing unrelated item objects", () => {
    useImportStore.getState().attachSession(projectA, session(Array.from({ length: 10_000 }, (_, index) => item(`item-${index}`))));
    const before = useImportStore.getState();
    const unchanged = before.itemById["item-1"];

    useImportStore.getState().patchItems(projectA, [{
      ...item("item-0", "preview_ready"),
      itemRevision: 2,
    }]);

    const after = useImportStore.getState();
    expect(after.itemById["item-0"]?.status).toBe("preview_ready");
    expect(after.itemById["item-1"]).toBe(unchanged);
    expect(after.session?.items.length).toBeLessThanOrEqual(600);
    expect(after.counts).toMatchObject({ active: 9_999, ready: 1 });
  });

  it("accepts same-revision local overlays without weakening authoritative revision ordering", () => {
    useImportStore.getState().attachSession(projectA, session([{ ...item("one"), itemRevision: 4 }]));

    expect(useImportStore.getState().replaceItemLocal(projectA, {
      ...item("one", "inspecting", false),
      itemRevision: 4,
      taskId: "task-one",
      progress: { current: 1, total: 3, label: "Inspecting input" },
    })).toBe(true);
    expect(useImportStore.getState().itemById.one).toMatchObject({ selected: false, taskId: "task-one", itemRevision: 4 });
    expect(useImportStore.getState().itemIdsByTaskId["task-one"]).toEqual(["one"]);

    expect(useImportStore.getState().replaceItem(projectA, { ...item("one", "completed"), itemRevision: 4 })).toBe(false);
    expect(useImportStore.getState().itemById.one?.status).toBe("inspecting");
  });

  it("keeps operation-local counters separate from session-wide Queue totals", () => {
    useImportStore.getState().attachSession(projectA, session(Array.from({ length: 10_000 }, (_, index) => item(`item-${index}`))));
    useImportStore.getState().recordOperationCounts(projectA, "batch-small", {
      total: 5, processed: 3, succeeded: 2, waiting: 0, failed: 1, cancelled: 0,
    }, [item("item-2", "failed")]);

    expect(useImportStore.getState().progress.total).toBe(10_000);
    expect(useImportStore.getState().operationCountsByBatchId["batch-small"]?.total).toBe(5);
    expect(useImportStore.getState().operationFailedItemIdsByBatchId["batch-small"]).toEqual(["item-2"]);
  });

  it("advances sparse filtered page offsets by actual matching rows", () => {
    const sparsePage = (start: number, cursor: string | null): ImportItemPage => ({
      sessionId: "session-a", snapshotRevision: 7,
      items: [item(`sparse-${start}`), item(`sparse-${start + 1}`)], nextCursor: cursor, total: 8,
    });
    useImportStore.getState().attachSessionWindow(projectA, overview(8), sparsePage(0, "cursor-1"));
    useImportStore.getState().appendItemPage(projectA, sparsePage(2, "cursor-2"));
    useImportStore.getState().appendItemPage(projectA, sparsePage(4, "cursor-3"));
    useImportStore.getState().appendItemPage(projectA, sparsePage(6, null));

    expect(useImportStore.getState().loadedItemStartIndex).toBe(2);
    expect(useImportStore.getState().session?.items).toHaveLength(6);
  });

  it("keeps page ordering stable for progress-only patches and updates it only on filter membership changes", () => {
    useImportStore.getState().attachSession(projectA, session([item("one"), item("two")]));
    const pageOrder = useImportStore.getState().orderedItemIdsByPage;
    const counts = useImportStore.getState().counts;
    const progress = useImportStore.getState().progress;

    useImportStore.getState().patchItems(projectA, [{ ...item("one"), progress: { current: 1, total: 2, label: "items" } }]);
    expect(useImportStore.getState().orderedItemIdsByPage).toBe(pageOrder);
    expect(useImportStore.getState().counts).toBe(counts);
    expect(useImportStore.getState().progress).toBe(progress);

    useImportStore.getState().patchItems(projectA, [item("one", "completed")]);
    expect(useImportStore.getState().orderedItemIdsByPage).not.toBe(pageOrder);
    expect(useImportStore.getState().session?.items.map(({ itemId }) => itemId)).toEqual(["two"]);
    expect(useImportStore.getState().itemPageTotal).toBe(1);
  });

  it("indexes task membership without rescanning the session", () => {
    useImportStore.getState().attachSession(projectA, session([
      { ...item("one"), taskId: "task-a" },
      { ...item("two"), taskId: "task-a" },
      { ...item("three"), taskId: "task-b" },
    ]));

    expect(useImportStore.getState().itemIdsByTaskId).toEqual({
      "task-a": ["one", "two"],
      "task-b": ["three"],
    });
  });

  it.each<ImportQueueFilter>(["all", "active", "ready", "needs_action", "failed", "completed"])(
    "stores queue filter %s and item-scoped dialog identities",
    (filter) => {
      useImportStore.getState().setFilter(filter);
      useImportStore.getState().openCapability("one");
      useImportStore.getState().openLogin("one");

      expect(useImportStore.getState()).toMatchObject({
        filter,
        capabilityItemId: "one",
        loginItemId: "one",
      });
    },
  );
});
