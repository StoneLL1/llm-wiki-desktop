import { readFile, readdir, stat } from "node:fs/promises";
import path from "node:path";

const root = process.cwd();
const evidencePath = "docs/qa/import-source-media-flow-batch9-evidence.json";
const failures = [];

const expectedIds = (prefix, count) =>
  Array.from({ length: count }, (_, index) => `${prefix}${String(index + 1).padStart(2, "0")}`);

const readText = async (relative) => readFile(path.join(root, relative), "utf8");

let evidence;
try {
  evidence = JSON.parse(await readText(evidencePath));
} catch (error) {
  failures.push(`Cannot read ${evidencePath}: ${error.message}`);
}

const sourceCache = new Map();
const source = async (relative) => {
  if (!sourceCache.has(relative)) {
    sourceCache.set(relative, await readText(relative));
  }
  return sourceCache.get(relative);
};

const escapeRegExp = (value) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

const expandReferencedTestData = (file, body, block) => {
  if (!file.endsWith(".rs")) return block;
  const pending = [...block.matchAll(/\b[A-Z][A-Z0-9_]{2,}\b/g)].map(
    (match) => match[0],
  );
  const referencedConstants = new Set();
  const supportingBlocks = [];
  while (pending.length > 0) {
    const name = pending.shift();
    if (referencedConstants.has(name)) continue;
    referencedConstants.add(name);
    const declaration = new RegExp(
      String.raw`(?:^|\n)\s*(?:pub(?:\([^)]*\))?\s+)?const\s+${escapeRegExp(name)}\b`,
      "m",
    ).exec(body);
    if (!declaration) continue;
    const rest = body.slice(declaration.index + declaration[0].length);
    const next = /(?:^|\n)\s*(?:(?:pub(?:\([^)]*\))?\s+)?const\s+[A-Z][A-Z0-9_]*\b|#\[[^\]]*test[^\]]*\])/m.exec(rest);
    const supportingBlock = body.slice(
      declaration.index,
      next ? declaration.index + declaration[0].length + next.index : body.length,
    );
    supportingBlocks.push(supportingBlock);
    pending.push(
      ...[...supportingBlock.matchAll(/\b[A-Z][A-Z0-9_]{2,}\b/g)].map(
        (match) => match[0],
      ),
    );
  }
  return [block, ...supportingBlocks].join("\n");
};

