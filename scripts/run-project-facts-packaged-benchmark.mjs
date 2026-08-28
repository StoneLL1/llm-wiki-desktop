import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { performance } from "node:perf_hooks";

import { verifyProjectFactsPackagedFixtures } from "./prepare-project-facts-packaged-fixtures.mjs";
import { verifyProjectFactsPackagedProvenance } from "./project-facts-packaged-provenance.mjs";

const delay = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));
const args = new Map();
for (let index = 2; index < process.argv.length; index += 2) {
  args.set(process.argv[index], process.argv[index + 1]);
}

function required(name) {
  const value = args.get(name);
  if (!value) throw new Error(`${name} is required.`);
  return path.resolve(value);
}

function requiredText(name) {
  const value = args.get(name);
  if (!value) throw new Error(`${name} is required.`);
  return value;
}

function percentile(values, quantile) {
  if (values.length === 0) return null;
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.ceil(quantile * sorted.length) - 1];
}

function summary(values) {
  return {
    count: values.length,
    p50Ms: percentile(values, 0.5),
    p95Ms: percentile(values, 0.95),
    maxMs: values.length === 0 ? null : Math.max(...values),
  };
}

async function requireAbsent(label, target) {
  try {
    await stat(target);
    throw new Error(`${label} must not already exist: ${target}`);
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }
}

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
    const deadline = performance.now() + 30_000;
    while (performance.now() < deadline) {
      try {
        const targets = await fetch(`${endpoint}/json`, {
          signal: AbortSignal.timeout(2_000),
        }).then((response) => response.json());
        const target = targets.find((candidate) =>
          candidate.type === "page" && candidate.url !== "about:blank"
        );
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
        // The WebView target can be advertised before its first navigation.
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

function startNativeWindowMovement(processId, durationMs) {
  const script = String.raw`
$ProcessId = ${processId}
$DurationMs = ${durationMs}
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class Batch4Window {
  [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out Rect rect);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr hWnd, int x, int y, int w, int h, bool repaint);
}
'@
$handleDeadline = [DateTime]::UtcNow.AddSeconds(10)
$handle = [IntPtr]::Zero
while ([DateTime]::UtcNow -lt $handleDeadline -and $handle -eq [IntPtr]::Zero) {
  $candidate = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
  if ($candidate) { $handle = $candidate.MainWindowHandle }
  Start-Sleep -Milliseconds 50
}
if ($handle -eq [IntPtr]::Zero) {
  $observed = Get-Process -ErrorAction SilentlyContinue |
    Where-Object { $_.Id -eq $ProcessId } |
    Select-Object Id, ProcessName, MainWindowHandle, MainWindowTitle |
    ConvertTo-Json -Compress
  throw ('Native app window handle was unavailable. Observed=' + $observed)
}
$rect = New-Object Batch4Window+Rect
[Batch4Window]::GetWindowRect($handle, [ref]$rect) | Out-Null
$width = $rect.Right - $rect.Left
$height = $rect.Bottom - $rect.Top
$originX = $rect.Left
$originY = $rect.Top
$moves = 0
$failedMoves = 0
$deadline = [DateTime]::UtcNow.AddMilliseconds($DurationMs)
while ([DateTime]::UtcNow -lt $deadline) {
  $phase = $moves % 80
  $offset = if ($phase -lt 40) { $phase } else { 80 - $phase }
  if (-not [Batch4Window]::MoveWindow($handle, $originX + $offset, $originY + [int]($offset / 2), $width, $height, $true)) { $failedMoves += 1 }
  $moves += 1
  Start-Sleep -Milliseconds 16
}
$restored = [Batch4Window]::MoveWindow($handle, $originX, $originY, $width, $height, $true)
[pscustomobject]@{ moves=$moves; failedMoves=$failedMoves; restored=$restored } | ConvertTo-Json -Compress
`;
  return spawn("powershell.exe", [
    "-NoProfile",
    "-NonInteractive",
    "-Command",
    script,
  ], { windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
}

async function waitForChild(child, timeoutMs) {
  let stdout = "";
  let stderr = "";
  child.stdout?.on("data", (chunk) => { stdout += chunk.toString(); });
  child.stderr?.on("data", (chunk) => { stderr += chunk.toString(); });
  const exitCode = await Promise.race([
    new Promise((resolve, reject) => {
      child.once("error", reject);
      child.once("exit", resolve);
    }),
    delay(timeoutMs).then(() => {
      child.kill();
      throw new Error("Child process timed out.");
    }),
  ]);
  if (exitCode !== 0) throw new Error(stderr.trim() || `Child exited ${exitCode}.`);
  return stdout.trim();
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
  const child = spawn("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", script], {
    windowsHide: true,
    stdio: ["ignore", "pipe", "pipe"],
  });
  const processId = Number(await waitForChild(child, 20_000));
  if (!Number.isInteger(processId) || processId <= 0) throw new Error("Resolved native process ID is invalid.");
  return processId;
}

async function readBlockingSpans(tracePath) {
  const text = await readFile(tracePath, "utf8").catch((error) => {
    if (error?.code === "ENOENT") return "";
    throw error;
  });
  return text.trim() ? text.trim().split(/\r?\n/u).map((line) => JSON.parse(line)) : [];
}

async function installInvokeObserver(client) {
  await client.evaluate(`(() => {
    if (!window.__LLM_WIKI_PROJECT_FACTS_IPC_COUNTS__
        || typeof window.__LLM_WIKI_PROJECT_FACTS_IPC_COUNTS__ !== 'object') {
      throw new Error('The packaged Project Facts IPC observer is unavailable.');
    }
    return true;
  })()`);
}

async function phaseSnapshot(client, tracePath, label) {
  const spans = await readBlockingSpans(tracePath);
  const operationCounts = {};
  const operationClassCounts = {};
  for (const span of spans) {
    operationCounts[span.operation] = (operationCounts[span.operation] ?? 0) + 1;
    const operationClass = `${span.operation}:${span.class}`;
    operationClassCounts[operationClass] = (operationClassCounts[operationClass] ?? 0) + 1;
  }
  const ipcCounts = await client.evaluate(
    `({ ...(window.__LLM_WIKI_PROJECT_FACTS_IPC_COUNTS__ ?? {}) })`,
  );
  return { label, spanCount: spans.length, operationCounts, operationClassCounts, ipcCounts };
}

function phaseDelta(before, after, command) {
  return (after[command] ?? 0) - (before[command] ?? 0);
}

async function waitForInitialProjectFacts(tracePath) {
  const requiredOperations = new Set([
    "project_facts_git_status",
    "project_facts_agent_detection",
    "project_facts_provider_status",
  ]);
  const deadline = performance.now() + 35_000;
  while (performance.now() < deadline) {
    const spans = await readBlockingSpans(tracePath);
    for (const span of spans) requiredOperations.delete(span.operation);
    if (requiredOperations.size === 0) return;
    await delay(100);
  }
  throw new Error(`Initial Project Facts did not settle: ${[...requiredOperations].join(", ")}`);
}

function processLifecycleSummary(samples) {
  const appearances = new Map();
  for (const [sampleIndex, sample] of samples.entries()) {
    for (const item of sample.processes ?? []) {
      const current = appearances.get(item.processId) ?? {
        processId: item.processId,
        parentProcessId: item.parentProcessId,
        name: item.name,
        firstSample: sampleIndex,
        lastSample: sampleIndex,
      };
      current.lastSample = sampleIndex;
      appearances.set(item.processId, current);
    }
  }
  const observedProcesses = [...appearances.values()];
  const probeProcesses = observedProcesses.filter((item) => ["cmd.exe", "node.exe"].includes(item.name.toLowerCase()));
  const observedLifetime = (item) => (item.lastSample - item.firstSample + 1) * 1_000;
  return {
    observedProcesses,
    maximumObservedLifetimeMs: observedProcesses.length === 0
      ? null
      : Math.max(...observedProcesses.map(observedLifetime)),
    probeProcessCount: probeProcesses.length,
    maximumProbeObservedLifetimeMs: probeProcesses.length === 0
      ? null
      : Math.max(...probeProcesses.map(observedLifetime)),
  };
}

async function waitForTraceQuiet(tracePath) {
  const deadline = performance.now() + 15_000;
  let previousCount = -1;
  let stableSince = performance.now();
  while (performance.now() < deadline) {
    const count = (await readBlockingSpans(tracePath)).length;
    if (count !== previousCount) {
      previousCount = count;
      stableSince = performance.now();
    } else if (performance.now() - stableSince >= 750) {
      return;
    }
    await delay(100);
  }
  throw new Error("Blocking trace did not become quiet.");
}

async function systemEvidence(exe) {
  const script = String.raw`
$exe = $env:BATCH4_EXE
$version = (Get-Item -LiteralPath $exe).VersionInfo
$power = powercfg /getactivescheme
$display = @(Get-CimInstance Win32_VideoController | Where-Object { $_.CurrentRefreshRate } | Select-Object Name,CurrentHorizontalResolution,CurrentVerticalResolution,CurrentRefreshRate)
$disk = @(Get-PhysicalDisk | Select-Object FriendlyName,MediaType,BusType,HealthStatus,Size)
$defender = Get-MpComputerStatus -ErrorAction SilentlyContinue
[pscustomobject]@{
  productName=$version.ProductName
  productVersion=$version.ProductVersion
  fileVersion=$version.FileVersion
  powerScheme=($power -join ' ')
  displays=$display
  disks=$disk
  antivirus=[pscustomobject]@{ antivirusEnabled=$defender.AntivirusEnabled; realTimeProtectionEnabled=$defender.RealTimeProtectionEnabled; serviceEnabled=$defender.AMServiceEnabled }
} | ConvertTo-Json -Compress -Depth 5
`;
  const child = spawn("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", script], {
    env: { ...process.env, BATCH4_EXE: exe },
    windowsHide: true,
    stdio: ["ignore", "pipe", "pipe"],
  });
  return JSON.parse(await waitForChild(child, 15_000));
}

async function sampleProcessTree(rootProcessId, durationMs) {
  const samples = [];
  const deadline = performance.now() + durationMs;
  while (performance.now() < deadline) {
    const startedAt = performance.now();
    const powershell = spawn("powershell.exe", [
      "-NoProfile",
      "-NonInteractive",
      "-Command",
      `$root=${rootProcessId}; $all=@(Get-CimInstance Win32_Process); $ids=@($root); do { $before=$ids.Count; $children=@($all | Where-Object { $ids -contains [int]$_.ParentProcessId } | ForEach-Object { [int]$_.ProcessId }); $ids=@($ids+$children | Select-Object -Unique) } while ($ids.Count -gt $before); $selected=@($all | Where-Object { $ids -contains [int]$_.ProcessId }); $processes=@(Get-Process -Id $ids -ErrorAction SilentlyContinue); [pscustomobject]@{ timestamp=[DateTime]::UtcNow.ToString('o'); processCount=$processes.Count; workingSetBytes=($processes | Measure-Object WorkingSet64 -Sum).Sum; cpuSeconds=($processes | Measure-Object CPU -Sum).Sum; handles=($processes | Measure-Object HandleCount -Sum).Sum; readTransferBytes=($selected | Measure-Object ReadTransferCount -Sum).Sum; writeTransferBytes=($selected | Measure-Object WriteTransferCount -Sum).Sum; processes=@($selected | Select-Object @{n='processId';e={[int]$_.ProcessId}},@{n='parentProcessId';e={[int]$_.ParentProcessId}},@{n='name';e={$_.Name}}) } | ConvertTo-Json -Compress -Depth 4`,
    ], { windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
    try {
      samples.push(JSON.parse(await waitForChild(powershell, 5_000)));
    } catch (error) {
      samples.push({ error: error.message });
    }
    await delay(Math.max(0, 1_000 - (performance.now() - startedAt)));
  }
  return samples;
}

async function sampleWebView(client, durationMs, moveWindow, processId) {
  const browserSampling = client.evaluate(`(async () => {
    const durationMs = ${durationMs};
    const frameGaps = [];
    const longTasks = [];
    const inputToPaint = [];
    let previous = performance.now();
    let running = true;
    let frame = 0;
    const observer = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) longTasks.push(entry.duration);
    });
    observer.observe({ type: 'longtask', buffered: false });
    const tick = (now) => {
      frameGaps.push(now - previous);
      previous = now;
      if (running) frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    const startedAt = performance.now();
    while (performance.now() - startedAt < durationMs) {
      const inputAt = performance.now();
      document.body.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, clientX: 10, clientY: 10 }));
      await new Promise((resolve) => requestAnimationFrame(() => {
        inputToPaint.push(performance.now() - inputAt);
        resolve();
      }));
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    running = false;
    cancelAnimationFrame(frame);
    observer.disconnect();
    return { frameGaps, longTasks, inputToPaint };
  })()`, durationMs + 30_000);
  const processSampling = sampleProcessTree(processId, durationMs);
  const movement = moveWindow
    ? waitForChild(startNativeWindowMovement(processId, durationMs), durationMs + 10_000)
    : Promise.resolve('{"moves":0,"failedMoves":0,"restored":true}');
  const [browser, processes, nativeMovementText] = await Promise.all([
    browserSampling,
    processSampling,
    movement,
  ]);
  return {
    durationMs,
    nativeMovement: JSON.parse(nativeMovementText),
    frameGaps: summary(browser.frameGaps),
    frameGapsOver100Ms: browser.frameGaps.filter((value) => value > 100).length,
    longTasks: summary(browser.longTasks),
    longTasksOver50Ms: browser.longTasks.filter((value) => value > 50).length,
    inputToPaint: summary(browser.inputToPaint),
    processes,
  };
}

async function measureCommand(client, name, request, repetitions = 1) {
  return client.evaluate(`(async () => {
    const invoke = window.__TAURI_INTERNALS__?.invoke;
    if (typeof invoke !== 'function') throw new Error('Tauri invoke is unavailable.');
    const values = [];
    const frameGaps = [];
    const longTasks = [];
    let previousFrame = performance.now();
    let sampling = true;
    let frameHandle = 0;
    const observer = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) longTasks.push(entry.duration);
    });
    observer.observe({ type: 'longtask', buffered: false });
    const sampleFrame = (now) => {
      frameGaps.push(now - previousFrame);
      previousFrame = now;
      if (sampling) frameHandle = requestAnimationFrame(sampleFrame);
    };
    frameHandle = requestAnimationFrame(sampleFrame);
    let lastValue = null;
    let lastError = null;
    for (let index = 0; index < ${repetitions}; index += 1) {
      const startedAt = performance.now();
      try {
        lastValue = await invoke(${JSON.stringify(name)}, { request: ${JSON.stringify(request)} });
        lastError = null;
      } catch (error) {
        lastError = { code: error?.code ?? null, message: error?.message ?? String(error) };
      }
      values.push(performance.now() - startedAt);
    }
    await new Promise(requestAnimationFrame);
    sampling = false;
    cancelAnimationFrame(frameHandle);
    observer.disconnect();
    return { values, lastValue, lastError, frameGaps, longTasks };
  })()`);
}

function commandEvidence(measurement) {
  return {
    roundTrip: summary(measurement.values),
    frameGaps: summary(measurement.frameGaps),
    frameGapsOver100Ms: measurement.frameGaps.filter((value) => value > 100).length,
    longTasks: summary(measurement.longTasks),
    longTasksOver50Ms: measurement.longTasks.filter((value) => value > 50).length,
    error: measurement.lastError,
  };
}

async function activateNativeProject(client, nativeRoot) {
  const registered = await measureCommand(client, "open_project", { path: nativeRoot });
  const reloadStartedAt = performance.now();
  await client.command("Page.reload", { ignoreCache: false }).catch((error) => {
    if (!error.message.includes("Execution context was destroyed")) throw error;
  });
  await delay(500);
  await waitForInteractive(client);
  const autoOpened = await client.evaluate(`Boolean(document.querySelector('[data-app-view="dashboard"]'))`);
  if (autoOpened) {
    return { registered, uiOpenMs: performance.now() - reloadStartedAt, autoOpened: true };
  }
  const uiOpenMs = await client.evaluate(`(async () => {
    const expectedPath = ${JSON.stringify(nativeRoot)};
    const deadline = performance.now() + 20000;
    let card = null;
    while (!card) {
      card = Array.from(document.querySelectorAll('button.projcard')).find((item) => item.title === expectedPath);
      if (performance.now() > deadline) throw new Error('Recent project card did not appear.');
      if (!card) await new Promise(requestAnimationFrame);
    }
    const startedAt = performance.now();
    card.click();
    while (!document.querySelector('[data-app-view="dashboard"]')) {
      if (performance.now() > deadline) throw new Error('Project workbench did not become ready.');
      await new Promise(requestAnimationFrame);
    }
    await new Promise(requestAnimationFrame);
    return performance.now() - startedAt;
  })()`);
  return { registered, uiOpenMs, autoOpened: false };
}

async function runRoutes(client) {
  return client.evaluate(`(async () => {
    const routeIds = ['dashboard', 'wiki'];
    const samples = [];
    for (let index = 0; index < 20; index += 1) {
      const view = routeIds[index % routeIds.length];
      const button = document.querySelector('[data-app-view="' + view + '"]');
      if (!button) throw new Error('Route button missing: ' + view);
      const startedAt = performance.now();
      button.click();
      while (button.getAttribute('aria-current') !== 'page') await new Promise(requestAnimationFrame);
      await new Promise(requestAnimationFrame);
      samples.push(performance.now() - startedAt);
    }
    return samples;
  })()`);
}

async function runNativeFocusCycles(processId) {
  const script = String.raw`
$TargetProcessId = ${processId}
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class Batch4Focus {
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int command);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hWnd);
}
'@
$candidate = Get-Process -Id $TargetProcessId -ErrorAction SilentlyContinue
$handle = if ($candidate) { $candidate.MainWindowHandle } else { [IntPtr]::Zero }
if ($handle -eq [IntPtr]::Zero) { throw 'Native app window handle was unavailable.' }
$samples = @()
for ($index = 0; $index -lt 10; $index += 1) {
  $watch = [Diagnostics.Stopwatch]::StartNew()
  $minimizedCall = [Batch4Focus]::ShowWindow($handle, 6)
  Start-Sleep -Milliseconds 150
  $minimized = [Batch4Focus]::IsIconic($handle)
  $restoredCall = [Batch4Focus]::ShowWindow($handle, 9)
  $focusedCall = [Batch4Focus]::SetForegroundWindow($handle)
  Start-Sleep -Milliseconds 150
  $watch.Stop()
  $samples += [pscustomobject]@{ elapsedMs=$watch.Elapsed.TotalMilliseconds; minimizedCall=$minimizedCall; minimized=$minimized; restoredCall=$restoredCall; focusedCall=$focusedCall; focused=([Batch4Focus]::GetForegroundWindow() -eq $handle) }
}
$samples | ConvertTo-Json -Compress
`;
  const child = spawn("powershell.exe", [
    "-NoProfile",
    "-NonInteractive",
    "-Command",
    script,
  ], { windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
  const result = JSON.parse(await waitForChild(child, 15_000));
  const samples = Array.isArray(result) ? result : [result];
  if (samples.length !== 10 || samples.some((sample) => !sample.minimized || !sample.focused)) {
    throw new Error(`Native focus verification failed: ${JSON.stringify(samples)}`);
  }
  return samples;
}

async function terminateProcessTree(child) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  const killer = spawn("taskkill", ["/pid", String(child.pid), "/t", "/f"], {
    stdio: "ignore",
    windowsHide: true,
  });
  await Promise.race([
    new Promise((resolve) => killer.once("exit", resolve)),
    delay(5_000),
  ]);
}

async function main() {
  if (process.platform !== "win32") throw new Error("This installed acceptance harness requires Windows.");
  const exe = required("--exe");
  const installer = required("--installer");
  const builtExe = required("--built-exe");
  const provenancePath = required("--provenance");
  const sourceRepository = required("--source-repository");
  const sourceCommit = requiredText("--source-commit");
  const expectedVersion = requiredText("--expected-version");
  const fixtureRoot = required("--fixture-root");
  const output = required("--output");
  const trace = required("--trace");
  const appData = required("--app-data");
  const webviewData = required("--webview-data");
  if (!/^[0-9a-f]{40}$/u.test(sourceCommit)) throw new Error("--source-commit must be a full lowercase Git commit hash.");
  if (path.basename(path.dirname(exe)) !== "LLM Wiki Desktop") {
    throw new Error("--exe must be the per-user installed LLM Wiki Desktop executable.");
  }
  const port = Number(args.get("--port") ?? "9524");
  const idleMs = Number(args.get("--idle-ms") ?? "60000");
  const dragMs = Number(args.get("--drag-ms") ?? "30000");
  const nativeRoot = path.join(fixtureRoot, "native-git-3-pages");
  const markerlessRoot = path.join(fixtureRoot, "markerless-control");
  const fakeAgentBin = path.join(fixtureRoot, "fake-agent-slow-bin");
  const fixtureSettings = path.join(nativeRoot, ".app", "settings.json");
  const credentialDenialMarker = path.join(path.dirname(output), "credential-denial.marker");
  const originalFixtureSettings = await readFile(fixtureSettings);
  await stat(exe);
  await stat(installer);
  await stat(builtExe);
  const provenance = JSON.parse(await readFile(provenancePath, "utf8"));
  const artifactBinding = await verifyProjectFactsPackagedProvenance({
    provenance,
    repository: sourceRepository,
    sourceCommit,
    installer,
    builtExecutable: builtExe,
    installedExecutable: exe,
    expectedVersion,
  });
  const manifestBytes = await readFile(path.join(fixtureRoot, "fixture-manifest.json"));
  const fixtureManifest = JSON.parse(manifestBytes.toString("utf8"));
  await verifyProjectFactsPackagedFixtures(fixtureRoot, fixtureManifest);
  await requireAbsent("output", output);
  await requireAbsent("trace", trace);
  await requireAbsent("app data", appData);
  await requireAbsent("WebView data", webviewData);
  await requireAbsent("credential denial marker", credentialDenialMarker);
  try {
    await fetch(`http://127.0.0.1:${port}/json`, { signal: AbortSignal.timeout(500) });
    throw new Error(`CDP port ${port} is already in use.`);
  } catch (error) {
    if (error.message === `CDP port ${port} is already in use.`) throw error;
  }
  await mkdir(path.dirname(output), { recursive: true });
  await mkdir(appData, { recursive: true });
  await mkdir(webviewData, { recursive: true });

  const child = spawn(exe, [], {
    env: {
      ...process.env,
      APPDATA: appData,
      PATH: `${fakeAgentBin}${path.delimiter}${process.env.PATH ?? ""}`,
      WEBVIEW2_USER_DATA_FOLDER: webviewData,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
      LLM_WIKI_BLOCKING_TRACE_PATH: trace,
      LLM_WIKI_PERF_SECRET_DENIAL_MARKER: credentialDenialMarker,
    },
    stdio: "ignore",
    windowsHide: false,
  });
  let client;
  let pendingSecretRequest = null;
  let cleanupFailure = null;
  let runFailure = null;
  try {
    client = await CdpClient.connect(`http://127.0.0.1:${port}`);
    await waitForInteractive(client);
    const appProcessId = await resolveNativeWindowProcess(child.pid);
    await installInvokeObserver(client);
    const phases = [await phaseSnapshot(client, trace, "start")];
    const noProject = await sampleWebView(client, idleMs, false, appProcessId);
    phases.push(await phaseSnapshot(client, trace, "after_no_project_idle"));
    const noProjectDrag = await sampleWebView(client, dragMs, true, appProcessId);
    phases.push(await phaseSnapshot(client, trace, "after_no_project_drag"));
    const opening = await activateNativeProject(client, nativeRoot);
    if (opening.registered.lastError || !opening.registered.lastValue?.summary) {
      throw new Error(`Native project registration failed: ${JSON.stringify(opening.registered.lastError)}`);
    }
    await installInvokeObserver(client);
    await waitForInitialProjectFacts(trace);
    await waitForTraceQuiet(trace);
    phases.push(await phaseSnapshot(client, trace, "initial_project_facts_ready"));
    const project = opening.registered.lastValue.summary;
    const scope = { projectId: project.projectId, projectRootPath: project.rootPath };
    const setActiveProject = await measureCommand(client, "set_active_project", {
      projectId: project.projectId,
      rootPath: project.rootPath,
    }, 5);
    const listExports = await measureCommand(client, "list_exports", scope, 5);
    const gitStatus = await measureCommand(client, "git_status", { ...scope, forceRefresh: true });
    const [detectAgents, agentProcessSamples] = await Promise.all([
      measureCommand(client, "detect_agents", { ...scope, forceRefresh: true }),
      sampleProcessTree(appProcessId, 14_000),
    ]);
    const providersMissing = await measureCommand(client, "list_llm_providers", { ...scope, forceRefresh: true });
    const savedProvider = await measureCommand(client, "save_llm_provider", {
      ...scope,
      config: {
        provider: "open_ai",
        model: "batch4-fixture",
        baseUrl: "https://api.openai.com",
        contextWindow: 8_192,
        enabled: true,
      },
    });
    if (savedProvider.lastError || !savedProvider.lastValue?.credentialBinding) {
      throw new Error(`Temporary provider binding failed: ${JSON.stringify(savedProvider.lastError)}`);
    }
    const binding = savedProvider.lastValue.credentialBinding;
    const secretRequest = {
      ...scope,
      provider: "open_ai",
      configId: binding.configId,
      bindingRevision: binding.revision,
      expectedCanonicalOrigin: binding.canonicalOrigin,
      secret: "batch4-not-a-real-secret",
    };
    // Cleanup is safe when the secret was never written, so arm it before the
    // store call to cover transport failures after the backend has persisted it.
    pendingSecretRequest = secretRequest;
    const storedProvider = await measureCommand(client, "store_provider_secret", secretRequest);
    const providersNormal = await measureCommand(client, "list_llm_providers", { ...scope, forceRefresh: true });
    if (storedProvider.lastError || providersNormal.lastError) {
      throw new Error(`Temporary provider credential lifecycle failed: ${JSON.stringify({
        store: storedProvider.lastError,
        list: providersNormal.lastError,
      })}`);
    }
    const normalHasSecret = providersNormal.lastValue?.some((provider) => provider.hasSecret) ?? false;
    if (!normalHasSecret) throw new Error("Normal credential scenario did not observe the stored secret.");
    await writeFile(credentialDenialMarker, "deny\n", "utf8");
    const providersDenied = await measureCommand(client, "list_llm_providers", { ...scope, forceRefresh: true });
    await rm(credentialDenialMarker, { force: true });
    if (providersDenied.lastError?.code !== "SECRET_BACKEND_FAILED") {
      throw new Error(`Credential denial scenario returned ${JSON.stringify(providersDenied.lastError)}.`);
    }
    const deletedProvider = await measureCommand(client, "delete_provider_secret", secretRequest);
    if (!deletedProvider.lastError) pendingSecretRequest = null;
    if (deletedProvider.lastError) {
      throw new Error(`Temporary provider credential cleanup failed: ${JSON.stringify(deletedProvider.lastError)}.`);
    }
    phases.push(await phaseSnapshot(client, trace, "after_explicit_commands"));
    const projectIdle = await sampleWebView(client, idleMs, false, appProcessId);
    phases.push(await phaseSnapshot(client, trace, "after_project_idle"));
    const projectDrag = await sampleWebView(client, dragMs, true, appProcessId);
    phases.push(await phaseSnapshot(client, trace, "after_project_drag"));
    const routes = await runRoutes(client);
    await waitForTraceQuiet(trace);
    phases.push(await phaseSnapshot(client, trace, "after_routes"));
    const focusSamples = await runNativeFocusCycles(appProcessId);
    await waitForTraceQuiet(trace);
    phases.push(await phaseSnapshot(client, trace, "after_focus"));
    const markerless = await measureCommand(client, "open_project", { path: markerlessRoot });
    if (markerless.lastError?.code !== "PROJECT_OPEN_REQUIRES_ASSESSMENT") {
      throw new Error(`Markerless control returned ${JSON.stringify(markerless.lastError)}.`);
    }
    await waitForTraceQuiet(trace);
    phases.push(await phaseSnapshot(client, trace, "after_markerless_control"));
    for (const [name, measurement] of Object.entries({
      set_active_project: setActiveProject,
      list_exports: listExports,
      git_status: gitStatus,
      detect_agents: detectAgents,
      list_llm_providers_missing: providersMissing,
    })) {
      if (measurement.lastError) throw new Error(`${name} failed: ${JSON.stringify(measurement.lastError)}`);
    }
    const projectFactsOperations = [
      "project_facts_git_status",
      "project_facts_agent_detection",
      "project_facts_provider_status",
    ];
    const projectFactsCommands = ["git_status", "detect_agents", "list_llm_providers"];
    for (const phaseIndex of [1, 2]) {
      for (const operation of projectFactsOperations) {
        if (phaseDelta(phases[0].operationCounts, phases[phaseIndex].operationCounts, operation) !== 0) {
          throw new Error(`No-project phase ran ${operation}.`);
        }
      }
      for (const command of projectFactsCommands) {
        if (phaseDelta(phases[0].ipcCounts, phases[phaseIndex].ipcCounts, command) !== 0) {
          throw new Error(`No-project phase invoked ${command}.`);
        }
      }
    }
    const explicitPhase = phases.find((phase) => phase.label === "after_explicit_commands");
    for (const label of ["after_project_idle", "after_project_drag"]) {
      const phase = phases.find((candidate) => candidate.label === label);
      for (const operation of projectFactsOperations) {
        if (phaseDelta(explicitPhase.operationCounts, phase.operationCounts, operation) !== 0) {
          throw new Error(`${label} ran ${operation}.`);
        }
      }
      for (const command of projectFactsCommands) {
        if (phaseDelta(explicitPhase.ipcCounts, phase.ipcCounts, command) !== 0) {
          throw new Error(`${label} invoked ${command}.`);
        }
      }
    }
    const afterRoutes = phases.find((phase) => phase.label === "after_routes");
    const afterFocus = phases.find((phase) => phase.label === "after_focus");
    const focusGitIpc = phaseDelta(afterRoutes.ipcCounts, afterFocus.ipcCounts, "git_status");
    if (focusGitIpc > 10) throw new Error(`Focus Git IPC count was ${focusGitIpc}.`);
    // One git_status IPC intentionally emits two blocking-work spans: authority
    // resolution on MetadataIo and the repository read on ProjectGit. Count
    // each stage separately so the trace assertion does not double-count one
    // frontend refresh as two Git Facts invocations.
    for (const workClass of ["metadata_io", "project_git"]) {
      const operationClass = `project_facts_git_status:${workClass}`;
      const focusGitStages = phaseDelta(
        afterRoutes.operationClassCounts,
        afterFocus.operationClassCounts,
        operationClass,
      );
      if (focusGitStages > 10) {
        throw new Error(`Focus Git ${workClass} stage count was ${focusGitStages}.`);
      }
    }
    for (const operation of ["project_facts_agent_detection", "project_facts_provider_status"]) {
      if (phaseDelta(afterRoutes.operationCounts, afterFocus.operationCounts, operation) !== 0) {
        throw new Error(`Focus unexpectedly ran ${operation}.`);
      }
    }
    for (const command of ["detect_agents", "list_llm_providers"]) {
      if (phaseDelta(afterRoutes.ipcCounts, afterFocus.ipcCounts, command) !== 0) {
        throw new Error(`Focus unexpectedly invoked ${command}.`);
      }
    }
    for (const scenario of [noProjectDrag, projectDrag]) {
      if (scenario.nativeMovement.moves < 1 || scenario.nativeMovement.failedMoves !== 0 || !scenario.nativeMovement.restored) {
        throw new Error(`Native window movement failed: ${JSON.stringify(scenario.nativeMovement)}`);
      }
      if (scenario.frameGaps.p95Ms > 33.4 || scenario.frameGapsOver100Ms !== 0 || scenario.longTasksOver50Ms !== 0) {
        throw new Error(`Window-drag responsiveness budget failed: ${JSON.stringify(scenario)}`);
      }
    }
    if (summary(routes).p95Ms > 100) throw new Error(`Route p95 exceeded 100 ms: ${summary(routes).p95Ms}`);
    const spans = await readBlockingSpans(trace);
    const measuredOperations = ["open_project", "set_active_project", "list_exports"];
    const dispatchByOperation = {};
    for (const operation of measuredOperations) {
      const values = spans.filter((span) => span.operation === operation).map((span) => span.dispatchNanos / 1_000_000);
      dispatchByOperation[operation] = summary(values);
      if (values.length === 0 || dispatchByOperation[operation].p95Ms > 4 || dispatchByOperation[operation].maxMs >= 16) {
        throw new Error(`Native dispatch budget failed for ${operation}: ${JSON.stringify(dispatchByOperation[operation])}`);
      }
    }
    const lifecycle = processLifecycleSummary(agentProcessSamples);
    if (lifecycle.probeProcessCount < 1 || lifecycle.maximumProbeObservedLifetimeMs > 4_000) {
      throw new Error(`Fake-Agent process lifetime failed: ${JSON.stringify(lifecycle)}`);
    }
    const environment = await systemEvidence(exe);
    if (environment.productVersion !== expectedVersion || environment.fileVersion !== expectedVersion) {
      throw new Error(`Installed binary version mismatch: ${JSON.stringify(environment)}`);
    }
    const result = {
      schemaVersion: 1,
      status: "completed",
      measuredAt: new Date().toISOString(),
      artifact: {
        sourceCommit,
        sourceTree: artifactBinding.sourceTree,
        expectedVersion,
        installer,
        installerSha256: artifactBinding.installerEvidence.sha256,
        builtExe,
        builtExeSha256: artifactBinding.builtEvidence.sha256,
        exe,
        installedExeSha256: artifactBinding.installedEvidence.sha256,
        provenance: provenancePath,
        environment,
      },
      fixture: {
        root: fixtureRoot,
        manifestSha256: createHash("sha256").update(manifestBytes).digest("hex"),
        manifest: fixtureManifest,
      },
      scenarios: {
        noProject,
        noProjectDrag,
        opening: { uiOpenMs: opening.uiOpenMs, command: commandEvidence(opening.registered) },
        projectIdle,
        projectDrag,
        routes: summary(routes),
        focus: summary(focusSamples.map((sample) => sample.elapsedMs)),
        fakeAgent: {
          command: summary(detectAgents.values),
          processSamples: agentProcessSamples,
          lifecycle,
          error: detectAgents.lastError,
          detectedStateCounts: (detectAgents.lastValue ?? []).reduce((counts, agent) => {
            counts[agent.state] = (counts[agent.state] ?? 0) + 1;
            return counts;
          }, {}),
          configuredSleepMs: 5_000,
          appTimeoutMs: 3_000,
        },
        providers: {
          missing: { command: summary(providersMissing.values), error: providersMissing.lastError },
          normal: {
            status: "completed",
            command: summary(providersNormal.values),
            hasSecret: normalHasSecret,
            cleanupCompleted: deletedProvider.lastError === null,
          },
          denied: { status: "completed", command: commandEvidence(providersDenied), errorCode: providersDenied.lastError.code },
        },
        markerless: {
          command: summary(markerless.values),
          responseKind: markerless.lastValue?.kind ?? null,
          error: markerless.lastError,
        },
      },
      commands: {
        openProject: commandEvidence(opening.registered),
        setActiveProject: commandEvidence(setActiveProject),
        listExports: commandEvidence(listExports),
        gitStatus: commandEvidence(gitStatus),
        detectAgents: commandEvidence(detectAgents),
        listLlmProviders: commandEvidence(providersMissing),
      },
      nativeDispatch: dispatchByOperation,
      phases,
      blockingSpans: spans,
    };
    await writeFile(output, `${JSON.stringify(result, null, 2)}\n`, "utf8");
    process.stdout.write(`${JSON.stringify({
      status: result.status,
      output,
      spanCount: spans.length,
      projectDrag: result.scenarios.projectDrag,
      commands: result.commands,
    })}\n`);
  } catch (error) {
    runFailure = error;
  } finally {
    await rm(credentialDenialMarker, { force: true }).catch(() => undefined);
    if (client && pendingSecretRequest) {
      try {
        const cleanup = await measureCommand(client, "delete_provider_secret", pendingSecretRequest);
        if (cleanup.lastError) cleanupFailure = new Error(`Credential cleanup failed: ${JSON.stringify(cleanup.lastError)}`);
      } catch (error) {
        cleanupFailure = new Error(`Credential cleanup failed: ${error.message}`);
      }
    }
    client?.close();
    try {
      await terminateProcessTree(child);
      await writeFile(fixtureSettings, originalFixtureSettings);
    } catch (error) {
      cleanupFailure ??= new Error(`Fixture cleanup failed: ${error.message}`);
    }
  }
  if (runFailure && cleanupFailure) throw new AggregateError([runFailure, cleanupFailure], "Benchmark and cleanup both failed.");
  if (runFailure) throw runFailure;
  if (cleanupFailure) throw cleanupFailure;
}

await main();
