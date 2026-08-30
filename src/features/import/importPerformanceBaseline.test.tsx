import { fireEvent, render, screen } from "@testing-library/react";
import { Profiler } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import { TaskEventDispatcher } from "../../services/taskEventDispatcher";
import { importProjectKey, useImportStore } from "../../stores/importStore";
import { handleTaskEvent, useTaskStore } from "../../stores/taskStore";
import { createReactCommitObserver, observeStorePublications } from "../../test/performanceObservers";
import type { ImportItem, ImportSession } from "../../types/importV2";
import type { BackendEvent, BackendTask } from "../../types/task";
import { ImportQueue } from "./ImportQueue";

const statusRenders = vi.hoisted(() => new Map<string, number>());
vi.mock("./ImportItemStatus", () => ({
  ImportItemStatus: ({ item: renderedItem }: { item: ImportItem }) => {
    statusRenders.set(renderedItem.itemId, (statusRenders.get(renderedItem.itemId) ?? 0) + 1);
    return <span data-testid={`status-render-${renderedItem.itemId}`} />;
  },
}));

const SCALE = [100, 1_000, 10_000] as const;
const projectKey = importProjectKey("perf-project", "D:/fixture-project");
const onFilterChange = vi.fn();
const onSelectItem = vi.fn();
const onSetItemSelected = vi.fn();
const onAction = vi.fn();

function task(progress: number): BackendTask {
  return {
    id: "task-import-1",
    taskType: "import",
    projectId: "perf-project",
    title: "Synthetic Import",
    status: "running",
    progress: { current: progress, total: 100, label: "Synthetic progress" },
    startedAt: "2026-08-27T00:00:00Z",
    updatedAt: `2026-08-27T00:00:${String(progress).padStart(2, "0")}Z`,
    completedAt: null,
    cancellable: true,
    logPath: null,
    result: null,
    error: null,
  };
}

function taskEvent(progress: number): BackendEvent {
  return {
    eventId: `event-${progress}`,
    eventType: "task_updated",
    projectId: "perf-project",
    taskId: "task-import-1",
    timestamp: `2026-08-27T00:00:${String(progress).padStart(2, "0")}Z`,
    payload: task(progress),
  };
}

function item(index: number): ImportItem {
  return {
    itemId: `item-${index}`,
    input: {
      kind: "file",
      displayName: `perf-${String(index).padStart(5, "0")}.md`,
      locator: `fixture/perf-${String(index).padStart(5, "0")}.md`,
      normalizedLocator: null,
    },
    status: "extracting",
    selected: true,
    taskId: "task-import-1",
    progress: { current: 1, total: 100, label: "Synthetic progress" },
    attempts: [],
    preview: null,
    issue: null,
  };
}

function session(size: number): ImportSession {
  return {
    schemaVersion: 2,
    sessionId: "session-perf",
    projectId: "perf-project",
    status: "processing",
    resourceMode: "balanced",
    createdAt: "2026-08-27T00:00:00Z",
    updatedAt: "2026-08-27T00:00:00Z",
    items: Array.from({ length: size }, (_, index) => item(index)),
  };
}

function queue(items: readonly ImportItem[], observer?: ReturnType<typeof createReactCommitObserver>) {
  const content = (
    <ImportQueue
      items={items}
      totalItems={items.length}
      counts={{ all: items.length, active: items.length, ready: 0, needsAction: 0, failed: 0, completed: 0 }}
      progress={{ completed: 0, total: items.length, active: items.length }}
      selectedItemId={null}
      filter="all"
      onFilterChange={onFilterChange}
      onSelectItem={onSelectItem}
      onSetItemSelected={onSetItemSelected}
      onAction={onAction}
    />
  );
  return observer ? <Profiler id="import-queue-baseline" onRender={observer.onRender}>{content}</Profiler> : content;
}

