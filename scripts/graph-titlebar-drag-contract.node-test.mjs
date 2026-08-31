import assert from "node:assert/strict";
import test from "node:test";

import { evaluateGraphTitlebarDragResult } from "./graph-titlebar-drag-contract.mjs";
import { assertGraphTitlebarDragProvenanceRecord } from "./graph-titlebar-drag-provenance.mjs";

function samples({ count = 112, unmoved = 2, lag = 0 } = {}) {
  return Array.from({ length: count }, (_, index) => ({
    index: index + 1,
    timestampUnixMs: 1_800_000_000_000 + index,
    targetCursorX: 500 + index,
    targetCursorY: 100 + index,
    actualCursorX: 500 + index,
    actualCursorY: 100 + index,
    expectedLeft: 200 + index,
    expectedTop: 100 + index,
    actualLeft: 200 + index,
    actualTop: 100 + index,
    errorX: 0,
    errorY: 0,
    euclideanError: lag,
    movedSincePrevious: index >= unmoved,
    route: "graph",
    domFocusEventCount: 0,
    rawFocused: true,
    rawFocusedSource: "Tauri tauri://focus and tauri://blur",
    normalizedForeground: true,
    getGraphPhaseCount: 0,
    currentProcessId: 41,
    foregroundProcessId: 41,
    windowHwnd: 101,
    foregroundHwnd: 101,
  }));
}

function round(options = {}, route = "graph") {
  const nativeSamples = samples(options).map((sample) => ({ ...sample, route }));
  return {
    route,
    actualRoute: route,
    native: {
      stimulus: "send-input-native-titlebar-drag",
      positionObserver: "GetWindowRect",
      moveWindowUsedDuringMeasurement: false,
      mouseUpGuaranteedByFinally: true,
      restoredWithMoveWindowAfterMeasurement: true,
      samples: nativeSamples,
    },
    diagnostics: {
      resizeEvents: 0,
      normalizedForegroundFalseCount: 0,
      rawFocusEvents: [{ focused: false }, { focused: true }],
      normalizedForegroundEvents: [{ foreground: true }, { foreground: true }],
      applicationStallsOver100Ms: 0,
      getGraphDelta: 0,
      getGraphObserverMode: "cdp-function-call-breakpoint",
      getGraphObserverErrors: [],
    },
  };
}

function fixture() {
  return {
    schemaVersion: 1,
    artifact: {
      sourceCommit: "a".repeat(40),
      sourceTree: "b".repeat(40),
      sourceWorkingTreeClean: true,
      packageVersion: "0.1.0",
      installer: { path: "candidate.msi", size: 10, sha256: "c".repeat(64) },
      builtExecutable: { path: "built.exe", size: 20, sha256: "d".repeat(64) },
      installedExecutable: { path: "installed.exe", size: 20, sha256: "d".repeat(64) },
      installedMatchesBuilt: true,
      buildProvenance: {
        path: "provenance.json",
        sha256: "e".repeat(64),
        buildCommand: "npm run tauri -- build --bundles msi",
      },
    },
    environment: {
      productVersion: "0.1.0",
      fileVersion: "0.1.0",
      webView2RuntimeVersion: "1.2.3.4",
      measuredWindowDpi: 144,
      measuredWindowScalePercent: 150,
      displays: [{ CurrentRefreshRate: 60 }],
    },
    parameters: {
      dragStimulus: "Win32 SendInput native non-client titlebar",
      positionObserver: "GetWindowRect",
      resetUsesMoveWindowOutsideMeasurement: true,
      moveWindowUsedDuringMeasurement: false,
    },
    observer: {
      getGraphObservationMode: "cdp-function-call-breakpoint",
      getGraphObservationErrors: [],
      rawFocusSource: "Tauri tauri://focus and tauri://blur",
      cdpPortPreflightUnused: true,
      cdpTargetBoundToSpawnedProcessEnvironment: true,
      cdpTarget: { type: "page" },
    },
    groups: {
      dashboard: { rounds: [round({}, "dashboard"), round({}, "dashboard"), round({}, "dashboard")] },
      graph: { rounds: [round(), round(), round()] },
    },
    altTab: {
      stimulus: "Win32 SendInput Alt-Tab",
      native: {
        targetProcessId: 41,
        targetHwnd: 101,
        controlProcessId: 42,
        before: { processId: 41, hwnd: 101 },
        away: { processId: 42 },
        returned: { processId: 41, hwnd: 101 },
      },
      normalizedForegroundSequence: [false, true],
      getGraphDelta: 1,
    },
  };
}

test("accepts the known good 2/112, P95 0 sample", () => {
  assert.equal(evaluateGraphTitlebarDragResult(fixture()).passed, true);
});

