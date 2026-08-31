import { describe, expect, it } from "vitest";

import importStoreSource from "../../stores/importStore.ts?raw";
import importTaskCoordinatorSource from "./useImportTaskCoordinator.ts?raw";
import importBatchControllerSource from "./useImportBatchController.ts?raw";
import importViewSource from "./ImportView.tsx?raw";
import importWorkflowSource from "./useImportWorkflow.ts?raw";
import importActionGroupsSource from "./ImportActionGroups.tsx?raw";
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

describe("Batch F Import scale contract", () => {
  it("freezes synthetic item cardinalities without media, network, or AI work", () => {
    expect(SCALE_FIXTURES).toEqual([100, 1_000, 10_000]);
    expect(GREEN_TARGETS.maxFrontendPublicationsFor10_000TerminalItems).toBe(100);
  });

  // Batch A's evidence index retains this witness name until Batch G retires
  // the expected-red catalog; the executable assertions below are the Batch F
  // green contract and must observe one publication for the whole patch.
  it("records one frontend store publication per terminal item under the current path", () => {
    useImportStore.getState().reset();
    useImportStore.getState().attachSession(projectKey, session(10_000));
    let publications = 0;
    const unsubscribe = useImportStore.subscribe(() => { publications += 1; });
    const patch = Array.from(
      { length: 10_000 },
      (_, index) => ({ ...item(index), status: "completed" as const }),
    );
    expect(useImportStore.getState().patchItems(projectKey, patch)).toBe(true);
    unsubscribe();
    expect(publications).toBe(1);
    expect(publications).toBeLessThanOrEqual(GREEN_TARGETS.maxFrontendPublicationsFor10_000TerminalItems);
    expect(useImportStore.getState().session?.items.length).toBeLessThanOrEqual(600);
    expect(Object.values(useImportStore.getState().itemById).every((value) => value.status === "completed")).toBe(true);
  }, 15_000);

  it("freezes the one-map, one-traversal bulk patch implementation", () => {
    expect(importStoreSource).toContain("patchItems:");
    expect(importStoreSource).toContain("const current = itemById[item.itemId]");
    expect(importStoreSource).toContain("applyProjectionDelta");
    expect(importStoreSource).toContain("itemById");
    expect(importStoreSource).not.toContain("items: state.session.items.map");
    expect(importStoreSource).not.toContain("authoritativeCounts");
  });

  it("freezes pending-task reconciliation on normalized indexes", () => {
    expect(importTaskCoordinatorSource).toContain("itemIdsByTaskId");
    expect(importTaskCoordinatorSource).not.toContain("session.items.some");
    expect(importBatchControllerSource).toContain("itemIdsByTaskId");
    expect(importBatchControllerSource).not.toContain("taskList.some");
    expect(importViewSource).toContain("onSetItemSelected={handleQueueSelection}");
    expect(importViewSource).toContain("onAction={handleQueueAction}");
    expect(importWorkflowSource).toContain("loadMoreRequestRef");
  });

  it("keeps discovery work scope and action groups independent from the loaded page window", () => {
    expect(importTaskCoordinatorSource).not.toContain("startNewQueuedItems");
    expect(importTaskCoordinatorSource).not.toContain("Object.values(current.itemById)");
    expect(importActionGroupsSource).toContain("groups: readonly ImportSessionActionGroup[]");
    expect(importActionGroupsSource).not.toContain("capabilityIdForItem");
    const startItemsBoundary = importWorkflowSource
      .split("const startItems = useCallback")[1]
      .split("const retryItem = useCallback")[0];
    const asrGroupBoundary = importWorkflowSource
      .split("const authorizeLocalAsrGroup = useCallback")[1]
      .split("const authorizeLocalAsr = useCallback")[0];
    const ocrGroupBoundary = importWorkflowSource
      .split("const authorizeLocalOcrGroup = useCallback")[1]
      .split("const authorizeLocalOcr = useCallback")[0];
    expect(startItemsBoundary).not.toContain("knownItemIds");
    expect(asrGroupBoundary).not.toContain("knownItemIds");
    expect(ocrGroupBoundary).not.toContain("knownItemIds");
    expect(importWorkflowSource).toContain('item.status === "queued" && !item.taskId');
  });

});
