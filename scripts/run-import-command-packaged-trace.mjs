import process from "node:process";
import { existsSync, readFileSync } from "node:fs";
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

  command(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = ++this.nextId;
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`${method} timed out.`));
      }, 20_000);
      this.pending.set(id, { resolve, reject, timer });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }

  async evaluate(expression) {
    const response = await this.command("Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (response.exceptionDetails) {
      throw new Error(response.exceptionDetails.exception?.description ?? response.exceptionDetails.text);
    }
    return response.result.value;
  }

  close() {
    this.socket.close();
  }
}

async function main() {
  const endpoint = args.get("--endpoint") ?? "http://127.0.0.1:9224";
  const projectRoot = args.get("--project-root");
  const tracePath = args.get("--trace-path");
  if (!projectRoot) throw new Error("--project-root is required.");
  if (!tracePath) throw new Error("--trace-path is required.");
  if (existsSync(projectRoot)) throw new Error("--project-root must not already exist.");
  if (existsSync(tracePath)) throw new Error("--trace-path must not already exist.");

  const client = await CdpClient.connect(endpoint);
  try {
    const result = await client.evaluate(`(async () => {
      let observer = null;
      let sampling = false;
      let frameHandle = 0;
      try {
      const invoke = window.__TAURI_INTERNALS__?.invoke;
      if (typeof invoke !== 'function') throw new Error('Tauri invoke is unavailable.');
      const longTasks = [];
      const frameGaps = [];
      observer = new PerformanceObserver((list) => {
        for (const entry of list.getEntries()) longTasks.push(entry.duration);
      });
      observer.observe({ type: 'longtask', buffered: false });
      let previousFrame = performance.now();
      sampling = true;
      const sampleFrame = (now) => {
        frameGaps.push(now - previousFrame);
        previousFrame = now;
        if (sampling) frameHandle = requestAnimationFrame(sampleFrame);
      };
      frameHandle = requestAnimationFrame(sampleFrame);
      const measure = async (name, request) => {
        const startedAt = performance.now();
        const value = await invoke(name, { request });
        return { value, elapsedMs: performance.now() - startedAt };
      };
      const measureExpectedError = async (name, request) => {
        const startedAt = performance.now();
        let observedError = null;
        try {
          await invoke(name, { request });
        } catch (error) {
          observedError = { code: error?.code ?? null, message: error?.message ?? String(error) };
        }
        if (!observedError) throw new Error(name + ' unexpectedly succeeded.');
        return { error: observedError, elapsedMs: performance.now() - startedAt };
      };
      const measureOutcome = async (name, request) => {
        const startedAt = performance.now();
        try {
          await invoke(name, { request });
          return { error: null, elapsedMs: performance.now() - startedAt };
        } catch (error) {
          return {
            error: { code: error?.code ?? null, message: error?.message ?? String(error) },
            elapsedMs: performance.now() - startedAt,
          };
        }
      };
      const opened = await invoke('create_project', {
        request: { rootPath: ${JSON.stringify(projectRoot)}, name: 'Batch 1 trace fixture', template: 'general' },
      });
      const project = opened.summary;
      const scope = { projectId: project.projectId, projectRootPath: project.rootPath };
      const readiness = await measure('get_import_frontend_readiness_v2', scope);
      const created = await measure('create_import_session_v2', { ...scope, resourceMode: 'balanced' });
      const session = await measure('get_import_session_v2', { ...scope, sessionId: created.value.sessionId });
      const history = await measure('list_import_history_v2', { ...scope, cursor: null, limit: 20 });
      const historySession = await measure('get_import_history_session_v2', {
        ...scope,
        sessionId: created.value.sessionId,
        historyBatchId: null,
      });
      const preview = await measureExpectedError('get_import_preview_content_v2', {
        ...scope,
        sessionId: created.value.sessionId,
        itemId: 'missing-item',
        candidateId: null,
        historyBatchId: null,
      });
      const resolution = await measureExpectedError('set_import_item_resolution_v2', {
        ...scope,
        sessionId: created.value.sessionId,
        itemId: 'missing-item',
        resolution: { kind: 'new_source' },
      });
      const asr = await measureExpectedError('authorize_local_asr_v2', {
        ...scope,
        sessionId: created.value.sessionId,
        itemId: 'missing-item',
      });
      const confirm = await measureOutcome('confirm_import_session_v2', {
        ...scope,
        sessionId: created.value.sessionId,
        batchTaskId: null,
        acknowledgeRestrictedContent: false,
        decisions: [],
      });
      await new Promise(requestAnimationFrame);
      sampling = false;
      cancelAnimationFrame(frameHandle);
      observer.disconnect();
      return {
        commands: [
          { name: 'get_import_frontend_readiness_v2', elapsedMs: readiness.elapsedMs },
          { name: 'create_import_session_v2', elapsedMs: created.elapsedMs },
          { name: 'get_import_session_v2', elapsedMs: session.elapsedMs },
          { name: 'list_import_history_v2', elapsedMs: history.elapsedMs },
          { name: 'get_import_history_session_v2', elapsedMs: historySession.elapsedMs },
          { name: 'get_import_preview_content_v2', elapsedMs: preview.elapsedMs, errorCode: preview.error.code },
          { name: 'set_import_item_resolution_v2', elapsedMs: resolution.elapsedMs, errorCode: resolution.error.code },
          { name: 'authorize_local_asr_v2', elapsedMs: asr.elapsedMs, errorCode: asr.error.code },
          { name: 'confirm_import_session_v2', elapsedMs: confirm.elapsedMs, errorCode: confirm.error?.code ?? null },
        ],
        webview: {
          frameSamples: frameGaps.length,
          maxFrameGapMs: frameGaps.length === 0 ? null : Math.max(...frameGaps),
          longTaskCount: longTasks.length,
          maxLongTaskMs: longTasks.length === 0 ? null : Math.max(...longTasks),
        },
      };
      } catch (error) {
        sampling = false;
        cancelAnimationFrame(frameHandle);
        observer?.disconnect();
        return { error: { code: error?.code ?? null, message: error?.message ?? String(error) } };
      }
    })()`);
    if (result?.error) {
      throw new Error(`Packaged invoke failed: ${JSON.stringify(result.error)}`);
    }
    const traceText = readFileSync(tracePath, "utf8").trim();
    const spans = traceText.length === 0
      ? []
      : traceText.split(/\r?\n/u).map((line) => JSON.parse(line));
    const allowedTraceKeys = [
      "callerThread",
      "class",
      "errorCode",
      "operation",
      "outcome",
      "queueWaitNanos",
      "runNanos",
      "workerThread",
    ];
    for (const span of spans) {
      const actualKeys = Object.keys(span).sort();
      if (JSON.stringify(actualKeys) !== JSON.stringify(allowedTraceKeys)) {
        throw new Error(`Blocking trace contains unexpected fields: ${actualKeys.join(", ")}`);
      }
    }
    const heavyIoSpans = spans.filter((span) => span.class === "heavy_io");
    if (heavyIoSpans.length < result.commands.length) {
      throw new Error(
        `Expected at least ${result.commands.length} heavy-I/O spans, observed ${heavyIoSpans.length}.`,
      );
    }
    if (heavyIoSpans.some((span) => span.callerThread === span.workerThread)) {
      throw new Error("A heavy-I/O span ran on its caller thread.");
    }
    if (traceText.includes(projectRoot)) {
      throw new Error("Blocking trace leaked the disposable project path.");
    }
    console.log(JSON.stringify({
      status: "completed",
      ...result,
      blockingTrace: {
        spanCount: spans.length,
        heavyIoSpanCount: heavyIoSpans.length,
        allHeavyIoSpansUsedDistinctCallerAndWorkerThreads: true,
        fieldAllowlistVerified: true,
        containsDisposableProjectPath: false,
      },
    }, null, 2));
  } finally {
    client.close();
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});
