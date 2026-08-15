import { spawn } from "node:child_process";
import { mkdir, mkdtemp } from "node:fs/promises";
import { createServer } from "node:http";
import path from "node:path";
import process from "node:process";
import { performance } from "node:perf_hooks";

const args = new Map();
for (let index = 2; index < process.argv.length; index += 2) {
  args.set(process.argv[index], process.argv[index + 1]);
}

const delay = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

function errorMessage(error) {
  return error instanceof Error ? error.message : String(error);
}

function failureCode(error) {
  const message = errorMessage(error);
  if (/timed out/i.test(message)) return "BENCHMARK_TIMEOUT";
  if (/CDP|socket|WebView2/i.test(message)) return "BENCHMARK_CDP_FAILED";
  if (/spawn|ENOENT|EACCES|EPERM/i.test(message)) return "BENCHMARK_PROCESS_START_FAILED";
  if (/fixture|synthetic/i.test(message)) return "BENCHMARK_FIXTURE_INVALID";
  return "BENCHMARK_ATTEMPT_FAILED";
}

function failedAttempt(error) {
  process.exitCode = 1;
  const result = { status: "failed", errorCode: failureCode(error) };
  if (args.get("--output-detail") === "raw") result.rawError = errorMessage(error);
  return result;
}

function nearestRank(values, percentile) {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.max(0, Math.ceil(percentile * sorted.length) - 1)];
}

function summary(values) {
  if (values.length === 0) return { n: 0, min: null, p50: null, p95: null, max: null };
  const result = {
    n: values.length,
    min: Math.min(...values),
    p50: nearestRank(values, 0.5),
    p95: nearestRank(values, 0.95),
    max: Math.max(...values),
  };
  if (args.get("--output-detail") === "raw") result.raw = values;
  return result;
}

class CdpClient {
  constructor(socket) {
    this.socket = socket;
    this.nextId = 0;
    this.pending = new Map();
    this.closed = false;
    socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      if (!message.id) return;
      const waiter = this.pending.get(message.id);
      if (!waiter) return;
      this.pending.delete(message.id);
      if (message.error) waiter.reject(new Error(message.error.message));
      else waiter.resolve(message.result);
    });
    const rejectPending = () => {
      this.closed = true;
      for (const { reject, timer } of this.pending.values()) {
        clearTimeout(timer);
        reject(new Error("CDP socket closed before the command completed."));
      }
      this.pending.clear();
    };
    socket.addEventListener("close", rejectPending, { once: true });
    socket.addEventListener("error", rejectPending, { once: true });
  }

  static async connect(endpoint, timeoutMs = 20_000) {
    const deadline = performance.now() + timeoutMs;
    let lastError;
    while (performance.now() < deadline) {
      try {
        const fetchTimeoutMs = Math.max(1, Math.min(2_000, deadline - performance.now()));
        const targets = await fetch(`${endpoint}/json`, { signal: AbortSignal.timeout(fetchTimeoutMs) }).then((response) => response.json());
        const target = targets.find((candidate) => candidate.type === "page" && candidate.url !== "about:blank");
        if (target) {
          const socket = new WebSocket(target.webSocketDebuggerUrl);
          let openTimer;
          try {
            await new Promise((resolve, reject) => {
              const openTimeoutMs = Math.max(1, Math.min(5_000, deadline - performance.now()));
              openTimer = setTimeout(() => reject(new Error("Timed out opening the CDP socket.")), openTimeoutMs);
              socket.addEventListener("open", resolve, { once: true });
              socket.addEventListener("error", reject, { once: true });
            });
          } catch (error) {
            socket.close();
            throw error;
          } finally {
            clearTimeout(openTimer);
          }
          return new CdpClient(socket);
        }
      } catch (error) {
        lastError = error;
      }
      await delay(25);
    }
    throw new Error(`Timed out waiting for WebView2 CDP endpoint ${endpoint}: ${lastError ?? "no page"}`);
  }

  command(method, params = {}, timeoutMs = 20_000) {
    return new Promise((resolve, reject) => {
      if (this.closed || this.socket.readyState !== WebSocket.OPEN) {
        reject(new Error(`Cannot run ${method}: CDP socket is not open.`));
        return;
      }
      const id = ++this.nextId;
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`Timed out after ${timeoutMs} ms running CDP command ${method}.`));
      }, timeoutMs);
      this.pending.set(id, {
        resolve: (value) => {
          clearTimeout(timer);
          resolve(value);
        },
        reject: (error) => {
          clearTimeout(timer);
          reject(error);
        },
        timer,
      });
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
    if (!this.closed) this.socket.close();
  }
}

