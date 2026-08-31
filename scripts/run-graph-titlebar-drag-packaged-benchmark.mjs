import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { performance } from "node:perf_hooks";
import { fileURLToPath, pathToFileURL } from "node:url";

import { evaluateGraphTitlebarDragResult } from "./graph-titlebar-drag-contract.mjs";
import { verifyGraphTitlebarDragProvenance } from "./graph-titlebar-drag-provenance.mjs";
import { verifyProjectFactsPackagedFixtures } from "./prepare-project-facts-packaged-fixtures.mjs";

const delay = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

function parseArguments(argv) {
  const parsed = new Map();
  for (let index = 2; index < argv.length; index += 2) parsed.set(argv[index], argv[index + 1]);
  return parsed;
}

function requiredPath(argumentsMap, name) {
  const value = argumentsMap.get(name);
  if (!value) throw new Error(`${name} is required.`);
  return path.resolve(value);
}

function requiredText(argumentsMap, name) {
  const value = argumentsMap.get(name);
  if (!value) throw new Error(`${name} is required.`);
  return value;
}

async function requireAbsent(label, target) {
  try {
    await stat(target);
    throw new Error(`${label} must not already exist: ${target}`);
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }
}

async function fileEvidence(file) {
  const bytes = await readFile(file);
  const metadata = await stat(file);
  return {
    path: file,
    size: metadata.size,
    sha256: createHash("sha256").update(bytes).digest("hex"),
  };
}

async function runChild(command, args, options = {}, timeoutMs = 30_000) {
  const child = spawn(command, args, {
    windowsHide: true,
    stdio: ["ignore", "pipe", "pipe"],
    ...options,
  });
  let stdout = "";
  let stderr = "";
  child.stdout?.on("data", (chunk) => { stdout += chunk.toString(); });
  child.stderr?.on("data", (chunk) => { stderr += chunk.toString(); });
  const code = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      child.kill();
      reject(new Error(`${command} timed out.`));
    }, timeoutMs);
    child.once("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
    child.once("exit", (exitCode) => {
      clearTimeout(timer);
      resolve(exitCode);
    });
  });
  if (code !== 0) throw new Error(stderr.trim() || `${command} exited ${code}.`);
  return stdout.trim();
}

async function git(repository, args) {
  return runChild("git", ["-C", repository, ...args], {}, 15_000);
}

class CdpClient {
  constructor(socket) {
    this.socket = socket;
    this.nextId = 0;
    this.pending = new Map();
    this.eventHandlers = new Map();
    socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      const waiter = this.pending.get(message.id);
      if (waiter) {
        this.pending.delete(message.id);
        clearTimeout(waiter.timer);
        if (message.error) waiter.reject(new Error(message.error.message));
        else waiter.resolve(message.result);
        return;
      }
      const handler = this.eventHandlers.get(message.method);
      if (handler) void handler(message.params);
    });
  }

  static async connect(endpoint) {
    const deadline = performance.now() + 30_000;
    while (performance.now() < deadline) {
      try {
        const targets = await fetch(`${endpoint}/json`, { signal: AbortSignal.timeout(2_000) })
          .then((response) => response.json());
        const target = targets.find((candidate) => candidate.type === "page" && candidate.url !== "about:blank");
        if (target) {
          const socket = new WebSocket(target.webSocketDebuggerUrl);
          await new Promise((resolve, reject) => {
            const timer = setTimeout(() => reject(new Error("CDP socket open timed out.")), 5_000);
            socket.addEventListener("open", () => { clearTimeout(timer); resolve(); }, { once: true });
            socket.addEventListener("error", reject, { once: true });
          });
          const client = new CdpClient(socket);
          client.target = {
            id: target.id,
            title: target.title,
            type: target.type,
            url: target.url,
          };
          return client;
        }
      } catch {
        // The packaged page can appear before its first navigation completes.
      }
      await delay(50);
    }
    throw new Error(`No packaged WebView page was available at ${endpoint}.`);
  }

  command(method, params = {}, timeoutMs = 30_000) {
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

  async evaluate(expression, timeoutMs = 30_000) {
    for (let attempt = 0; attempt < 5; attempt += 1) {
      try {
        const response = await this.command("Runtime.evaluate", {
          expression,
          awaitPromise: true,
          returnByValue: true,
        }, timeoutMs);
        if (response.exceptionDetails) {
          throw new Error(response.exceptionDetails.exception?.description ?? response.exceptionDetails.text);
        }
        return response.result.value;
      } catch (error) {
        if (!error.message.includes("Execution context was destroyed") || attempt === 4) throw error;
        await delay(100);
      }
    }
    throw new Error("Runtime evaluation did not complete.");
  }

  on(method, handler) {
    this.eventHandlers.set(method, handler);
    return () => this.eventHandlers.delete(method);
  }

  close() {
    this.socket.close();
  }
}

