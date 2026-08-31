const DEFAULT_THRESHOLDS = Object.freeze({
  minimumSamplesPerRound: 100,
  maximumGraphUnmovedRatio: 0.1,
  maximumGraphLagP95Px: 12,
  maximumGraphLagMaxPx: 24,
  maximumGraphDashboardLagP95DeltaPx: 8,
  maximumGraphDashboardUnmovedRatioDelta: 0.05,
  maximumResizeEvents: 0,
  maximumGraphDragGetGraphDelta: 0,
  maximumTitlebarNormalizedFalseCount: 0,
  requiredAltTabGetGraphDelta: 1,
  maximumApplicationStallsOver100Ms: 0,
});

function percentile(values, quantile) {
  if (values.length === 0) return null;
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.ceil(quantile * sorted.length) - 1];
}

function isFiniteNumber(value) {
  return typeof value === "number" && Number.isFinite(value);
}

function isValidNativeSample(sample, expectedRoute) {
  return Number.isInteger(sample?.index)
    && isFiniteNumber(sample.timestampUnixMs)
    && isFiniteNumber(sample.targetCursorX)
    && isFiniteNumber(sample.targetCursorY)
    && isFiniteNumber(sample.actualCursorX)
    && isFiniteNumber(sample.actualCursorY)
    && isFiniteNumber(sample.expectedLeft)
    && isFiniteNumber(sample.expectedTop)
    && isFiniteNumber(sample.actualLeft)
    && isFiniteNumber(sample.actualTop)
    && isFiniteNumber(sample.errorX)
    && isFiniteNumber(sample.errorY)
    && isFiniteNumber(sample.euclideanError)
    && typeof sample.movedSincePrevious === "boolean"
    && sample.route === expectedRoute
    && Number.isInteger(sample.domFocusEventCount)
    && typeof sample.rawFocused === "boolean"
    && sample.rawFocusedSource === "Tauri tauri://focus and tauri://blur"
    && typeof sample.normalizedForeground === "boolean"
    && Number.isInteger(sample.getGraphPhaseCount)
    && Number.isInteger(sample.currentProcessId)
    && sample.currentProcessId > 0
    && Number.isInteger(sample.foregroundProcessId)
    && sample.foregroundProcessId > 0
    && sample.foregroundProcessId === sample.currentProcessId
    && Number.isInteger(sample.windowHwnd)
    && sample.windowHwnd > 0
    && Number.isInteger(sample.foregroundHwnd)
    && sample.foregroundHwnd > 0
    && sample.foregroundHwnd === sample.windowHwnd;
}

export function summarizeNativeRound(round, expectedRoute = round?.route) {
  const suppliedSamples = round?.native?.samples ?? [];
  const samples = suppliedSamples.filter((sample) => isValidNativeSample(sample, expectedRoute));
  const errors = samples.map((sample) => sample.euclideanError);
  const unmovedCount = samples.filter((sample) => sample.movedSincePrevious === false).length;
  return {
    sampleCount: samples.length,
    suppliedSampleCount: suppliedSamples.length,
    invalidSampleCount: suppliedSamples.length - samples.length,
    unmovedCount,
    unmovedRatio: samples.length === 0 ? null : unmovedCount / samples.length,
    lagP95Px: percentile(errors, 0.95),
    lagMaxPx: errors.length === 0 ? null : Math.max(...errors),
  };
}

function addFailure(failures, code, message, details = undefined) {
  failures.push({ code, message, ...(details === undefined ? {} : { details }) });
}