const clickRouteExpression = (label) => `(async () => {
  const button = document.querySelector('button[aria-label=${JSON.stringify(label)}]');
  if (!button) throw new Error('Missing route button: ${label}');
  const startedAt = performance.now();
  const deadline = startedAt + 15000;
  let loadingObserved = false;
  button.click();
  while (button.getAttribute('aria-current') !== 'page') {
    loadingObserved ||= /(^|\\n)Loading(…|\\.\\.\\.)?(\\n|$)/.test(document.body.innerText);
    if (performance.now() > deadline) throw new Error('Route did not become current: ${label}');
    await new Promise(requestAnimationFrame);
  }
  await new Promise(requestAnimationFrame);
  await new Promise(requestAnimationFrame);
  loadingObserved ||= /(^|\\n)Loading(…|\\.\\.\\.)?(\\n|$)/.test(document.body.innerText);
  return { durationMs: performance.now() - startedAt, loadingObserved };
})()`;

async function waitUntilInteractive(client, expectedProjectName = "fixture-project") {
  const startedAt = performance.now();
  while (performance.now() - startedAt < 20_000) {
    let ready = false;
    try {
      ready = await client.evaluate(`Boolean(document.body?.innerText.includes(${JSON.stringify(expectedProjectName)})) && Boolean(document.querySelector('button[aria-current="page"]'))`);
    } catch {
      // The first CDP target can be observable before its initial navigation commits.
    }
    if (ready) return;
    await delay(25);
  }
  throw new Error(`Packaged shell did not become interactive for ${expectedProjectName}.`);
}

