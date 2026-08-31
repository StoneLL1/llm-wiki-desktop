import process from "node:process";
import { performance } from "node:perf_hooks";

const args = new Map();
for (let index = 2; index < process.argv.length; index += 2) {
  args.set(process.argv[index], process.argv[index + 1]);
}

const delay = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

class CdpClient {
  constructor(socket) {
    this.socket = socket;
    this.nextId = 0;
    this.pending = new Map();
    socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      const waiter = this.pending.get(message.id);
      if (!waiter) return;
      this.pending.delete(message.id);
      clearTimeout(waiter.timer);
      if (message.error) waiter.reject(new Error(message.error.message));
      else waiter.resolve(message.result);
    });
  }

  static async connect(endpoint) {
    const deadline = performance.now() + 20_000;
    while (performance.now() < deadline) {
      try {
        const targets = await fetch(`${endpoint}/json`, {
          signal: AbortSignal.timeout(2_000),
        }).then((response) => response.json());
        const target = targets.find((candidate) => candidate.type === "page" && candidate.url !== "about:blank");
        if (target) {
          const socket = new WebSocket(target.webSocketDebuggerUrl);
          await new Promise((resolve, reject) => {
            const timer = setTimeout(() => reject(new Error("CDP socket open timed out.")), 5_000);
            socket.addEventListener("open", () => {
              clearTimeout(timer);
              resolve();
            }, { once: true });
            socket.addEventListener("error", reject, { once: true });
          });
          return new CdpClient(socket);
        }
      } catch {
        // The packaged WebView can advertise its target before navigation.
      }
      await delay(50);
    }
    throw new Error(`No packaged WebView2 page was available at ${endpoint}.`);
  }

  command(method, params = {}, timeoutMs = 60_000) {
    return new Promise((resolve, reject) => {
      const id = ++this.nextId;
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`${method} timed out.`));
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timer });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }

  async evaluate(expression, timeoutMs = 60_000) {
    const response = await this.command("Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
    }, timeoutMs);
    if (response.exceptionDetails) {
      throw new Error(response.exceptionDetails.exception?.description ?? response.exceptionDetails.text);
    }
    return response.result.value;
  }

  close() {
    this.socket.close();
  }
}

function summarize(values) {
  if (values.length === 0) return { count: 0, min: null, max: null };
  return { count: values.length, min: Math.min(...values), max: Math.max(...values) };
}

function median(values) {
  if (values.length === 0) return null;
  const ordered = [...values].sort((left, right) => left - right);
  const middle = Math.floor(ordered.length / 2);
  return ordered.length % 2 === 0 ? (ordered[middle - 1] + ordered[middle]) / 2 : ordered[middle];
}

function percentile(values, quantile) {
  if (values.length === 0) return null;
  const ordered = [...values].sort((left, right) => left - right);
  return ordered[Math.min(ordered.length - 1, Math.floor(ordered.length * quantile))];
}

const clickRoute = (view) => `(async () => {
  const button = document.querySelector('button[data-app-view=${JSON.stringify(view)}]');
  if (!button) throw new Error('Missing route: ${view}');
  const startedAt = performance.now();
  button.click();
  const deadline = startedAt + 15000;
  while (button.getAttribute('aria-current') !== 'page') {
    if (performance.now() > deadline) throw new Error('Route timed out: ${view}');
    await new Promise(requestAnimationFrame);
  }
  await new Promise(requestAnimationFrame);
  return performance.now() - startedAt;
})()`;

