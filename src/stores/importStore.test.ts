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