async function runRoutesAndSplitters(endpoint) {
  const splitterRepetitions = Number(args.get("--splitter-repetitions") ?? "1");
  if (!Number.isInteger(splitterRepetitions) || splitterRepetitions < 1) {
    throw new Error("--splitter-repetitions must be a positive integer.");
  }
  const client = await CdpClient.connect(endpoint);
  try {
  await client.command("Page.reload", { ignoreCache: false });
  await delay(250);
  await waitUntilInteractive(client);
  await client.evaluate(`(() => {
    window.__batch6LongTasks = [];
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) window.__batch6LongTasks.push(entry.duration);
    }).observe({ type: 'longtask', buffered: false });
    window.__batch6StorageWrites = 0;
    const originalSetItem = Storage.prototype.setItem;
    Storage.prototype.setItem = function (...args) {
      window.__batch6StorageWrites += 1;
      return originalSetItem.apply(this, args);
    };
  })()`);

  const routeLabels = ["Wiki", "Chat", "Graph", "Exports", "Lint"];
  for (const label of routeLabels) await client.evaluate(clickRouteExpression(label));
  await client.evaluate(clickRouteExpression("Wiki"));

  const routeSamples = [];
  const routeAttempts = [];
  for (let repetition = 0; repetition < 20; repetition += 1) {
    for (const label of ["Chat", "Graph", "Exports", "Lint", "Wiki"]) {
      try {
        const sample = await client.evaluate(clickRouteExpression(label));
        routeSamples.push({ label, ...sample });
        routeAttempts.push({ repetition: repetition + 1, label, status: "completed" });
      } catch (error) {
        routeAttempts.push({ repetition: repetition + 1, label, ...failedAttempt(error) });
      }
    }
  }

  const splitters = [
    { route: "Dashboard", label: "Resize sidebar", deltaX: 40 },
    { route: "Dashboard", label: "Resize context panel", deltaX: -40 },
    { route: "Wiki", label: "Resize wiki tree", deltaX: 40 },
    { route: "Exports", label: "Resize export list", deltaX: 40 },
    { route: "Lint", label: "Resize lint issue details", deltaX: 40 },
  ];
  const splitterResults = [];
  for (const splitter of splitters) {
    const allInputToNextRaf = [];
    const allStorageWrites = [];
    const allValueChanges = [];
    const attempts = [];
    for (let repetition = 0; repetition < splitterRepetitions; repetition += 1) {
      try {
        await client.evaluate(clickRouteExpression(splitter.route));
        const beforeWrites = await client.evaluate("window.__batch6StorageWrites");
        const rect = await client.evaluate(`(() => {
          const separator = document.querySelector('[role="separator"][aria-label=${JSON.stringify(splitter.label)}]');
          if (!separator) throw new Error('Missing splitter: ${splitter.label}');
          return {
            ...separator.getBoundingClientRect().toJSON(),
            value: Number(separator.getAttribute('aria-valuenow')),
            min: Number(separator.getAttribute('aria-valuemin')),
            max: Number(separator.getAttribute('aria-valuemax')),
          };
        })()`);
        const startX = rect.x + rect.width / 2;
        const startY = rect.y + rect.height / 2;
        const targetValue = rect.value <= (rect.min + rect.max) / 2 ? rect.max - 20 : rect.min + 20;
        const dragDelta = targetValue - rect.value;
        await client.command("Input.dispatchMouseEvent", { type: "mousePressed", x: startX, y: startY, button: "left", buttons: 1, clickCount: 1 });
        const inputToNextRaf = [];
        for (let move = 0; move < 120; move += 1) {
          const moveStartedAt = performance.now();
          const x = startX + (move / 119) * dragDelta;
          await client.command("Input.dispatchMouseEvent", { type: "mouseMoved", x, y: startY, button: "none", buttons: 1 });
          await client.evaluate("new Promise((resolve) => requestAnimationFrame(() => resolve(true)))");
          inputToNextRaf.push(performance.now() - moveStartedAt);
          await delay(Math.max(0, 16.67 - inputToNextRaf.at(-1)));
        }
        await client.command("Input.dispatchMouseEvent", { type: "mouseReleased", x: startX + dragDelta, y: startY, button: "left", buttons: 0, clickCount: 1 });
        await client.evaluate("new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))");
        const storageWrites = await client.evaluate("window.__batch6StorageWrites") - beforeWrites;
        const finalValue = await client.evaluate(`Number(document.querySelector('[role="separator"][aria-label=${JSON.stringify(splitter.label)}]').getAttribute('aria-valuenow'))`);
        allInputToNextRaf.push(...inputToNextRaf);
        allStorageWrites.push(storageWrites);
        allValueChanges.push(Math.abs(finalValue - rect.value));
        attempts.push({ repetition: repetition + 1, status: "completed" });
      } catch (error) {
        attempts.push({ repetition: repetition + 1, ...failedAttempt(error) });
        try {
          await client.command("Input.dispatchMouseEvent", { type: "mouseReleased", x: 0, y: 0, button: "left", buttons: 0, clickCount: 1 });
        } catch {
          // The command failure that ended this attempt remains the primary evidence.
        }
      }
    }
    splitterResults.push({
      ...splitter,
      repetitions: splitterRepetitions,
      attempts,
      storageWrites: summary(allStorageWrites),
      writesNotExactlyOne: allStorageWrites.filter((writes) => writes !== 1).length,
      valueChangePx: summary(allValueChanges),
      valuesNotChanged: allValueChanges.filter((change) => change === 0).length,
      inputToNextRafCdpProxyMs: summary(allInputToNextRaf),
    });
  }

  let rawDiagnostics = { longTasksOver50Ms: [], bodyHasLoadingText: null };
  let diagnosticsAttempt = { status: "completed" };
  try {
    rawDiagnostics = await client.evaluate(`({
      longTasksOver50Ms: window.__batch6LongTasks.filter((duration) => duration > 50),
      bodyHasLoadingText: /(^|\\n)Loading(…|\\.\\.\\.)?(\\n|$)/.test(document.body.innerText),
    })`);
  } catch (error) {
    diagnosticsAttempt = failedAttempt(error);
  }
  const diagnostics = {
    attempt: diagnosticsAttempt,
    longTasksOver50Ms: summary(rawDiagnostics.longTasksOver50Ms),
    bodyHasLoadingText: rawDiagnostics.bodyHasLoadingText,
  };
  if (args.get("--output-detail") === "raw") {
    diagnostics.longTaskDurationsOver50Ms = rawDiagnostics.longTasksOver50Ms;
  }
  const byRoute = Object.fromEntries(routeLabels.map((label) => [
    label,
    summary(routeSamples.filter((sample) => sample.label === label).map((sample) => sample.durationMs)),
  ]));
  const routeFailureCount = routeAttempts.filter((attempt) => attempt.status === "failed").length;
  const splitterFailureCount = splitterResults.reduce((count, splitter) => count + splitter.attempts.filter((attempt) => attempt.status === "failed").length, 0);
  const diagnosticsFailureCount = diagnosticsAttempt.status === "failed" ? 1 : 0;
  return {
    status: routeFailureCount + splitterFailureCount + diagnosticsFailureCount === 0 ? "completed" : "failed",
    routeLoop: {
      repetitions: 20,
      metric: "click-to-current-route-plus-two-RAF CDP proxy; not route-specific interactive readiness",
      attempts: routeAttempts,
      failureCount: routeFailureCount,
      loadingObservedTransitions: routeSamples.filter((sample) => sample.loadingObserved).length,
      byRoute,
    },
    splitters: splitterResults,
    splitterFailureCount,
    diagnosticsFailureCount,
    diagnostics,
  };
  } finally {
    client.close();
  }
}