async function main() {
  const endpoint = args.get("--endpoint") ?? "http://127.0.0.1:9223";
  const durationMs = Number(args.get("--duration-ms") ?? "30000");
  const expectedItems = Number(args.get("--expected-items") ?? "10000");
  const minimumProgressHz = Number(args.get("--min-progress-hz") ?? "8");
  const maximumProgressHz = Number(args.get("--max-progress-hz") ?? "12");
  const minimumTransitionsPerSecond = Number(args.get("--min-transitions-per-second") ?? "4");
  const syntheticBackendProgress = args.get("--synthetic-backend-progress") === "yes";
  const requireActiveImport = args.get("--require-active-import") === "yes";
  const windowDragObserved = args.get("--window-drag-observed") === "yes";
  if (!Number.isFinite(durationMs) || durationMs < 10_000) {
    throw new Error("--duration-ms must be at least 10000.");
  }
  if (!Number.isInteger(expectedItems) || expectedItems < 1) {
    throw new Error("--expected-items must be a positive integer.");
  }

  const client = await CdpClient.connect(endpoint);
  try {
    await client.evaluate(clickRoute("import"));
    await client.evaluate(`(async () => {
      const deadline = performance.now() + 15000;
      while (!document.querySelector('.import-v2-queue')) {
        if (performance.now() > deadline) throw new Error('Import Queue mount timed out.');
        await new Promise(requestAnimationFrame);
      }
    })()`);
    const fixture = await client.evaluate(`(async () => {
      const queue = document.querySelector('.import-v2-queue');
      if (!queue) throw new Error('Import Queue is not mounted.');
      const allFilter = queue.querySelector('.import-v2-queue__filter');
      const match = allFilter?.textContent?.match(/([0-9][0-9,._ ]*)$/);
      const itemCount = match ? Number(match[1].replace(/[^0-9]/g, '')) : null;
      const initialRows = queue.querySelectorAll('.import-v2-queue__row').length;
      const metrics = window.__batch0Import = {
        longTasks: [],
        rafGaps: [],
        mutations: 0,
        currentRows: initialRows,
        maxRows: initialRows,
        stopped: false,
        syntheticEmits: [],
        syntheticPaintLatencies: [],
        syntheticError: null,
        syntheticPending: 0,
        syntheticCompleted: 0,
        syntheticFailed: 0,
        activeImportAtStart: false,
        activeImportAtEnd: false,
        activeImportStatusAtStart: null,
        activeImportStatusAtEnd: null,
      };
      const longTaskObserver = new PerformanceObserver((list) => {
        for (const entry of list.getEntries()) window.__batch0Import.longTasks.push(entry.duration);
      });
      longTaskObserver.observe({ type: 'longtask', buffered: false });
      const list = queue.querySelector('.import-v2-queue__list');
      const countRows = (nodes) => Array.from(nodes).reduce((count, node) => {
        if (!(node instanceof Element)) return count;
        return count + (node.matches('.import-v2-queue__row') ? 1 : 0)
          + node.querySelectorAll('.import-v2-queue__row').length;
      }, 0);
      const mutationObserver = new MutationObserver((records) => {
        metrics.mutations += records.length;
        for (const record of records) {
          if (record.type !== 'childList') continue;
          metrics.currentRows += countRows(record.addedNodes) - countRows(record.removedNodes);
        }
        metrics.maxRows = Math.max(metrics.maxRows, metrics.currentRows);
      });
      mutationObserver.observe(list, { childList: true, subtree: true, characterData: true });
      let previous = performance.now();
      const tick = (now) => {
        const gap = now - previous;
        if (gap >= 50) metrics.rafGaps.push(gap);
        previous = now;
        if (!metrics.stopped) metrics.rafHandle = requestAnimationFrame(tick);
      };
      metrics.rafHandle = requestAnimationFrame(tick);
      if (${syntheticBackendProgress}) {
        const invoke = window.__TAURI_INTERNALS__?.invoke;
        if (!invoke) throw new Error('Packaged Tauri invoke bridge is unavailable.');
        const recent = await invoke('list_recent_projects');
        const project = recent[0];
        if (!project) throw new Error('Synthetic progress requires an active fixture project.');
        const listProjectTasks = () => invoke('list_tasks', {
          request: { projectId: project.projectId, projectRootPath: project.rootPath, statusFilter: null },
        });
        const isActiveImport = (task) => task?.taskType === 'import'
          && ['queued', 'running', 'cancelling'].includes(task.status);
        metrics.isActiveImport = isActiveImport;
        const initialTasks = await listProjectTasks();
        const activeImportTask = initialTasks.find(isActiveImport);
        metrics.activeImportTaskId = activeImportTask?.id ?? null;
        metrics.activeImportAtStart = Boolean(activeImportTask);
        metrics.activeImportStatusAtStart = activeImportTask?.status ?? null;
        metrics.readActiveImportTask = () => metrics.activeImportTaskId
          ? invoke('get_task', {
            request: {
              taskId: metrics.activeImportTaskId,
              projectId: project.projectId,
              projectRootPath: project.rootPath,
            },
          })
          : Promise.resolve(null);
        const appInitialHref = Array.from(document.querySelectorAll('link[rel="modulepreload"][href]'))
          .map((link) => link.href)
          .find((href) => href.includes('/assets/app-initial-'));
        if (!appInitialHref) throw new Error('Packaged app-initial module was not discoverable.');
        const appInitial = await import(appInitialHref);
        const listen = appInitial._;
        if (typeof listen !== 'function') throw new Error('Packaged Tauri event listener export is unavailable.');
        metrics.syntheticUnlisten = await listen('task://updated', (event) => {
          const backendEvent = event?.payload;
          const task = backendEvent?.payload;
          if (backendEvent?.projectId !== project.projectId || task?.taskType !== 'project_inventory') return;
          const receivedAt = performance.now();
          requestAnimationFrame(() => metrics.syntheticPaintLatencies.push(performance.now() - receivedAt));
        });
        const replay = () => {
          if (metrics.stopped) return;
          const startedAt = performance.now();
          metrics.syntheticEmits.push(startedAt);
          metrics.syntheticPending += 1;
          void invoke('start_project_inventory', {
              request: { projectId: project.projectId, projectRootPath: project.rootPath },
            })
            .then(() => { metrics.syntheticCompleted += 1; })
            .catch((error) => {
              metrics.syntheticFailed += 1;
              metrics.syntheticError = String(error);
            })
            .finally(() => { metrics.syntheticPending -= 1; });
        };
        replay();
        metrics.syntheticTimer = setInterval(replay, 100);
      }
      metrics.stopScheduling = () => {
        metrics.stopped = true;
        clearInterval(metrics.syntheticTimer);
      };
      metrics.finishObservers = () => {
        void metrics.syntheticUnlisten?.();
        cancelAnimationFrame(metrics.rafHandle);
        mutationObserver.disconnect();
        longTaskObserver.disconnect();
      };
      return { itemCount, initialRows };
    })()`);
    if (fixture.itemCount !== expectedItems) {
      throw new Error(`Expected ${expectedItems} Import items; observed ${fixture.itemCount}.`);
    }

    const progressTransitions = [];
    let previousProgress = null;
    const routeDurations = [];
    const deadline = performance.now() + durationMs;
    let route = "import";
    let sampleCount = 0;
    while (performance.now() < deadline) {
      const progress = await client.evaluate("document.querySelector('.import-v2-queue__header')?.innerText ?? ''");
      sampleCount += 1;
      if (progress && progress !== previousProgress) {
        progressTransitions.push({ at: performance.now(), value: progress });
        previousProgress = progress;
      }
      if (sampleCount % 20 === 0) {
        route = route === "import" ? "wiki" : "import";
        routeDurations.push(await client.evaluate(clickRoute(route)));
      }
      await delay(50);
    }
    if (route !== "import") routeDurations.push(await client.evaluate(clickRoute("import")));

    const diagnostics = await client.evaluate(`(async () => {
      const metrics = window.__batch0Import;
      metrics.stopScheduling();
      const pendingDeadline = performance.now() + 10000;
      while (metrics.syntheticPending > 0 && performance.now() < pendingDeadline) {
        await new Promise((resolve) => setTimeout(resolve, 20));
      }
      if (metrics.readActiveImportTask) {
        const activeImportTaskAtEnd = await metrics.readActiveImportTask();
        metrics.activeImportAtEnd = metrics.isActiveImport(activeImportTaskAtEnd);
        metrics.activeImportStatusAtEnd = activeImportTaskAtEnd?.status ?? null;
      }
      await new Promise(requestAnimationFrame);
      await new Promise(requestAnimationFrame);
      metrics.finishObservers();
      return {
        longTasks: metrics.longTasks,
        rafGaps: metrics.rafGaps,
        mutations: metrics.mutations,
        maxRows: metrics.maxRows,
        finalRows: document.querySelectorAll('.import-v2-queue__row').length,
        syntheticEmits: metrics.syntheticEmits,
        syntheticPaintLatencies: metrics.syntheticPaintLatencies,
        syntheticError: metrics.syntheticError,
        syntheticPending: metrics.syntheticPending,
        syntheticCompleted: metrics.syntheticCompleted,
        syntheticFailed: metrics.syntheticFailed,
        activeImportAtStart: metrics.activeImportAtStart,
        activeImportAtEnd: metrics.activeImportAtEnd,
        activeImportStatusAtStart: metrics.activeImportStatusAtStart,
        activeImportStatusAtEnd: metrics.activeImportStatusAtEnd,
      };
    })()`);
    const transitionIntervals = progressTransitions
      .slice(1)
      .map((entry, index) => entry.at - progressTransitions[index].at)
      .filter((interval) => interval <= 500);
    const syntheticIntervals = diagnostics.syntheticEmits
      .slice(1)
      .map((entry, index) => entry - diagnostics.syntheticEmits[index]);
    const cadenceIntervals = syntheticBackendProgress ? syntheticIntervals : transitionIntervals;
    const medianIntervalMs = median(cadenceIntervals);
    const observedHz = medianIntervalMs === null ? null : 1000 / medianIntervalMs;
    const minimumVisibleTransitions = Math.floor((durationMs / 1000) * minimumTransitionsPerSecond);
    const observedTransitions = syntheticBackendProgress ? diagnostics.syntheticEmits.length : progressTransitions.length;
    const cadenceVerified = observedTransitions >= minimumVisibleTransitions
      && observedHz !== null
      && observedHz >= minimumProgressHz
      && observedHz <= maximumProgressHz;
    const taskEventP95 = percentile(diagnostics.syntheticPaintLatencies, 0.95);
    const eventBudgetVerified = !syntheticBackendProgress || (
      diagnostics.syntheticPaintLatencies.length >= minimumVisibleTransitions
      && taskEventP95 !== null
      && taskEventP95 <= 100
    );
    const backendReplayVerified = !syntheticBackendProgress || (
      diagnostics.syntheticError === null
      && diagnostics.syntheticFailed === 0
      && diagnostics.syntheticPending === 0
      && diagnostics.syntheticCompleted >= minimumVisibleTransitions
    );
    const activeImportVerified = !requireActiveImport
      || (diagnostics.activeImportAtStart && diagnostics.activeImportAtEnd);
    const domBudgetVerified = diagnostics.maxRows <= 200;
    const longTaskBudgetVerified = diagnostics.longTasks.length === 0;
    const routeBudgetVerified = routeDurations.length > 0 && Math.max(...routeDurations) <= 150;
    const completed = windowDragObserved
      && cadenceVerified
      && eventBudgetVerified
      && backendReplayVerified
      && activeImportVerified
      && domBudgetVerified
      && longTaskBudgetVerified
      && routeBudgetVerified;
    return {
      schemaVersion: 1,
      scenario: "window drag + route switch + 10 Hz progress + 10k session",
      status: completed ? "completed" : "incomplete",
      fixture: { expectedItems, observedItems: fixture.itemCount },
      operatorObservation: {
        nativeWindowDraggedForScenario: windowDragObserved,
        note: "Pass --window-drag-observed yes only after continuously dragging the native packaged window during sampling.",
      },
      durationMs,
      progress: {
        sampleIntervalMs: 50,
        samples: sampleCount,
        transitions: progressTransitions.length,
        minimumVisibleTransitions,
        medianTransitionIntervalMs: medianIntervalMs,
        observedHz,
        expectedHz: { minimum: minimumProgressHz, maximum: maximumProgressHz },
        cadenceVerified,
        source: syntheticBackendProgress ? "10 Hz packaged TaskService inventory replay" : "Import Queue header",
      },
      taskEventToNextPaintMs: {
        ...summarize(diagnostics.syntheticPaintLatencies),
        p95: taskEventP95,
        budgetVerified: eventBudgetVerified,
      },
      syntheticBackendProgressError: diagnostics.syntheticError,
      backendReplay: {
        completed: diagnostics.syntheticCompleted,
        failed: diagnostics.syntheticFailed,
        pending: diagnostics.syntheticPending,
        budgetVerified: backendReplayVerified,
      },
      activeImport: {
        required: requireActiveImport,
        atStart: diagnostics.activeImportAtStart,
        atEnd: diagnostics.activeImportAtEnd,
        statusAtStart: diagnostics.activeImportStatusAtStart,
        statusAtEnd: diagnostics.activeImportStatusAtEnd,
        budgetVerified: activeImportVerified,
      },
      routes: summarize(routeDurations),
      routeBudgetVerified,
      dom: { initialRows: fixture.initialRows, maxRows: diagnostics.maxRows, finalRows: diagnostics.finalRows, mutations: diagnostics.mutations, budgetVerified: domBudgetVerified },
      longTasksOver50Ms: { ...summarize(diagnostics.longTasks), budgetVerified: longTaskBudgetVerified },
      animationFrameGapsOver50Ms: summarize(diagnostics.rafGaps),
      privacy: "Only counts, durations, and operator attestation are emitted; project paths, labels, and content are omitted.",
    };
  } finally {
    client.close();
  }
}

let result;
try {
  result = await main();
} catch (error) {
  process.exitCode = 1;
  result = { schemaVersion: 1, status: "failed", error: error instanceof Error ? error.message : String(error) };
}
process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