async function waitForInteractive(client) {
  await client.evaluate(`(async () => {
    const deadline = performance.now() + 20000;
    while (!document.body || document.body.childElementCount === 0) {
      if (performance.now() > deadline) throw new Error('App did not become interactive.');
      await new Promise(requestAnimationFrame);
    }
    await new Promise(requestAnimationFrame);
    return true;
  })()`);
}

async function resolveNativeWindowProcess(rootProcessId) {
  const script = String.raw`
$root = ${rootProcessId}
$deadline = [DateTime]::UtcNow.AddSeconds(15)
while ([DateTime]::UtcNow -lt $deadline) {
  $all = @(Get-CimInstance Win32_Process)
  $ids = @($root)
  do {
    $before = $ids.Count
    $children = @($all | Where-Object { $ids -contains [int]$_.ParentProcessId } | ForEach-Object { [int]$_.ProcessId })
    $ids = @($ids + $children | Select-Object -Unique)
  } while ($ids.Count -gt $before)
  $candidate = Get-Process -Id $ids -ErrorAction SilentlyContinue |
    Where-Object { $_.MainWindowHandle -ne [IntPtr]::Zero } |
    Select-Object -First 1
  if ($candidate) { Write-Output $candidate.Id; exit 0 }
  Start-Sleep -Milliseconds 50
}
throw 'No native app window belongs to the spawned process tree.'
`;
  const processId = Number(await runChild("powershell.exe", [
    "-NoProfile", "-NonInteractive", "-Command", script,
  ], {}, 20_000));
  if (!Number.isInteger(processId) || processId <= 0) throw new Error("Resolved native process ID is invalid.");
  return processId;
}