test("rejects the known bad 77/112, P95 152 sample", () => {
  const result = fixture();
  result.groups.graph.rounds = [round({ unmoved: 77, lag: 152 }), round({ unmoved: 77, lag: 152 }), round({ unmoved: 77, lag: 152 })];
  assert.equal(evaluateGraphTitlebarDragResult(result).passed, false);
});

test("rejects poor HWND following even with good rAF and no long tasks", () => {
  const result = fixture();
  result.groups.graph.rounds[0] = round({ unmoved: 0, lag: 25 });
  result.groups.graph.rounds[0].diagnostics.frameGapP95Ms = 16;
  result.groups.graph.rounds[0].diagnostics.longTasksOver100Ms = 0;
  assert.match(evaluateGraphTitlebarDragResult(result).failures.map((failure) => failure.code).join(" "), /GRAPH_LAG/u);
});

test("rejects missing HWND samples", () => {
  const result = fixture();
  result.groups.graph.rounds[0].native.samples = [];
  assert.equal(evaluateGraphTitlebarDragResult(result).passed, false);
});

test("rejects malformed entries mixed into an otherwise good HWND round", () => {
  const result = fixture();
  result.groups.graph.rounds[0].native.samples[3].actualLeft = Number.NaN;
  assert.ok(evaluateGraphTitlebarDragResult(result).failures.some((failure) => failure.code === "INVALID_SAMPLES"));
});

test("rejects fewer than 100 valid samples", () => {
  const result = fixture();
  result.groups.graph.rounds[0] = round({ count: 99, unmoved: 0 });
  assert.ok(evaluateGraphTitlebarDragResult(result).failures.some((failure) => failure.code === "SAMPLE_COUNT"));
});

test("rejects MoveWindow measurement data", () => {
  const result = fixture();
  result.parameters.dragStimulus = "Win32 MoveWindow";
  result.parameters.moveWindowUsedDuringMeasurement = true;
  result.groups.graph.rounds[0].native.stimulus = "move-window";
  assert.equal(evaluateGraphTitlebarDragResult(result).passed, false);
});

test("rejects a Graph regression when Dashboard still passes", () => {
  const result = fixture();
  result.groups.graph.rounds[1] = round({ unmoved: 10, lag: 9 });
  assert.ok(evaluateGraphTitlebarDragResult(result).failures.some((failure) =>
    failure.code === "GRAPH_DASHBOARD_LAG_DELTA" || failure.code === "GRAPH_DASHBOARD_UNMOVED_DELTA"));
});

test("rejects any Graph drag get_graph call", () => {
  const result = fixture();
  result.groups.graph.rounds[0].diagnostics.getGraphDelta = 1;
  assert.ok(evaluateGraphTitlebarDragResult(result).failures.some((failure) => failure.code === "GRAPH_DRAG_REFRESH"));
});

test("rejects normalized foreground=false during titlebar drag", () => {
  const result = fixture();
  result.groups.graph.rounds[0].diagnostics.normalizedForegroundFalseCount = 1;
  assert.ok(evaluateGraphTitlebarDragResult(result).failures.some((failure) => failure.code === "TITLEBAR_FOREGROUND_FALSE"));
});

test("requires exactly one Graph refresh after Alt-Tab return", () => {
  for (const delta of [0, 2]) {
    const result = fixture();
    result.altTab.getGraphDelta = delta;
    assert.ok(evaluateGraphTitlebarDragResult(result).failures.some((failure) => failure.code === "ALT_TAB_REFRESH"));
  }
});

test("rejects an Alt-Tab away snapshot that never left the target window", () => {
  const result = fixture();
  result.altTab.native.away = { processId: 41, hwnd: 101 };
  assert.ok(evaluateGraphTitlebarDragResult(result).failures.some((failure) => failure.code === "ALT_TAB_NATIVE"));
});

test("binds MSI and installed-build executable evidence to one clean source commit", () => {
  const expected = {
    sourceCommit: "a".repeat(40),
    sourceTree: "b".repeat(40),
    sourceHead: "a".repeat(40),
    sourceClean: true,
    packageVersion: "0.1.0",
    installer: { name: "candidate.msi", size: 10, sha256: "c".repeat(64) },
    builtExecutable: { name: "built.exe", size: 20, sha256: "d".repeat(64) },
  };
  const provenance = {
    schemaVersion: 1,
    source: { commitSha: expected.sourceCommit, treeSha: expected.sourceTree, clean: true },
    build: { packageVersion: expected.packageVersion, command: "npm run tauri -- build --bundles msi" },
    artifacts: { installer: expected.installer, builtExecutable: expected.builtExecutable },
  };
  assert.doesNotThrow(() => assertGraphTitlebarDragProvenanceRecord(provenance, expected));
  assert.throws(() => assertGraphTitlebarDragProvenanceRecord({
    ...provenance,
    artifacts: { ...provenance.artifacts, builtExecutable: { ...expected.builtExecutable, sha256: "e".repeat(64) } },
  }, expected), /does not match/u);
});
