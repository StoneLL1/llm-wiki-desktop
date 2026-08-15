import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { pathToFileURL } from "node:url";

const defaultGraphPath = path.join("dist", "bundle-graph.json");
const defaultBudgetPath = path.join("scripts", "bundle-budget.json");

const formatBytes = (bytes) => `${bytes.toLocaleString("en-US")} B`;

export const evaluateBundleBudget = (graph, budget) => {
  if (graph.schemaVersion !== 1 || !graph.initial?.js) {
    throw new Error("Unsupported or incomplete bundle graph; run `npm run build` again.");
  }

  const actual = {
    initialJsRawBytes: graph.initial.js.rawBytes,
    initialJsGzipBytes: graph.initial.js.gzipBytes,
    initialJsFiles: graph.initial.js.fileCount,
  };
  const checks = [
    ["initialJsRawBytes", "maxInitialJsRawBytes"],
    ["initialJsGzipBytes", "maxInitialJsGzipBytes"],
    ["initialJsFiles", "maxInitialJsFiles"],
  ];
  const violations = checks
    .filter(([, limitName]) => !Number.isFinite(budget[limitName]))
    .map(([, limitName]) => ({
      metric: limitName,
      error: `Missing numeric budget ${limitName}`,
    }));

  for (const [actualName, limitName] of checks) {
    if (Number.isFinite(budget[limitName]) && actual[actualName] > budget[limitName]) {
      violations.push({
        metric: actualName,
        actual: actual[actualName],
        limit: budget[limitName],
      });
    }
  }

  return { actual, violations };
};

const metricLabel = (metric) => ({
  initialJsRawBytes: "initial JS raw bytes",
  initialJsGzipBytes: "initial JS gzip bytes",
  initialJsFiles: "initial JS file count",
}[metric] ?? metric);

export const formatBudgetFailure = (graph, result, contributorLimit = 10) => {
  const lines = ["Initial bundle budget failed:"];
  for (const violation of result.violations) {
    if (violation.error) {
      lines.push(`- ${violation.error}`);
      continue;
    }
    const formatter = violation.metric === "initialJsFiles"
      ? (value) => value.toLocaleString("en-US")
      : formatBytes;
    lines.push(
      `- ${metricLabel(violation.metric)}: ${formatter(violation.actual)} (limit ${formatter(violation.limit)})`,
    );
  }

  lines.push("Largest statically reachable module sources:");
  for (const contributor of graph.initial.moduleContributors.slice(0, contributorLimit)) {
    lines.push(
      `- ${contributor.moduleId}: ${formatBytes(contributor.renderedBytes)} in ${contributor.chunks.join(", ")}`,
    );
  }
  return lines.join("\n");
};

const parseArguments = (arguments_) => {
  const options = {
    graphPath: defaultGraphPath,
    budgetPath: defaultBudgetPath,
  };
  for (let index = 0; index < arguments_.length; index += 1) {
    if (arguments_[index] === "--graph") {
      options.graphPath = arguments_[index + 1];
      index += 1;
    } else if (arguments_[index] === "--budget") {
      options.budgetPath = arguments_[index + 1];
      index += 1;
    } else {
      throw new Error(`Unknown argument: ${arguments_[index]}`);
    }
  }
  return options;
};

const run = async () => {
  const options = parseArguments(process.argv.slice(2));
  const [graph, budget] = await Promise.all([
    fs.readFile(options.graphPath, "utf8").then(JSON.parse),
    fs.readFile(options.budgetPath, "utf8").then(JSON.parse),
  ]);
  const result = evaluateBundleBudget(graph, budget);

  if (result.violations.length > 0) {
    process.stderr.write(`${formatBudgetFailure(graph, result)}\n`);
    process.exitCode = 1;
    return;
  }

  process.stdout.write([
    "Initial bundle budget passed:",
    `- JS raw: ${formatBytes(result.actual.initialJsRawBytes)} / ${formatBytes(budget.maxInitialJsRawBytes)}`,
    `- JS gzip: ${formatBytes(result.actual.initialJsGzipBytes)} / ${formatBytes(budget.maxInitialJsGzipBytes)}`,
    `- JS files: ${result.actual.initialJsFiles.toLocaleString("en-US")} / ${budget.maxInitialJsFiles.toLocaleString("en-US")}`,
    `- CSS raw/gzip: ${formatBytes(graph.initial.css.rawBytes)} / ${formatBytes(graph.initial.css.gzipBytes)}`,
    "",
  ].join("\n"));
};

const isMain = process.argv[1]
  && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url;
if (isMain) {
  await run();
}
