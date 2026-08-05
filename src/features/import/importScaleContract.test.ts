import { describe, expect, it } from "vitest";

import importStoreSource from "../../stores/importStore.ts?raw";
import { importProjectKey, useImportStore } from "../../stores/importStore";
import type { ImportItem, ImportSession } from "../../types/importV2";

const SCALE_FIXTURES = [100, 1_000, 10_000] as const;
const GREEN_TARGETS = { maxFrontendPublicationsFor10_000TerminalItems: 100 } as const;
const projectKey = importProjectKey("batch-a", "D:/batch-a");

function item(index: number): ImportItem {
  return {
    itemId: `item-${index}`,
    input: { kind: "file", displayName: `${index}.md`, locator: `D:/batch-a/${index}.md`, normalizedLocator: null },
    status: "queued", selected: true, taskId: null, progress: null, attempts: [], preview: null, issue: null,
  };
}

function session(size: number): ImportSession {
  return {
    schemaVersion: 2, sessionId: "batch-a-session", projectId: "batch-a", status: "processing",
    resourceMode: "balanced", createdAt: "2026-08-05T00:00:00Z", updatedAt: "2026-08-05T00:00:00Z",
    items: Array.from({ length: size }, (_, index) => item(index)),
  };
}

describe("Batch A expected-red Import scale contract", () => {
  it("freezes synthetic item cardinalities without media, network, or AI work", () => {
    expect(SCALE_FIXTURES).toEqual([100, 1_000, 10_000]);
    expect(GREEN_TARGETS.maxFrontendPublicationsFor10_000TerminalItems).toBe(100);
  });

  it("records one frontend store publication per terminal item under the current path", () => {
    useImportStore.getState().reset();
    useImportStore.getState().attachSession(projectKey, session(10_000));
    let publications = 0;
    const unsubscribe = useImportStore.subscribe(() => { publications += 1; });
    for (let index = 0; index < 10_000; index += 1) {
      expect(useImportStore.getState().replaceItem(projectKey, { ...item(index), status: "completed" })).toBe(true);
    }
    unsubscribe();
    expect(publications).toBe(10_000);
    expect(useImportStore.getState().session?.items.every((value) => value.status === "completed")).toBe(true);
  }, 15_000);

  it("records the current full-session traversal boundary that Batch F must replace", () => {
    expect(importStoreSource).toContain("state.session.items.some");
    expect(importStoreSource).toContain("state.session.items.map");
  });

});