const executableTestBlock = (file, anchor, body) => {
  const escaped = escapeRegExp(anchor);
  let declaration;
  let nextDeclaration;
  if (file.endsWith(".rs")) {
    declaration = new RegExp(
      String.raw`#\[[^\]]*test[^\]]*\]\s*(?:async\s+)?fn\s+${escaped}\s*\(`,
    );
    nextDeclaration = /#\[[^\]]*test[^\]]*\]\s*(?:async\s+)?fn\s+[A-Za-z0-9_]+\s*\(/g;
  } else if (/\.(?:[cm]?[jt]sx?)$/.test(file)) {
    declaration = new RegExp(
      String.raw`\b(?:it|test)(?:\.each\([\s\S]{0,20000}?\))?\s*\(\s*["'\`]${escaped}`,
    );
    nextDeclaration = /(?:^|\n)\s*(?:it|test)(?:\.each)?\s*[.(]/g;
  } else {
    return null;
  }
  const match = declaration.exec(body);
  if (!match) return null;
  nextDeclaration.lastIndex = match.index + match[0].length;
  const next = nextDeclaration.exec(body);
  return expandReferencedTestData(
    file,
    body,
    body.slice(match.index, next?.index ?? body.length),
  );
};

const validateReferences = async (label, id, references) => {
  if (!Array.isArray(references) || references.length === 0) {
    failures.push(`${label} ${id} has no executable test reference`);
    return [];
  }
  const bodies = [];
  for (const reference of references) {
    if (!Array.isArray(reference) || reference.length !== 2) {
      failures.push(`${label} ${id} has an invalid test reference`);
      continue;
    }
    const [file, anchor] = reference;
    try {
      const body = await source(file);
      const block = executableTestBlock(file, anchor, body);
      if (!block) {
        failures.push(`${label} ${id} is not an executable test declaration: ${file} :: ${anchor}`);
      } else {
        bodies.push({ file, body: block });
      }
    } catch (error) {
      failures.push(`${label} ${id} test file missing: ${file} (${error.message})`);
    }
  }
  return bodies;
};

const validateRows = async (label, rows, expected) => {
  const actual = Object.keys(rows ?? {}).sort();
  if (JSON.stringify(actual) !== JSON.stringify([...expected].sort())) {
    failures.push(`${label} IDs must be exactly ${expected.join(", ")}; found ${actual.join(", ")}`);
    return;
  }
  for (const [id, row] of Object.entries(rows)) {
    if (!String(row.requirement ?? row.category ?? "").trim()) {
      failures.push(`${label} ${id} has no requirement/category`);
    }
    await validateReferences(label, id, row.tests);
  }
};

const validateProductionPipelineBoundary = async () => {
  const file = "src-tauri/tests/import_v2_format_pipeline.rs";
  const body = await source(file);
  const block = executableTestBlock(
    file,
    "every_supported_local_format_runs_discovery_route_execution_candidate_and_commit",
    body,
  );
  if (!block) {
    failures.push(`production format pipeline test is missing from ${file}`);
    return;
  }
  for (const token of [
    "register_capability_pack(",
    "pack.batch9-runtime-fixture.pack.office-legacy",
    "pack.batch9-runtime-fixture.ocr.cjk-accurate",
    "pack.batch9-runtime-fixture.media.asr",
    "builtin.native-file",
    "builtin.office-xlsx",
    "builtin.local-media-companion",
    ".commit_items(",
    "let manifest: SourceManifest",
    ".raw_evidence",
    "\"source_snapshot\"",
    "committed_bytes, fixture_bytes",
    "raw_snapshot.sha256",
  ]) {
    if (!block.includes(token)) {
      failures.push(`production format pipeline is missing required boundary evidence: ${token}`);
    }
  }
  for (const token of [
    "register_engine(",
    "ExternalCapabilityFixtureEngine",
    "ProductionMatrixEngine",
    "impl ImportEngine for",
  ]) {
    if (body.includes(token)) {
      failures.push(`production format pipeline bypasses a production engine boundary: ${token}`);
    }
  }
  for (const token of [
    "ResolvedCapabilityPack",
    "JsonRpcRequest<EngineRequest>",
    "JsonRpcResponse",
    "batch9_capability_runner_process",
    "run_legacy_office_capability",
    "run_ocr_capability",
    "run_asr_capability",
    "identify_file(",
  ]) {
    if (!body.includes(token)) {
      failures.push(`capability fixture does not exercise the runtime protocol/input boundary: ${token}`);
    }
  }
};

if (evidence) {
  if (evidence.schemaVersion !== 1) failures.push("Evidence schemaVersion must be 1");
  if (evidence.authority !== "docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md") {
    failures.push("Evidence authority must point to the sole Import / Source product specification");
  }

  await validateRows("design scenario", evidence.designScenarios, expectedIds("S", 32));
  await validateRows("contract", evidence.contractTests, expectedIds("C", 26));
  await validateRows("format fixture", evidence.formatFixtures, expectedIds("F", 14));
  await validateRows("forbidden closure", evidence.forbiddenClosures, expectedIds("N", 9));
  await validateProductionPipelineBoundary();

  for (const [id, row] of Object.entries(evidence.formatFixtures ?? {})) {
    if (!Array.isArray(row.fixtures) || row.fixtures.length === 0) {
      failures.push(`format fixture ${id} has no real fixture`);
      continue;
    }
    const pipelineBodies = await validateReferences(
      "format fixture pipeline",
      id,
      row.pipelineTests,
    );
    await validateReferences("format fixture discovery", id, row.tests);
    for (const fixture of row.fixtures) {
      try {
        const metadata = await stat(path.join(root, fixture));
        if (!metadata.isFile() || metadata.size === 0) {
          failures.push(`format fixture ${id} is not a non-empty file: ${fixture}`);
        }
        const basename = path.posix.basename(fixture.replaceAll("\\", "/"));
        if (!pipelineBodies.some(({ body }) => body.includes(basename))) {
          failures.push(`format fixture ${id} is not consumed by its production pipeline tests: ${fixture}`);
        }
      } catch (error) {
        failures.push(`format fixture ${id} is missing: ${fixture} (${error.message})`);
      }
    }
  }

  for (const check of evidence.absenceChecks ?? []) {
    try {
      const body = await source(check.file);
      for (const token of check.tokens ?? []) {
        if (body.includes(token)) failures.push(`forbidden legacy token remains in ${check.file}: ${token}`);
      }
    } catch (error) {
      failures.push(`absence-check file missing: ${check.file} (${error.message})`);
    }
  }
}

try {
  const importFiles = (await readdir(path.join(root, "src/features"), { recursive: true }))
    .map((name) => String(name).replaceAll("\\", "/"))
    .filter((name) => name.endsWith(".tsx") && !name.endsWith(".test.tsx"));
  const migrationCallers = [];
  for (const name of importFiles) {
    const relative = `src/features/${name}`;
    if ((await source(relative)).includes("<ImportMigrationDialog")) migrationCallers.push(relative);
  }
  const expected = ["src/features/settings/ImportCompatibilitySettings.tsx"];
  if (JSON.stringify(migrationCallers.sort()) !== JSON.stringify(expected)) {
    failures.push(`ImportMigrationDialog must be reachable only from Settings; found ${migrationCallers.join(", ")}`);
  }
} catch (error) {
  failures.push(`Cannot verify the Settings-only migration boundary: ${error.message}`);
}

const report = {
  passed: failures.length === 0,
  counts: {
    designScenarios: Object.keys(evidence?.designScenarios ?? {}).length,
    contracts: Object.keys(evidence?.contractTests ?? {}).length,
    fixtureCategories: Object.keys(evidence?.formatFixtures ?? {}).length,
    forbiddenClosures: Object.keys(evidence?.forbiddenClosures ?? {}).length,
  },
  failures,
  readOnly: true,
  execution:
    "This gate validates evidence declarations and production-boundary wiring; npm run check executes every referenced suite.",
};

if (process.argv.includes("--json")) {
  console.log(JSON.stringify(report, null, 2));
} else if (report.passed) {
  console.log(`Import / Source Batch 9 evidence declarations passed: ${report.counts.designScenarios} scenarios, ${report.counts.contracts} contracts, ${report.counts.fixtureCategories} real-fixture categories, ${report.counts.forbiddenClosures} forbidden-closure checks. npm run check establishes executable pass results.`);
} else {
  console.error("Import / Source Batch 9 evidence failed:");
  for (const failure of failures) console.error(`- ${failure}`);
}

if (!report.passed) process.exitCode = 1;