async function installBrowserObserver(client) {
  return client.evaluate(`(async () => {
    const internals = window.__TAURI_INTERNALS__;
    if (!internals || typeof internals.invoke !== 'function' || typeof internals.transformCallback !== 'function') {
      throw new Error('Tauri internals are unavailable.');
    }
    const observer = {
      domFocusTimeline: [{ timestampUnixMs: Date.now(), focused: document.hasFocus(), type: 'initial' }],
      rawFocusTimeline: [{ timestampUnixMs: Date.now(), focused: document.hasFocus(), event: 'initial' }],
      normalizedForegroundTimeline: [],
      resizeTimeline: [],
      frameGaps: [],
      longTasks: [],
      running: true,
      invokeDescriptor: Object.getOwnPropertyDescriptor(internals, 'invoke') ?? null,
      eventIds: [],
    };
    const originalInvoke = internals.invoke.bind(internals);
    const recordFocus = (type) => observer.domFocusTimeline.push({
      timestampUnixMs: Date.now(), focused: document.hasFocus(), type,
    });
    window.addEventListener('focus', () => recordFocus('focus'));
    window.addEventListener('blur', () => recordFocus('blur'));
    window.addEventListener('resize', () => observer.resizeTimeline.push({ timestampUnixMs: Date.now() }));
    const normalizedCallbackId = internals.transformCallback((event) => {
      observer.normalizedForegroundTimeline.push({
        timestampUnixMs: Date.now(),
        foreground: Boolean(event?.payload?.foreground),
      });
    });
    observer.eventIds.push(await originalInvoke('plugin:event|listen', {
      event: 'app://foreground-changed',
      target: { kind: 'Any' },
      handler: normalizedCallbackId,
    }));
    for (const [event, focused] of [['tauri://focus', true], ['tauri://blur', false]]) {
      const handler = internals.transformCallback(() => {
        observer.rawFocusTimeline.push({ timestampUnixMs: Date.now(), focused, event });
      });
      observer.eventIds.push(await originalInvoke('plugin:event|listen', {
        event,
        target: { kind: 'Window', label: internals.metadata.currentWindow.label },
        handler,
      }));
    }
    let previousFrame = performance.now();
    const tick = (now) => {
      observer.frameGaps.push({ timestampUnixMs: Date.now(), durationMs: now - previousFrame });
      previousFrame = now;
      if (observer.running) requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
    if (typeof PerformanceObserver === 'function') {
      observer.longTaskObserver = new PerformanceObserver((list) => {
        for (const entry of list.getEntries()) {
          observer.longTasks.push({ timestampUnixMs: Date.now(), durationMs: entry.duration });
        }
      });
      observer.longTaskObserver.observe({ type: 'longtask', buffered: false });
    }
    window.__LLM_GRAPH_TITLEBAR_OBSERVER__ = observer;
    return { rawFocusEventsObserved: true, invokeDescriptor: observer.invokeDescriptor };
  })()`);
}

async function installGetGraphInvokeObserver(client) {
  const remote = await client.command("Runtime.evaluate", {
    expression: "window.__TAURI_INTERNALS__.invoke",
    returnByValue: false,
  });
  const objectId = remote.result?.objectId;
  if (!objectId) throw new Error("Packaged invoke function object was unavailable to CDP.");
  await client.command("Debugger.enable");
  const breakpoint = await client.command("Debugger.setBreakpointOnFunctionCall", {
    objectId,
    condition: "arguments[0] === 'get_graph'",
  });
  const state = { count: 0, timeline: [], errors: [] };
  const unbind = client.on("Debugger.paused", async (params) => {
    try {
      const frame = params.callFrames?.[0];
      if (!frame) throw new Error("get_graph breakpoint pause had no call frame.");
      state.count += 1;
      state.timeline.push({ timestampUnixMs: Date.now(), command: "get_graph", count: state.count });
    } catch (error) {
      state.errors.push(error.message);
    } finally {
      await client.command("Debugger.resume").catch((error) => state.errors.push(error.message));
    }
  });
  return {
    mode: "cdp-function-call-breakpoint",
    breakpointId: breakpoint.breakpointId,
    reset() {
      state.count = 0;
      state.timeline = [];
      state.errors = [];
    },
    snapshot() {
      return { count: state.count, timeline: [...state.timeline], errors: [...state.errors] };
    },
    async dispose() {
      unbind();
      await client.command("Debugger.removeBreakpoint", { breakpointId: breakpoint.breakpointId }).catch(() => undefined);
      await client.command("Debugger.disable").catch(() => undefined);
    },
  };
}

async function resetBrowserPhase(client, label) {
  return client.evaluate(`(() => {
    const observer = window.__LLM_GRAPH_TITLEBAR_OBSERVER__;
    if (!observer) throw new Error('Graph titlebar observer is unavailable.');
    observer.phase = ${JSON.stringify(label)};
    observer.phaseStartedAtUnixMs = Date.now();
    observer.domFocusTimeline = [{ timestampUnixMs: Date.now(), focused: document.hasFocus(), type: 'phase-start' }];
    observer.normalizedForegroundTimeline = [];
    observer.resizeTimeline = [];
    observer.frameGaps = [];
    observer.longTasks = [];
    observer.rawFocusTimeline = [{ timestampUnixMs: Date.now(), focused: document.hasFocus(), event: 'phase-start' }];
    return { startedAtUnixMs: observer.phaseStartedAtUnixMs };
  })()`);
}

