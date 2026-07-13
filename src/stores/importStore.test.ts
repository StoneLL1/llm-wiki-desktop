import { beforeEach, describe, expect, it } from "vitest";

import type { ImportItem, ImportSession } from "../types/importV2";
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

beforeEach(() => {
  useImportStore.getState().reset();
});

describe("Import V2 session store", () => {
  it("recovers a session and appends new items without replacing existing items", () => {
    const first = session([item("one")]);
    useImportStore.getState().attachSession(projectA, first);
    useImportStore.getState().appendItems(projectA, [item("two")]);

    expect(useImportStore.getState().session?.items.map(({ itemId }) => itemId)).toEqual(["one", "two"]);
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
    useImportStore.getState().resetProjectPresentation(projectB);

    expect(useImportStore.getState()).toMatchObject({
      projectKey: projectB,
      session: null,
      selectedItemId: null,
      previewItemId: null,
      byokItemId: null,
      capabilityItemId: null,
      loginItemId: null,
      migrationDialogOpen: false,
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

  it.each<ImportQueueFilter>(["all", "active", "ready", "needs_action", "failed", "completed"])(
    "stores queue filter %s and dialog identities",
    (filter) => {
      useImportStore.getState().setFilter(filter);
      useImportStore.getState().openByok("one");
      useImportStore.getState().openCapability("one");
      useImportStore.getState().openLogin("one");
      useImportStore.getState().setMigrationDialogOpen(true);

      expect(useImportStore.getState()).toMatchObject({
        filter,
        byokItemId: "one",
        capabilityItemId: "one",
        loginItemId: "one",
        migrationDialogOpen: true,
      });
    },
  );
});