async function terminateProcessTree(child) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  if (process.platform === "win32") {
    const killer = spawn("taskkill", ["/pid", String(child.pid), "/t", "/f"], {
      stdio: "ignore",
      windowsHide: true,
    });
    await Promise.race([
      new Promise((resolve) => killer.once("exit", resolve)),
      delay(5_000),
    ]);
  } else {
    child.kill("SIGTERM");
  }
}

async function closeApp(client, child) {
  const exited = new Promise((resolve) => child.once("exit", resolve));
  try {
    await client.evaluate("window.__TAURI_INTERNALS__.invoke('plugin:window|close', { label: 'main' })");
  } catch {
    await terminateProcessTree(child);
  }
  await Promise.race([
    exited,
    delay(5_000).then(() => terminateProcessTree(child)),
  ]);
}

async function sampleStartup({ exe, appData, webviewData, port }) {
  await mkdir(webviewData, { recursive: true });
  const startedAt = performance.now();
  const child = spawn(exe, [], {
    env: {
      ...process.env,
      APPDATA: appData,
      WEBVIEW2_USER_DATA_FOLDER: webviewData,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: "ignore",
    windowsHide: true,
  });
  const spawnFailure = new Promise((_, reject) => {
    child.once("error", reject);
  });
  let client;
  try {
    client = await Promise.race([
      CdpClient.connect(`http://127.0.0.1:${port}`),
      spawnFailure,
    ]);
    await waitUntilInteractive(client);
    const interactiveMs = performance.now() - startedAt;
    const browserMetrics = await client.evaluate(`({
      navigation: performance.getEntriesByType('navigation')[0]?.toJSON(),
      paints: performance.getEntriesByType('paint').map((entry) => entry.toJSON()),
    })`);
    return { interactiveMs, browserMetrics };
  } finally {
    if (client) {
      await closeApp(client, child);
      client.close();
    } else {
      await terminateProcessTree(child);
    }
  }
}

async function runStartupSamples() {
  const exeArg = args.get("--exe");
  const appDataArg = args.get("--app-data");
  const webviewRootArg = args.get("--webview-root");
  if (!exeArg || !appDataArg || !webviewRootArg) {
    throw new Error("Startup mode requires --exe, --app-data, and --webview-root.");
  }
  const exe = path.resolve(exeArg);
  const appData = path.resolve(appDataArg);
  const webviewRoot = path.resolve(webviewRootArg);
  await mkdir(webviewRoot, { recursive: true });
  const repetitions = Number(args.get("--runs") ?? 10);
  if (!Number.isInteger(repetitions) || repetitions < 1) {
    throw new Error("--runs must be a positive integer.");
  }
  const freshProfile = [];
  const freshProfileAttempts = [];
  for (let index = 0; index < repetitions; index += 1) {
    try {
      const freshProfilePath = await mkdtemp(path.join(webviewRoot, `fresh-profile-${index + 1}-`));
      freshProfile.push(await sampleStartup({
        exe,
        appData,
        webviewData: freshProfilePath,
        port: 9300 + index,
      }));
      freshProfileAttempts.push({ repetition: index + 1, status: "completed" });
    } catch (error) {
      freshProfileAttempts.push({ repetition: index + 1, ...failedAttempt(error) });
    }
  }
  const warmProfile = path.join(webviewRoot, "warm");
  let warmPreparation = { status: "completed" };
  try {
    await sampleStartup({ exe, appData, webviewData: warmProfile, port: 9400 });
  } catch (error) {
    warmPreparation = failedAttempt(error);
  }
  const warm = [];
  const warmAttempts = [];
  for (let index = 0; warmPreparation.status === "completed" && index < repetitions; index += 1) {
    try {
      warm.push(await sampleStartup({ exe, appData, webviewData: warmProfile, port: 9401 + index }));
      warmAttempts.push({ repetition: index + 1, status: "completed" });
    } catch (error) {
      warmAttempts.push({ repetition: index + 1, ...failedAttempt(error) });
    }
  }
  const failureCount = [...freshProfileAttempts, ...warmAttempts, warmPreparation].filter((attempt) => attempt.status === "failed").length;
  const startupResult = {
    status: failureCount === 0 ? "completed" : "failed",
    freshProfile: summary(freshProfile.map((sample) => sample.interactiveMs)),
    warm: summary(warm.map((sample) => sample.interactiveMs)),
    attempts: { freshProfile: freshProfileAttempts, warmPreparation, warm: warmAttempts },
    failureCount,
  };
  if (args.get("--output-detail") === "raw") {
    startupResult.freshProfileBrowserMetrics = freshProfile.map((sample) => sample.browserMetrics);
    startupResult.warmBrowserMetrics = warm.map((sample) => sample.browserMetrics);
  }
  return startupResult;
}

function createChatFixture() {
  const pattern = "## Synthetic section\n中文段落 with `code` and $x^2$.\n\n";
  let text = "";
  while (Buffer.byteLength(text + pattern + "Z", "utf8") <= 262_144) text += pattern;
  text += "x".repeat(262_144 - Buffer.byteLength(text + "Z", "utf8")) + "Z";
  if (Buffer.byteLength(text, "utf8") !== 262_144) throw new Error("Chat fixture byte size drifted.");
  return text;
}

function splitFixture(text, count) {
  const codePoints = Array.from(text);
  const deltas = [];
  let cursor = 0;
  let emittedBytes = 0;
  for (let index = 1; index <= count; index += 1) {
    const targetBytes = Math.floor((262_144 * index) / count);
    const start = cursor;
    while (cursor < codePoints.length && (emittedBytes < targetBytes || cursor === start)) {
      emittedBytes += Buffer.byteLength(codePoints[cursor], "utf8");
      cursor += 1;
    }
    deltas.push(codePoints.slice(start, cursor).join(""));
  }
  if (cursor < codePoints.length) deltas[deltas.length - 1] += codePoints.slice(cursor).join("");
  if (deltas.length !== count || deltas.some((delta) => !delta)) throw new Error("Invalid chat delta split.");
  if (deltas.join("") !== text) throw new Error("Chat delta byte order drifted.");
  return deltas;
}

async function runChatBenchmark(endpoint) {
  const repetitions = Number(args.get("--chat-repetitions") ?? "1");
  if (!Number.isInteger(repetitions) || repetitions < 1) {
    throw new Error("--chat-repetitions must be a positive integer.");
  }
  const fixture = createChatFixture();
  const projectId = args.get("--project-id");
  const projectRoot = args.get("--project-root");
  if (!projectId || !projectRoot) throw new Error("Chat mode requires --project-id and --project-root.");
  if (args.get("--confirm-synthetic-fixture") !== "yes" || path.basename(path.resolve(projectRoot)).toLowerCase() !== "fixture-project") {
    throw new Error("Chat mode mutates provider/session state and is restricted to a disposable directory named fixture-project; pass --confirm-synthetic-fixture yes.");
  }
  const runId = `${Date.now().toString(36)}-${process.pid}`;

  const streams = [];
  const requests = [];
  const streamTimers = new Set();
  const server = createServer((request, response) => {
    requests.push({ method: request.method, url: request.url });
    if (request.url === "/api/tags") {
      response.writeHead(200, { "content-type": "application/json", connection: "close" });
      response.end(JSON.stringify({ models: [{ name: "perf-fixture" }] }));
      return;
    }
    if (request.url !== "/api/chat" || request.method !== "POST") {
      response.writeHead(404).end();
      return;
    }
    const stream = streams.shift();
    if (!stream) {
      response.writeHead(409).end();
      return;
    }
    response.writeHead(200, {
      "content-type": "application/x-ndjson; charset=utf-8",
      "cache-control": "no-store",
      connection: "close",
    });
    stream.response = response;
    stream.startedAt = performance.now();
    let index = 0;
    const batchSize = Math.max(1, Math.ceil(stream.deltas.length / 200));
    const timer = setInterval(() => {
      for (let batchIndex = 0; batchIndex < batchSize && index < stream.deltas.length; batchIndex += 1) {
        response.write(`${JSON.stringify({ message: { content: stream.deltas[index] }, done: false })}\n`);
        index += 1;
      }
      if (index >= stream.deltas.length) {
        clearInterval(timer);
        streamTimers.delete(timer);
        response.end(`${JSON.stringify({ message: { content: "" }, done: true })}\n`);
        stream.endedAt = performance.now();
        stream.resolve();
      }
    }, 20);
    stream.timer = timer;
    streamTimers.add(timer);
    response.once("close", () => {
      clearInterval(timer);
      streamTimers.delete(timer);
    });
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(11435, "127.0.0.1", resolve);
  });

  let client;
  const results = [];
  const attempts = [];
  try {
  client = await CdpClient.connect(endpoint);
  await waitUntilInteractive(client);
  await client.evaluate(`window.__TAURI_INTERNALS__.invoke('save_llm_provider', { request: {
    projectId: ${JSON.stringify(projectId)},
    projectRootPath: ${JSON.stringify(projectRoot)},
    config: { provider: 'ollama', model: 'perf-fixture', baseUrl: 'http://127.0.0.1:11435', contextWindow: 32768, enabled: true }
  } })`);

  for (const deltaCount of [1_000, 10_000]) {
    for (let repetition = 0; repetition < repetitions; repetition += 1) {
    const sessionTitle = `Batch 6 ${runId} ${deltaCount} deltas ${repetition + 1}`;
    const stream = { deltas: splitFixture(fixture, deltaCount) };
    try {
    stream.done = new Promise((resolve) => { stream.resolve = resolve; });
    streams.push(stream);
    const session = await client.evaluate(`window.__TAURI_INTERNALS__.invoke('create_chat_session', { request: {
      projectId: ${JSON.stringify(projectId)},
      projectRootPath: ${JSON.stringify(projectRoot)},
      title: ${JSON.stringify(sessionTitle)},
      contextPagePath: null
    } })`);
    await client.command("Page.reload", { ignoreCache: false });
    await delay(250);
    await waitUntilInteractive(client);
    // Project bootstrap can briefly render the persisted route before restoring the
    // safe Dashboard default. Wait for that handoff, then keep the benchmark pinned
    // to Chat until its lazy boundary and composer have both settled.
    await delay(1_500);
    await client.evaluate(`(async () => {
      const deadline = performance.now() + 10000;
      while (!document.querySelector('textarea[aria-label="Chat message"]')) {
        if (performance.now() > deadline) throw new Error('Chat composer did not load.');
        const chatButton = document.querySelector('button[aria-label="Chat"]');
        if (!chatButton) throw new Error('Chat route button missing.');
        if (chatButton.getAttribute('aria-current') !== 'page') chatButton.click();
        await new Promise(requestAnimationFrame);
      }
      const sessionTitle = ${JSON.stringify(sessionTitle)};
      const sessionDeadline = performance.now() + 10000;
      let sessionButton = null;
      while (!sessionButton) {
        sessionButton = Array.from(document.querySelectorAll('button')).find((button) => button.textContent.trim() === sessionTitle);
        if (performance.now() > sessionDeadline) throw new Error('Created Chat session is missing from the session list.');
        if (!sessionButton) await new Promise(requestAnimationFrame);
      }
      sessionButton.click();
      while (!document.querySelector('.chat-route-seg')) {
        if (performance.now() > sessionDeadline) throw new Error('Created Chat session did not become active.');
        await new Promise(requestAnimationFrame);
      }
      const byok = Array.from(document.querySelectorAll('.chat-route-seg button')).find((button) => button.textContent.trim() === 'BYOK');
      if (!byok) throw new Error('BYOK route button missing.');
      byok.click();
      const byokDeadline = performance.now() + 10000;
      while (byok.getAttribute('aria-pressed') !== 'true') {
        if (performance.now() > byokDeadline) throw new Error('BYOK route did not become active.');
        await new Promise(requestAnimationFrame);
      }
      await new Promise(requestAnimationFrame);
      window.__batch6ChatMutations = 0;
      window.__batch6ChatLongTasks = [];
      new PerformanceObserver((list) => {
        for (const entry of list.getEntries()) if (entry.duration > 50) window.__batch6ChatLongTasks.push(entry.duration);
      }).observe({ type: 'longtask', buffered: false });
      const transcript = document.querySelector('[role="log"]');
      new MutationObserver((records) => { window.__batch6ChatMutations += records.length; }).observe(transcript, { childList: true, subtree: true, characterData: true });
      const textarea = document.querySelector('textarea[aria-label="Chat message"]');
      const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set;
      setter.call(textarea, 'Run the deterministic packaged stream fixture.');
      textarea.dispatchEvent(new Event('input', { bubbles: true }));
      await new Promise(requestAnimationFrame);
      const send = Array.from(document.querySelectorAll('button')).find((button) => button.textContent.trim() === 'Send');
      if (!send || send.disabled) throw new Error('Chat send button unavailable.');
      send.click();
    })()`);

    const requestDeadline = performance.now() + 15_000;
    while (!stream.startedAt && performance.now() < requestDeadline) await delay(20);
    if (!stream.startedAt) throw new Error(`Ollama fixture was not requested; requests=${JSON.stringify(requests)}`);

    await delay(250);
    const midStreamInteraction = await client.evaluate(`(async () => {
      const textarea = document.querySelector('textarea[aria-label="Chat message"]');
      const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set;
      setter.call(textarea, 'Draft stays editable while generating.');
      textarea.dispatchEvent(new Event('input', { bubbles: true }));
      const transcript = document.querySelector('[role="log"]');
      transcript.scrollTop = 0;
      await new Promise(requestAnimationFrame);
      return {
        draftEditable: textarea.value === 'Draft stays editable while generating.',
        scrolledAwayFromBottom: transcript.scrollHeight > transcript.clientHeight && transcript.scrollTop < transcript.scrollHeight - transcript.clientHeight,
      };
    })()`);
    const splitterRect = await client.evaluate(`(() => {
      const separator = document.querySelector('[role="separator"][aria-label="Resize context panel"]');
      return { ...separator.getBoundingClientRect().toJSON(), value: Number(separator.getAttribute('aria-valuenow')) };
    })()`);
    const startX = splitterRect.x + splitterRect.width / 2;
    const startY = splitterRect.y + splitterRect.height / 2;
    await client.command("Input.dispatchMouseEvent", { type: "mousePressed", x: startX, y: startY, button: "left", buttons: 1, clickCount: 1 });
    for (let move = 0; move < 30; move += 1) {
      await client.command("Input.dispatchMouseEvent", { type: "mouseMoved", x: startX - (move / 29) * 20, y: startY, button: "none", buttons: 1 });
      await delay(16);
    }
    await client.command("Input.dispatchMouseEvent", { type: "mouseReleased", x: startX - 20, y: startY, button: "left", buttons: 0, clickCount: 1 });
    const paneDragChangedValue = await client.evaluate(`Number(document.querySelector('[role="separator"][aria-label="Resize context panel"]').getAttribute('aria-valuenow')) !== ${JSON.stringify(splitterRect.value)}`);

    const routeRoundTrip = {
      wikiMs: (await client.evaluate(clickRouteExpression("Wiki"))).durationMs,
      chatMs: null,
    };
    await delay(250);
    routeRoundTrip.chatMs = (await client.evaluate(clickRouteExpression("Chat"))).durationMs;

    let streamTimeout;
    await Promise.race([
      stream.done,
      new Promise((_, reject) => {
        streamTimeout = setTimeout(() => reject(new Error(`Ollama fixture stream timed out; requests=${JSON.stringify(requests)}`)), 35_000);
      }),
    ]).finally(() => clearTimeout(streamTimeout));
    let persisted;
    let assistant;
    const persistedDeadline = performance.now() + 30_000;
    while (!assistant && performance.now() < persistedDeadline) {
      persisted = await client.evaluate(`window.__TAURI_INTERNALS__.invoke('load_chat_session', { request: {
        projectId: ${JSON.stringify(projectId)}, projectRootPath: ${JSON.stringify(projectRoot)}, sessionId: ${JSON.stringify(session.id)}
      } })`);
      assistant = persisted.messages.find((message) => message.role === "assistant");
      if (!assistant) await delay(50);
    }
    if (!assistant?.taskId) throw new Error("Chat assistant result did not persist with a task id.");
    const taskId = assistant.taskId;
    const task = await client.evaluate(`window.__TAURI_INTERNALS__.invoke('get_task', { request: {
      taskId: ${JSON.stringify(taskId)}, projectId: ${JSON.stringify(projectId)}, projectRootPath: ${JSON.stringify(projectRoot)}
    } })`);
    if (task?.status !== "succeeded") throw new Error(`Chat task ended as ${task?.status}: ${task?.error?.message ?? "unknown"}`);
    await delay(500);
    const ui = await client.evaluate(`(() => {
      const transcript = document.querySelector('[role="log"]');
      const textarea = document.querySelector('textarea[aria-label="Chat message"]');
      return {
        mutations: window.__batch6ChatMutations,
        longTasksOver50Ms: window.__batch6ChatLongTasks,
        draft: textarea.value,
        scrollTop: transcript.scrollTop,
        scrollHeight: transcript.scrollHeight,
        clientHeight: transcript.clientHeight,
        scrolledAwayFromBottom: transcript.scrollHeight > transcript.clientHeight && transcript.scrollTop < transcript.scrollHeight - transcript.clientHeight,
        backToLatestVisible: Array.from(document.querySelectorAll('button')).some((button) => button.textContent.includes('latest')),
      };
    })()`);
    results.push({
      deltaCount,
      repetition: repetition + 1,
      terminalStatus: task.status,
      streamedDurationMs: stream.endedAt - stream.startedAt,
      persistedUtf8Bytes: Buffer.byteLength(assistant?.content ?? "", "utf8"),
      byteEqual: assistant?.content === fixture,
      routeRoundTrip,
      midStreamInteraction,
      paneDragChangedValue,
      ui,
    });
    attempts.push({ deltaCount, repetition: repetition + 1, status: "completed", sessionId: session.id });
    } catch (error) {
      attempts.push({ deltaCount, repetition: repetition + 1, ...failedAttempt(error) });
      if (stream.timer) {
        clearInterval(stream.timer);
        streamTimers.delete(stream.timer);
      }
      stream.response?.destroy();
      const streamIndex = streams.indexOf(stream);
      if (streamIndex >= 0) streams.splice(streamIndex, 1);
    }
    }
  }
  const failureCount = attempts.filter((attempt) => attempt.status === "failed").length;
  const chatResult = {
    status: failureCount === 0 ? "completed" : "failed",
    runId,
    fixtureUtf8Bytes: Buffer.byteLength(fixture, "utf8"),
    attempts,
    failureCount,
    summaries: Object.fromEntries([1_000, 10_000].map((deltaCount) => {
      const matching = results.filter((sample) => sample.deltaCount === deltaCount);
      return [deltaCount, {
        completed: matching.length,
        streamedDurationMs: summary(matching.map((sample) => sample.streamedDurationMs)),
        byteEqual: matching.filter((sample) => sample.byteEqual).length,
        longTasksOver50Ms: summary(matching.flatMap((sample) => sample.ui.longTasksOver50Ms)),
        interactionChecks: {
          draftEditableDuringGeneration: matching.filter((sample) => sample.midStreamInteraction.draftEditable).length,
          scrollAwayFromBottomEstablished: matching.filter((sample) => sample.midStreamInteraction.scrolledAwayFromBottom).length,
          paneDragChangedValue: matching.filter((sample) => sample.paneDragChangedValue).length,
          routeRoundTripCompleted: matching.filter((sample) => Number.isFinite(sample.routeRoundTrip.wikiMs) && Number.isFinite(sample.routeRoundTrip.chatMs)).length,
          draftRetainedAfterRouteRoundTrip: matching.filter((sample) => sample.ui.draft === "Draft stays editable while generating.").length,
          scrollAwayFromBottomAfterCompletion: matching.filter((sample) => sample.ui.scrolledAwayFromBottom).length,
          backToLatestVisibleAfterCompletion: matching.filter((sample) => sample.ui.backToLatestVisible).length,
        },
      }];
    })),
    sideEffects: "Benchmark-owned provider/session state remains only in the explicitly confirmed disposable fixture project; discard that project after the run.",
  };
  if (args.get("--output-detail") === "raw") chatResult.runs = results;
  return chatResult;
  } finally {
  client?.close();
  for (const timer of streamTimers) clearInterval(timer);
  streamTimers.clear();
  server.closeAllConnections();
  await new Promise((resolve) => server.close(resolve));
  }
}

const mode = args.get("--mode");
let result;
try {
  result = mode === "routes-and-splitters"
    ? await runRoutesAndSplitters(args.get("--endpoint") ?? "http://127.0.0.1:9223")
    : mode === "startup"
      ? await runStartupSamples()
      : mode === "chat"
        ? await runChatBenchmark(args.get("--endpoint") ?? "http://127.0.0.1:9223")
        : null;
  if (!result) throw new Error("Use --mode routes-and-splitters, --mode startup, or --mode chat.");
} catch (error) {
  result = { mode, ...failedAttempt(error) };
}
process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