async function browserPhaseSnapshot(client) {
  return client.evaluate(`(() => {
    const observer = window.__LLM_GRAPH_TITLEBAR_OBSERVER__;
    return {
      phase: observer.phase,
      phaseStartedAtUnixMs: observer.phaseStartedAtUnixMs,
      phaseEndedAtUnixMs: Date.now(),
      domFocusTimeline: [...observer.domFocusTimeline],
      rawFocusTimeline: [...observer.rawFocusTimeline],
      normalizedForegroundTimeline: [...observer.normalizedForegroundTimeline],
      resizeTimeline: [...observer.resizeTimeline],
      frameGaps: [...observer.frameGaps],
      longTasks: [...observer.longTasks],
      route: document.querySelector('[data-app-view][aria-current="page"]')?.getAttribute('data-app-view') ?? null,
    };
  })()`);
}

async function invoke(client, command, request) {
  return client.evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)}, { request: ${JSON.stringify(request)} })`);
}

async function activateNativeProject(client, nativeRoot) {
  const opened = await invoke(client, "open_project", { path: nativeRoot });
  if (!opened?.summary) throw new Error("Native fixture did not open.");
  await client.command("Page.reload", { ignoreCache: false }).catch((error) => {
    if (!error.message.includes("Execution context was destroyed")) throw error;
  });
  await delay(500);
  await waitForInteractive(client);
  const ready = await client.evaluate(`Boolean(document.querySelector('[data-app-view="dashboard"]'))`);
  if (!ready) throw new Error("Project workbench did not become ready after fixture registration.");
  return opened.summary;
}

async function activateRoute(client, route) {
  await client.evaluate(`(async () => {
    const route = ${JSON.stringify(route)};
    const button = document.querySelector('[data-app-view="' + route + '"]');
    if (!button) throw new Error('Route button missing: ' + route);
    button.click();
    const deadline = performance.now() + 20000;
    while (button.getAttribute('aria-current') !== 'page') {
      if (performance.now() > deadline) throw new Error('Route did not activate: ' + route);
      await new Promise(requestAnimationFrame);
    }
    await new Promise(requestAnimationFrame);
    return true;
  })()`);
  await delay(route === "graph" ? 2_000 : 500);
}

async function runNativeHelper(helper, processId, mode, parameters = {}) {
  const args = [
    "-NoProfile", "-NonInteractive", "-STA", "-ExecutionPolicy", "Bypass", "-File", helper,
    "-ProcessId", String(processId), "-Mode", mode,
  ];
  if (mode === "drag") {
    args.push(
      "-Samples", String(parameters.samples),
      "-StepX", String(parameters.stepX),
      "-StepY", String(parameters.stepY),
      "-CadenceMs", String(parameters.cadenceMs),
    );
  }
  try {
    return JSON.parse(await runChild("powershell.exe", args, {}, mode === "drag" ? 45_000 : 15_000));
  } catch (error) {
    if (mode === "drag") {
      await runChild("powershell.exe", [
        "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", helper,
        "-ProcessId", String(processId), "-Mode", "release-mouse",
      ], {}, 15_000).catch(() => undefined);
    }
    throw error;
  }
}

function latestAt(timeline, timestamp, field, fallback) {
  let value = fallback;
  for (const item of timeline) {
    if (item.timestampUnixMs > timestamp) break;
    value = item[field];
  }
  return value;
}

function enrichNativeSamples(native, browser, route) {
  return {
    ...native,
    samples: native.samples.map((sample) => ({
      ...sample,
      route,
      domFocusEventCount: browser.domFocusTimeline.filter((event) => event.timestampUnixMs <= sample.timestampUnixMs).length,
      rawFocused: latestAt(browser.rawFocusTimeline, sample.timestampUnixMs, "focused", true),
      rawFocusedSource: "Tauri tauri://focus and tauri://blur",
      normalizedForeground: latestAt(browser.normalizedForegroundTimeline, sample.timestampUnixMs, "foreground", true),
      getGraphPhaseCount: browser.getGraphTimeline
        .filter((event) => event.timestampUnixMs <= sample.timestampUnixMs).length,
    })),
  };
}

function roundDiagnostics(browser, native) {
  const firstSampleAt = native.samples.at(0)?.timestampUnixMs ?? Number.POSITIVE_INFINITY;
  const lastSampleAt = native.samples.at(-1)?.timestampUnixMs ?? Number.NEGATIVE_INFINITY;
  const resizeDuringMeasurement = browser.resizeTimeline.filter((event) => (
    event.timestampUnixMs >= firstSampleAt && event.timestampUnixMs <= lastSampleAt
  ));
  return {
    getGraphObserverMode: browser.getGraphObserverMode,
    getGraphObserverErrors: browser.getGraphObserverErrors,
    getGraphDelta: browser.getGraphCount,
    resizeEvents: resizeDuringMeasurement.length,
    resizeEventsBeforeMeasurement: browser.resizeTimeline.length - resizeDuringMeasurement.length,
    domFocusEvents: browser.domFocusTimeline,
    rawFocusEvents: browser.rawFocusTimeline,
    rawFocusedSource: "Tauri tauri://focus and tauri://blur",
    normalizedForegroundEvents: browser.normalizedForegroundTimeline,
    normalizedForegroundFalseCount: browser.normalizedForegroundTimeline
      .filter((event) => event.foreground === false).length,
    frameGapP95Ms: percentile(browser.frameGaps.map((entry) => entry.durationMs), 0.95),
    frameGapsOver100Ms: browser.frameGaps.filter((entry) => entry.durationMs > 100).length,
    longTasksOver100Ms: browser.longTasks.filter((entry) => entry.durationMs > 100).length,
    applicationStallsOver100Ms: browser.longTasks.filter((entry) => entry.durationMs > 100).length,
  };
}

function percentile(values, quantile) {
  if (values.length === 0) return null;
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.ceil(quantile * sorted.length) - 1];
}

async function captureDragRound(client, invokeObserver, helper, processId, route, round, parameters) {
  await resetBrowserPhase(client, `${route}-${round}`);
  invokeObserver.reset();
  const native = await runNativeHelper(helper, processId, "drag", parameters);
  await delay(250);
  const browser = {
    ...(await browserPhaseSnapshot(client)),
    getGraphCount: invokeObserver.snapshot().count,
    getGraphTimeline: invokeObserver.snapshot().timeline,
    getGraphObserverErrors: invokeObserver.snapshot().errors,
    getGraphObserverMode: invokeObserver.mode,
  };
  return {
    round,
    route,
    startedAtUnixMs: browser.phaseStartedAtUnixMs,
    endedAtUnixMs: browser.phaseEndedAtUnixMs,
    actualRoute: browser.route,
    native: enrichNativeSamples(native, browser, browser.route),
    diagnostics: roundDiagnostics(browser, native),
  };
}

async function captureDragWarmUp(client, invokeObserver, helper, processId, route, parameters) {
  await resetBrowserPhase(client, `${route}-warmup`);
  invokeObserver.reset();
  const native = await runNativeHelper(helper, processId, "drag", parameters);
  await delay(250);
  const browser = await browserPhaseSnapshot(client);
  const invokes = invokeObserver.snapshot();
  return {
    phase: browser.phase,
    route,
    actualRoute: browser.route,
    sampleCount: native.sampleCount,
    observerErrors: invokes.errors,
    getGraphDelta: invokes.count,
    normalizedForegroundFalseCount: browser.normalizedForegroundTimeline
      .filter((event) => event.foreground === false).length,
  };
}

async function terminateProcessTree(child) {
  if (!child || child.exitCode !== null) return;
  await runChild("taskkill.exe", ["/PID", String(child.pid), "/T", "/F"], {}, 10_000).catch(() => undefined);
}

async function systemEvidence(executable) {
  const script = String.raw`
