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

const SCALE = [100, 1_000, 10_000] as const;
const projectKey = importProjectKey("perf-project", "D:/fixture-project");

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
      counts={{ all: items.length, active: items.length, ready: 0, needsAction: 0, failed: 0, completed: 0 }}
      progress={{ completed: 0, total: items.length, active: items.length }}
      selectedItemId={null}
      filter="all"
      onFilterChange={vi.fn()}
      onSelectItem={vi.fn()}
      onSetItemSelected={vi.fn()}
      onAction={vi.fn()}
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

  it("observes a full 10k item-array replacement for one changed Import item", () => {
    useImportStore.getState().attachSession(projectKey, session(10_000));
    const before = useImportStore.getState().session?.items;
    const observer = observeStorePublications((listener) => useImportStore.subscribe(listener));

    useImportStore.getState().patchItems(projectKey, [{ ...item(0), progress: { current: 2, total: 100, label: "Synthetic progress" } }]);

    const after = useImportStore.getState().session?.items;
    observer.stop();
    expect(observer.publications).toBe(1);
    expect(after).toHaveLength(10_000);
    expect(after).not.toBe(before);
    expect(after?.[1]).toBe(before?.[1]);
  });

  it("observes Queue commits and the current cumulative 10k mounted-row growth", () => {
    const items = session(10_000).items;
    const commits = createReactCommitObserver();
    const rendered = render(queue(items, commits));
    expect(screen.getAllByRole("listitem")).toHaveLength(200);

    const updated = [...items];
    updated[0] = { ...updated[0], progress: { current: 2, total: 100, label: "Synthetic progress" } };
    rendered.rerender(queue(updated, commits));
    expect(commits.commits).toBe(2);

    for (let page = 1; page < 50; page += 1) {
      fireEvent.click(screen.getByRole("button", { name: /load more/i }));
    }
    expect(screen.getAllByRole("listitem")).toHaveLength(10_000);
    expect(commits.commits).toBe(99);
  }, 300_000);
});