beforeEach(async () => {
  await i18next.changeLanguage("en");
  useTaskStore.setState({
    activeProjectId: "perf-project",
    activeProjectRootPath: "D:/fixture-project",
    taskById: {},
    taskIdsByProject: {},
    runningCountByProject: {},
    taskFacts: {},
    tasks: [],
    logs: {},
    activities: {},
    taskOutputs: {},
    drawerOpen: false,
    selectedTaskId: null,
    runningCount: 0,
    tasksHydrated: true,
    projectPersistence: "persistent",
    projectPersistenceReason: null,
  });
  useImportStore.getState().reset();
  statusRenders.clear();
});

describe("three-core performance frontend contracts", () => {
  it("freezes the N fixtures used by store and Queue observers", () => {
    expect(SCALE).toEqual([100, 1_000, 10_000]);
  });

  it("publishes one task fact when owner and a legacy listener observe the same snapshot", () => {
    const dispatcher = new TaskEventDispatcher();
    dispatcher.registerOwner(handleTaskEvent);
    dispatcher.register((event) => useTaskStore.getState().upsertTask(event.payload as BackendTask));
    const observer = observeStorePublications((listener) => useTaskStore.subscribe(listener));

    dispatcher.dispatch(taskEvent(1));

    observer.stop();
    expect(observer.publications).toBe(1);
  });

  it("publishes task progress no faster than 5 Hz during a 10 Hz replay", () => {
    vi.useFakeTimers();
    const dispatcher = new TaskEventDispatcher();
    dispatcher.registerOwner(handleTaskEvent);
    dispatcher.register((event) => useTaskStore.getState().upsertTask(event.payload as BackendTask));
    const observer = observeStorePublications((listener) => useTaskStore.subscribe(listener));

    for (let progress = 1; progress <= 10; progress += 1) {
      dispatcher.dispatch(taskEvent(progress));
      vi.advanceTimersByTime(100);
    }
    vi.advanceTimersByTime(250);

    observer.stop();
    expect(observer.publications).toBeLessThanOrEqual(5);
    expect(useTaskStore.getState().tasks[0]?.progress?.current).toBe(10);
    vi.useRealTimers();
  });

  it("replaces only the changed normalized Import item during a 10k session patch", () => {
    useImportStore.getState().attachSession(projectKey, session(10_000));
    const before = useImportStore.getState().itemById;
    const unchanged = before["item-1"];
    const observer = observeStorePublications((listener) => useImportStore.subscribe(listener));

    useImportStore.getState().patchItems(projectKey, [{ ...item(0), progress: { current: 2, total: 100, label: "Synthetic progress" } }]);

    const after = useImportStore.getState().itemById;
    observer.stop();
    expect(observer.publications).toBe(1);
    expect(Object.keys(after)).toHaveLength(10_000);
    expect(after).not.toBe(before);
    expect(after["item-1"]).toBe(unchanged);
    expect(useImportStore.getState().session?.items.length).toBeLessThanOrEqual(600);
  });

  it("keeps Queue commits and mounted rows bounded while reaching the 10k tail", async () => {
    const items = session(10_000).items;
    const commits = createReactCommitObserver();
    const rendered = render(queue(items, commits));
    expect(screen.getAllByRole("option").length).toBeLessThanOrEqual(80);

    const updated = [...items];
    updated[0] = { ...updated[0], progress: { current: 2, total: 100, label: "Synthetic progress" } };
    rendered.rerender(queue(updated, commits));
    expect(commits.commits).toBe(2);
    expect(statusRenders.get("item-0")).toBe(2);
    expect(statusRenders.get("item-1")).toBe(1);

    fireEvent.scroll(screen.getByRole("listbox", { name: "Sources" }), {
      target: { scrollTop: 9_999 * 72 },
    });
    expect(await screen.findByTestId("import-item-item-9999")).toBeInTheDocument();
    expect(screen.getAllByRole("option").length).toBeLessThanOrEqual(80);
    expect(commits.commits).toBeLessThanOrEqual(4);
  }, 300_000);
});