$executable = $env:GRAPH_DRAG_EXE
$os = Get-CimInstance Win32_OperatingSystem
$display = @(Get-CimInstance Win32_VideoController | Where-Object { $_.CurrentRefreshRate } | Select-Object Name,CurrentHorizontalResolution,CurrentVerticalResolution,CurrentRefreshRate)
$webview = @(
  Get-ItemProperty -Path 'HKCU:\Software\Microsoft\EdgeUpdate\Clients\*' -ErrorAction SilentlyContinue
  Get-ItemProperty -Path 'HKLM:\Software\Microsoft\EdgeUpdate\Clients\*' -ErrorAction SilentlyContinue
  Get-ItemProperty -Path 'HKLM:\Software\WOW6432Node\Microsoft\EdgeUpdate\Clients\*' -ErrorAction SilentlyContinue
) | Where-Object { $_.name -match 'WebView2' } | Select-Object -First 1
$desktop = Get-ItemProperty -Path 'HKCU:\Control Panel\Desktop' -ErrorAction SilentlyContinue
$scale = if ($desktop.LogPixels) { [Math]::Round(([double]$desktop.LogPixels / 96) * 100) } else { $null }
$version = (Get-Item -LiteralPath $executable).VersionInfo
[pscustomobject]@{ osCaption=$os.Caption; osVersion=$os.Version; osBuild=$os.BuildNumber; displays=$display; configuredDisplayScalePercent=$scale; webView2RuntimeVersion=$webview.pv; productName=$version.ProductName; productVersion=$version.ProductVersion; fileVersion=$version.FileVersion } | ConvertTo-Json -Compress -Depth 5
`;
  return JSON.parse(await runChild("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", script], {
    env: { ...process.env, GRAPH_DRAG_EXE: executable },
  }, 15_000));
}

export async function runGraphTitlebarDragPackagedBenchmark(argv = process.argv) {
  if (process.platform !== "win32") throw new Error("The full packaged benchmark requires Windows.");
  const argumentsMap = parseArguments(argv);
  const repository = requiredPath(argumentsMap, "--source-repository");
  const sourceCommit = requiredText(argumentsMap, "--source-commit");
  const installer = requiredPath(argumentsMap, "--installer");
  const builtExe = requiredPath(argumentsMap, "--built-exe");
  const installedExe = requiredPath(argumentsMap, "--exe");
  const provenancePath = requiredPath(argumentsMap, "--provenance");
  const fixtureRoot = requiredPath(argumentsMap, "--fixture-root");
  const appData = requiredPath(argumentsMap, "--app-data");
  const webviewData = requiredPath(argumentsMap, "--webview-data");
  const output = requiredPath(argumentsMap, "--output");
  const helper = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "run-graph-titlebar-native-input.ps1");
  const port = Number(argumentsMap.get("--port") ?? "9531");
  const parameters = {
    samples: Number(argumentsMap.get("--samples") ?? "112"),
    stepX: Number(argumentsMap.get("--step-x") ?? "2"),
    stepY: Number(argumentsMap.get("--step-y") ?? "1"),
    cadenceMs: Number(argumentsMap.get("--cadence-ms") ?? "16"),
  };
  if (!/^[0-9a-f]{40}$/u.test(sourceCommit)) throw new Error("--source-commit must be a full lowercase Git hash.");
  await Promise.all([installer, builtExe, installedExe, provenancePath, fixtureRoot, helper].map((target) => stat(target)));
  await Promise.all([
    requireAbsent("output", output),
    requireAbsent("AppData", appData),
    requireAbsent("WebView data", webviewData),
  ]);
  const resolvedCommit = await git(repository, ["rev-parse", `${sourceCommit}^{commit}`]);
  if (resolvedCommit !== sourceCommit) throw new Error("Source commit does not resolve exactly.");
  const repositoryHead = await git(repository, ["rev-parse", "HEAD"]);
  if (repositoryHead !== sourceCommit) throw new Error("Source repository HEAD does not equal --source-commit.");
  const sourceTree = await git(repository, ["rev-parse", `${sourceCommit}^{tree}`]);
  const sourceWorkingTreeClean = (await git(repository, ["status", "--porcelain"])).length === 0;
  if (!sourceWorkingTreeClean) throw new Error("Source repository must be clean for packaged acceptance.");
  const packageVersion = JSON.parse(await readFile(path.join(repository, "package.json"), "utf8")).version;
  const [installerEvidence, builtEvidence, installedEvidence] = await Promise.all([
    fileEvidence(installer), fileEvidence(builtExe), fileEvidence(installedExe),
  ]);
  const provenance = JSON.parse(await readFile(provenancePath, "utf8"));
  await verifyGraphTitlebarDragProvenance({
    provenance,
    repository,
    sourceCommit,
    installer,
    builtExecutable: builtExe,
  });
  if (builtEvidence.sha256 !== installedEvidence.sha256 || builtEvidence.size !== installedEvidence.size) {
    throw new Error("Installed executable does not match the executable built with the MSI.");
  }
  const fixtureManifestBytes = await readFile(path.join(fixtureRoot, "fixture-manifest.json"));
  const fixtureManifest = JSON.parse(fixtureManifestBytes.toString("utf8"));
  await verifyProjectFactsPackagedFixtures(fixtureRoot, fixtureManifest);
  await mkdir(path.dirname(output), { recursive: true });
  await mkdir(appData, { recursive: true });
  await mkdir(webviewData, { recursive: true });
  try {
    await fetch(`http://127.0.0.1:${port}/json`, { signal: AbortSignal.timeout(500) });
    throw new Error(`CDP port ${port} is already in use.`);
  } catch (error) {
    if (error.message === `CDP port ${port} is already in use.`) throw error;
  }
  const nativeRoot = path.join(fixtureRoot, "native-git-3-pages");
  const child = spawn(installedExe, [], {
    env: {
      ...process.env,
      APPDATA: appData,
      WEBVIEW2_USER_DATA_FOLDER: webviewData,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: "ignore",
    windowsHide: false,
  });
  let client;
  let invokeObserver;
  try {
    client = await CdpClient.connect(`http://127.0.0.1:${port}`);
    await waitForInteractive(client);
    const processId = await resolveNativeWindowProcess(child.pid);
    const project = await activateNativeProject(client, nativeRoot);
    await installBrowserObserver(client);
    invokeObserver = await installGetGraphInvokeObserver(client);
    const groups = { dashboard: { route: "dashboard", rounds: [] }, graph: { route: "graph", rounds: [] } };
    await activateRoute(client, "dashboard");
    groups.dashboard.warmUp = await captureDragWarmUp(client, invokeObserver, helper, processId, "dashboard", parameters);
    for (let round = 1; round <= 3; round += 1) {
      groups.dashboard.rounds.push(await captureDragRound(client, invokeObserver, helper, processId, "dashboard", round, parameters));
    }
    await resetBrowserPhase(client, "graph-prewarm");
    invokeObserver.reset();
    await activateRoute(client, "graph");
    const prewarm = invokeObserver.snapshot();
    if (prewarm.count !== 1 || prewarm.errors.length !== 0) {
      throw new Error(`Graph prewarm must invoke get_graph exactly once without observer errors: ${JSON.stringify(prewarm)}.`);
    }
    groups.graph.warmUp = await captureDragWarmUp(client, invokeObserver, helper, processId, "graph", parameters);
    for (let round = 1; round <= 3; round += 1) {
      groups.graph.rounds.push(await captureDragRound(client, invokeObserver, helper, processId, "graph", round, parameters));
    }
    await resetBrowserPhase(client, "graph-alt-tab");
    invokeObserver.reset();
    const nativeAltTab = await runNativeHelper(helper, processId, "alt-tab");
    await delay(2_000);
    const altTabBrowser = await browserPhaseSnapshot(client);
    const altTabInvokes = invokeObserver.snapshot();
    const result = {
      schemaVersion: 1,
      status: "captured",
      measuredAt: new Date().toISOString(),
      artifact: {
        sourceCommit,
        sourceTree,
        sourceWorkingTreeClean,
        packageVersion,
        installer: installerEvidence,
        builtExecutable: builtEvidence,
        installedExecutable: installedEvidence,
        installedMatchesBuilt: true,
        buildProvenance: {
          path: provenancePath,
          sha256: (await fileEvidence(provenancePath)).sha256,
          buildCommand: provenance.build.command,
        },
      },
      fixture: {
        root: nativeRoot,
        manifestSha256: createHash("sha256").update(fixtureManifestBytes).digest("hex"),
        fixtureHash: fixtureManifest.fixtureHash,
        gitTree: fixtureManifest.native.gitTree,
        wikiPages: fixtureManifest.native.wikiPages,
        supportFiles: fixtureManifest.native.supportFiles,
        projectId: project.projectId,
      },
      environment: {
        platform: process.platform,
        architecture: process.arch,
        hostname: os.hostname(),
        ...(await systemEvidence(installedExe)),
        measuredWindowDpi: groups.graph.rounds[0]?.native?.dpi ?? null,
        measuredWindowScalePercent: groups.graph.rounds[0]?.native?.displayScalePercent ?? null,
      },
      parameters: {
        dragStimulus: "Win32 SendInput native non-client titlebar",
        positionObserver: "GetWindowRect",
        roundsPerGroup: 3,
        warmUpRoundsPerGroup: 1,
        samplesPerRound: parameters.samples,
        stepX: parameters.stepX,
        stepY: parameters.stepY,
        cadenceMs: parameters.cadenceMs,
        resetUsesMoveWindowOutsideMeasurement: true,
        moveWindowUsedDuringMeasurement: false,
        fixedWindowSize: true,
      },
      observer: {
        getGraphObservationMode: invokeObserver.mode,
        getGraphObservationErrors: altTabInvokes.errors,
        normalizedEvent: "app://foreground-changed",
        rawFocusSource: "Tauri tauri://focus and tauri://blur",
        cdpPortPreflightUnused: true,
        cdpTarget: client.target,
        cdpTargetBoundToSpawnedProcessEnvironment: true,
      },
      nativeHelper: await fileEvidence(helper),
      prewarm: { getGraphCount: prewarm.count, observerErrors: prewarm.errors },
      groups,
      altTab: {
        stimulus: "Win32 SendInput Alt-Tab",
        native: nativeAltTab,
        normalizedForegroundSequence: altTabBrowser.normalizedForegroundTimeline.map((event) => event.foreground),
        getGraphDelta: altTabInvokes.count,
        domFocusEvents: altTabBrowser.domFocusTimeline,
      },
    };
    result.contract = evaluateGraphTitlebarDragResult(result);
    result.status = result.contract.passed ? "passed" : "failed";
    await writeFile(output, `${JSON.stringify(result, null, 2)}\n`, "utf8");
    if (!result.contract.passed) {
      throw new Error(`Packaged Graph titlebar drag contract failed: ${JSON.stringify(result.contract.failures)}`);
    }
    return result;
  } finally {
    await invokeObserver?.dispose();
    client?.close();
    await terminateProcessTree(child);
    await rm(appData, { recursive: true, force: true }).catch(() => undefined);
    await rm(webviewData, { recursive: true, force: true }).catch(() => undefined);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const result = await runGraphTitlebarDragPackagedBenchmark();
  process.stdout.write(`${JSON.stringify({ status: result.status, output: path.resolve(parseArguments(process.argv).get("--output")) })}\n`);
}
