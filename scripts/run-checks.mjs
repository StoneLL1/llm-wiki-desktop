import { spawn } from "node:child_process";
import path from "node:path";
import process from "node:process";
import { performance } from "node:perf_hooks";

const repositoryRoot = path.join(import.meta.dirname, "..");
const serial = process.env.LLM_WIKI_CHECK_SERIAL === "1";
const activeChildren = new Set();

const fullLanes = [
  {
    name: "frontend",
    scripts: ["check:import-source-media", "test", "test:capability-tools", "lint", "build", "check:bundle", "check:console"],
  },
  {
    name: "rust",
    scripts: ["check:rust:gui", "test:rust"],
  },
];

const quickLanes = [
  {
    name: "frontend",
    scripts: ["lint", "build", "check:bundle", "check:console"],
  },
  {
    name: "rust",
    scripts: ["check:rust:core"],
  },
];

const requestedMode = process.argv[2] ?? "full";
if (!["full", "quick"].includes(requestedMode)) {
  process.stderr.write(
    `[check] unknown mode "${requestedMode}"; expected "full" or "quick"\n`,
  );
  process.exit(2);
}
const lanes = requestedMode === "quick" ? quickLanes : fullLanes;

const formatDuration = (milliseconds) => {
  const seconds = milliseconds / 1000;
  return seconds < 60
    ? `${seconds.toFixed(1)}s`
    : `${Math.floor(seconds / 60)}m ${(seconds % 60).toFixed(1)}s`;
};

const npmInvocation = (script) => {
  if (process.env.npm_execpath) {
    return {
      command: process.execPath,
      arguments: [process.env.npm_execpath, "run", script],
    };
  }

  if (process.platform === "win32") {
    return {
      command: process.env.ComSpec ?? "cmd.exe",
      arguments: ["/d", "/s", "/c", `npm run ${script}`],
    };
  }

  return {
    command: "npm",
    arguments: ["run", script],
  };
};

const runScript = (lane, script) => new Promise((resolve, reject) => {
  const startedAt = performance.now();
  const invocation = npmInvocation(script);
  process.stdout.write(`\n[check:${lane}] npm run ${script}\n`);

  const child = spawn(invocation.command, invocation.arguments, {
    cwd: repositoryRoot,
    env: process.env,
    shell: false,
    stdio: "inherit",
    windowsHide: true,
  });
  activeChildren.add(child);

  child.once("error", (error) => {
    activeChildren.delete(child);
    reject(new Error(`[check:${lane}] npm run ${script} could not start: ${error.message}`));
  });
  child.once("exit", (code, signal) => {
    activeChildren.delete(child);
    const duration = formatDuration(performance.now() - startedAt);
    if (code === 0) {
      process.stdout.write(`[check:${lane}] npm run ${script} passed in ${duration}\n`);
      resolve();
      return;
    }

    const reason = signal ? `signal ${signal}` : `exit code ${code ?? 1}`;
    reject(new Error(`[check:${lane}] npm run ${script} failed after ${duration} (${reason})`));
  });
});

const runLane = async ({ name, scripts }) => {
  const startedAt = performance.now();
  for (const script of scripts) {
    await runScript(name, script);
  }
  return {
    name,
    duration: performance.now() - startedAt,
  };
};

const stopChildren = (signal) => {
  for (const child of activeChildren) {
    child.kill(signal);
  }
};

process.once("SIGINT", () => stopChildren("SIGINT"));
process.once("SIGTERM", () => stopChildren("SIGTERM"));

const startedAt = performance.now();
const results = serial
  ? await (async () => {
      const completed = [];
      for (const lane of lanes) {
        try {
          completed.push({ status: "fulfilled", value: await runLane(lane) });
        } catch (reason) {
          completed.push({ status: "rejected", reason });
          break;
        }
      }
      return completed;
    })()
  : await Promise.allSettled(lanes.map(runLane));

const failures = results.filter(({ status }) => status === "rejected");
const totalDuration = formatDuration(performance.now() - startedAt);

for (const result of results) {
  if (result.status === "fulfilled") {
    process.stdout.write(
      `[check] ${result.value.name} lane completed in ${formatDuration(result.value.duration)}\n`,
    );
  } else {
    process.stderr.write(`${result.reason.message}\n`);
  }
}

if (failures.length > 0) {
  process.stderr.write(`[check] failed after ${totalDuration}\n`);
  process.exitCode = 1;
} else {
  process.stdout.write(
    `[check] ${requestedMode} mode passed in ${totalDuration}${serial ? " (serial)" : ""}\n`,
  );
}