export function evaluateGraphTitlebarDragResult(result, thresholds = DEFAULT_THRESHOLDS) {
  const failures = [];
  if (result?.schemaVersion !== 1) {
    addFailure(failures, "SCHEMA_VERSION", "Result must use schemaVersion 1.");
  }
  if (result?.parameters?.dragStimulus !== "Win32 SendInput native non-client titlebar"
      || result?.parameters?.positionObserver !== "GetWindowRect"
      || result?.parameters?.resetUsesMoveWindowOutsideMeasurement !== true) {
    addFailure(failures, "STIMULUS", "Result must attest SendInput titlebar input and GetWindowRect observation.");
  }
  if (result?.parameters?.moveWindowUsedDuringMeasurement !== false) {
    addFailure(failures, "MOVE_WINDOW_MEASUREMENT", "MoveWindow data cannot be used as the measured stimulus.");
  }
  if (result?.observer?.getGraphObservationMode !== "cdp-function-call-breakpoint"
      || (result?.observer?.getGraphObservationErrors?.length ?? Number.POSITIVE_INFINITY) !== 0
      || result?.observer?.rawFocusSource !== "Tauri tauri://focus and tauri://blur"
      || result?.observer?.cdpPortPreflightUnused !== true
      || result?.observer?.cdpTargetBoundToSpawnedProcessEnvironment !== true
      || result?.observer?.cdpTarget?.type !== "page") {
    addFailure(failures, "OBSERVERS", "Raw Tauri focus and get_graph observers must be active without errors.");
  }
  const artifact = result?.artifact;
  if (!/^[0-9a-f]{40}$/u.test(artifact?.sourceCommit ?? "")
      || !/^[0-9a-f]{40}$/u.test(artifact?.sourceTree ?? "")
      || artifact?.sourceWorkingTreeClean !== true
      || artifact?.installedMatchesBuilt !== true
      || !artifact?.packageVersion
      || typeof artifact?.buildProvenance?.path !== "string"
      || !/^[0-9a-f]{64}$/u.test(artifact?.buildProvenance?.sha256 ?? "")
      || typeof artifact?.buildProvenance?.buildCommand !== "string"
      || !artifact.buildProvenance.buildCommand.includes("tauri")
      || ![artifact?.installer, artifact?.builtExecutable, artifact?.installedExecutable].every((item) =>
        typeof item?.path === "string" && item.path.length > 0
        && Number.isInteger(item?.size) && item.size > 0
        && /^[0-9a-f]{64}$/u.test(item?.sha256 ?? ""))) {
    addFailure(failures, "PROVENANCE", "Same-SHA source, installer, built EXE, and installed EXE provenance is incomplete.");
  } else if (artifact.builtExecutable.sha256 !== artifact.installedExecutable.sha256
      || artifact.builtExecutable.size !== artifact.installedExecutable.size) {
    addFailure(failures, "INSTALLED_BINARY", "Installed executable does not match the built executable.");
  }
  if (result?.environment?.productVersion !== artifact?.packageVersion
      || result?.environment?.fileVersion !== artifact?.packageVersion) {
    addFailure(failures, "PRODUCT_VERSION", "Installed executable version does not match the source package version.");
  }
  if (typeof result?.environment?.webView2RuntimeVersion !== "string"
      || result.environment.webView2RuntimeVersion.length === 0
      || !isFiniteNumber(result?.environment?.measuredWindowDpi)
      || !isFiniteNumber(result?.environment?.measuredWindowScalePercent)
      || !Array.isArray(result?.environment?.displays)
      || result.environment.displays.length === 0) {
    addFailure(failures, "ENVIRONMENT", "WebView2, window DPI/scale, and display evidence are required.");
  }

  const groups = result?.groups ?? {};
  const dashboardRounds = groups.dashboard?.rounds ?? [];
  const graphRounds = groups.graph?.rounds ?? [];
  if (dashboardRounds.length < 3 || graphRounds.length < 3) {
    addFailure(failures, "ROUND_COUNT", "Dashboard and Graph each require at least three rounds.", {
      dashboard: dashboardRounds.length,
      graph: graphRounds.length,
    });
  }

  const summaries = { dashboard: [], graph: [] };
  for (const [groupName, rounds] of [["dashboard", dashboardRounds], ["graph", graphRounds]]) {
    for (const [roundIndex, round] of rounds.entries()) {
      const summary = summarizeNativeRound(round, groupName);
      summaries[groupName].push(summary);
      const label = `${groupName} round ${roundIndex + 1}`;
      if (round?.native?.stimulus !== "send-input-native-titlebar-drag"
          || round?.native?.positionObserver !== "GetWindowRect"
          || round?.native?.moveWindowUsedDuringMeasurement !== false
          || round?.native?.mouseUpGuaranteedByFinally !== true
          || round?.native?.restoredWithMoveWindowAfterMeasurement !== true) {
        addFailure(failures, "ROUND_STIMULUS", `${label} is not a real SendInput/GetWindowRect titlebar round.`);
      }
      if (round?.route !== groupName || round?.actualRoute !== groupName) {
        addFailure(failures, "ROUND_ROUTE", `${label} did not measure the expected active route.`);
      }
      if (summary.invalidSampleCount > 0) {
        addFailure(failures, "INVALID_SAMPLES", `${label} contains malformed or misrouted native samples.`, summary);
      }
      if (summary.sampleCount < thresholds.minimumSamplesPerRound) {
        addFailure(failures, "SAMPLE_COUNT", `${label} has fewer than ${thresholds.minimumSamplesPerRound} valid HWND samples.`, summary);
      }
      if (summary.lagP95Px === null || summary.lagMaxPx === null) {
        addFailure(failures, "HWND_SAMPLES_MISSING", `${label} is missing HWND lag samples.`);
      }
      if ((round?.diagnostics?.resizeEvents ?? Number.POSITIVE_INFINITY) > thresholds.maximumResizeEvents) {
        addFailure(failures, "RESIZE", `${label} emitted resize events during fixed-size drag.`);
      }
      if ((round?.diagnostics?.normalizedForegroundFalseCount ?? Number.POSITIVE_INFINITY)
          > thresholds.maximumTitlebarNormalizedFalseCount) {
        addFailure(failures, "TITLEBAR_FOREGROUND_FALSE", `${label} emitted normalized foreground=false.`);
      }
      const rawFocusSequence = (round?.diagnostics?.rawFocusEvents ?? []).map((event) => event.focused);
      const normalizedSequence = (round?.diagnostics?.normalizedForegroundEvents ?? []).map((event) => event.foreground);
      if (!rawFocusSequence.includes(false) || rawFocusSequence.at(-1) !== true) {
        addFailure(failures, "RAW_FOCUS_SEQUENCE", `${label} did not capture raw Tauri false -> true focus churn.`);
      }
      if (normalizedSequence.length < 2 || normalizedSequence.some((foreground) => foreground !== true)) {
        addFailure(failures, "NORMALIZED_TITLEBAR_SEQUENCE", `${label} did not normalize titlebar churn to true -> true.`);
      }
      if ((round?.diagnostics?.applicationStallsOver100Ms ?? Number.POSITIVE_INFINITY)
          > thresholds.maximumApplicationStallsOver100Ms) {
        addFailure(failures, "APPLICATION_STALL", `${label} recorded an application-caused stall over 100 ms.`);
      }
      if (groupName === "graph" && (round?.diagnostics?.getGraphDelta ?? Number.POSITIVE_INFINITY)
          > thresholds.maximumGraphDragGetGraphDelta) {
        addFailure(failures, "GRAPH_DRAG_REFRESH", `${label} invoked get_graph during titlebar drag.`);
      }
      if (round?.diagnostics?.getGraphObserverMode !== "cdp-function-call-breakpoint"
          || (round?.diagnostics?.getGraphObserverErrors?.length ?? Number.POSITIVE_INFINITY) !== 0) {
        addFailure(failures, "ROUND_GET_GRAPH_OBSERVER", `${label} get_graph observer was unavailable or errored.`);
      }
      if (groupName === "graph" && summary.unmovedRatio !== null
          && summary.unmovedRatio > thresholds.maximumGraphUnmovedRatio) {
        addFailure(failures, "GRAPH_UNMOVED", `${label} exceeded the Graph unmoved-sample ratio.`, summary);
      }
      if (groupName === "graph" && summary.lagP95Px !== null
          && summary.lagP95Px > thresholds.maximumGraphLagP95Px) {
        addFailure(failures, "GRAPH_LAG_P95", `${label} exceeded the Graph native lag P95.`, summary);
      }
      if (groupName === "graph" && summary.lagMaxPx !== null
          && summary.lagMaxPx > thresholds.maximumGraphLagMaxPx) {
        addFailure(failures, "GRAPH_LAG_MAX", `${label} exceeded the Graph native lag maximum.`, summary);
      }
    }
  }

  const pairedRounds = Math.min(summaries.dashboard.length, summaries.graph.length);
  for (let index = 0; index < pairedRounds; index += 1) {
    const dashboard = summaries.dashboard[index];
    const graph = summaries.graph[index];
    if (dashboard.lagP95Px !== null && graph.lagP95Px !== null
        && graph.lagP95Px - dashboard.lagP95Px > thresholds.maximumGraphDashboardLagP95DeltaPx) {
      addFailure(failures, "GRAPH_DASHBOARD_LAG_DELTA", `Round ${index + 1} Graph P95 regressed against Dashboard.`, {
        dashboard: dashboard.lagP95Px,
        graph: graph.lagP95Px,
      });
    }
    if (dashboard.unmovedRatio !== null && graph.unmovedRatio !== null
        && graph.unmovedRatio - dashboard.unmovedRatio > thresholds.maximumGraphDashboardUnmovedRatioDelta) {
      addFailure(failures, "GRAPH_DASHBOARD_UNMOVED_DELTA", `Round ${index + 1} Graph unmoved ratio regressed against Dashboard.`, {
        dashboard: dashboard.unmovedRatio,
        graph: graph.unmovedRatio,
      });
    }
  }

  const altTab = result?.altTab;
  if (altTab?.stimulus !== "Win32 SendInput Alt-Tab"
      || !Array.isArray(altTab?.normalizedForegroundSequence)
      || !altTab.normalizedForegroundSequence.includes(false)
      || altTab.normalizedForegroundSequence.at(-1) !== true) {
    addFailure(failures, "ALT_TAB_FOREGROUND", "Alt-Tab must prove a normalized false -> true cycle.");
  }
  if (altTab?.native?.before?.processId !== altTab?.native?.returned?.processId
      || altTab?.native?.away?.processId === altTab?.native?.before?.processId
      || altTab?.native?.before?.processId !== altTab?.native?.targetProcessId
      || altTab?.native?.returned?.processId !== altTab?.native?.targetProcessId
      || altTab?.native?.before?.hwnd !== altTab?.native?.targetHwnd
      || altTab?.native?.returned?.hwnd !== altTab?.native?.targetHwnd
      || (altTab?.native?.away?.hwnd != null && altTab?.native?.away?.hwnd === altTab?.native?.targetHwnd)) {
    addFailure(failures, "ALT_TAB_NATIVE", "Native foreground PID evidence must prove a real process switch and return.");
  }
  if (altTab?.getGraphDelta !== thresholds.requiredAltTabGetGraphDelta) {
    addFailure(failures, "ALT_TAB_REFRESH", "Observed Graph must refresh exactly once after real Alt-Tab return.", {
      actual: altTab?.getGraphDelta,
      expected: thresholds.requiredAltTabGetGraphDelta,
    });
  }

  return {
    passed: failures.length === 0,
    thresholds,
    summaries,
    failures,
  };
}

export { DEFAULT_THRESHOLDS as GRAPH_TITLEBAR_DRAG_THRESHOLDS };
