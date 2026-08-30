import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import process from "node:process";

function option(name, required = true) {
  const index = process.argv.indexOf(`--${name}`);
  const value = index >= 0 ? process.argv[index + 1] : null;
  if (required && !value) throw new Error(`--${name} is required`);
  return value;
}

function invoke(program, args, request, cwd, timeout = 30 * 60 * 1000) {
  return new Promise((resolve, reject) => {
    const child = spawn(program, args, { cwd, shell: false, windowsHide: true, stdio: ["pipe", "pipe", "pipe"] });
    let stdout = ""; let stderr = ""; let settled = false;
    const timer = setTimeout(() => { child.kill("SIGKILL"); }, timeout);
    child.stdout.setEncoding("utf8"); child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.once("error", reject);
    child.once("exit", (code, signal) => {
      clearTimeout(timer); settled = true;
      resolve({ code, signal, stdout, stderr });
    });
    child.stdin.end(`${JSON.stringify(request)}\n`);
    void settled;
  });
}

function response(result, id) {
  return result.stdout.trim().split(/\r?\n/u).map((line) => {
    try { return JSON.parse(line); } catch { return null; }
  }).find((message) => message?.id === id);
}

function requestFor(projectRoot, relative, id) {
  return {
    jsonrpc: "2.0", id, method: "import.execute",
    params: {
      protocolVersion: "2", requestId: id, sessionId: "release-qualification",
      itemId: id, taskId: "release-qualification", operation: "extract",
      input: { kind: "file", displayName: relative, locator: relative, normalizedLocator: relative, sourceIdentity: null },
      projectRoot, stagingRoot: "staging", chainedInput: relative,
      localAsrAuthorized: true, localOcrAuthorized: true, asrProbeOnly: false,
      recognitionLanguage: "auto", asrProfile: "balanced",
    },
  };
}

async function assertContainedOutputs(message, staging) {
  assert.equal(message?.error, null, JSON.stringify(message?.error));
  for (const key of ["sourceSnapshotPath", "markdownPath", "metadataPath"]) {
    const value = message?.result?.[key];
    assert.equal(typeof value, "string", `${key} is missing`);
    const resolved = path.resolve(staging, value);
    assert.equal(resolved.startsWith(`${path.resolve(staging)}${path.sep}`), true, `${key} escapes staging`);
    assert.equal((await fs.stat(resolved).catch(() => null))?.isFile(), true, `${key} was not produced`);
  }
}

async function killForCancellation(program, args, request, cwd) {
  const child = spawn(program, args, { cwd, shell: false, windowsHide: true, stdio: ["pipe", "ignore", "ignore"] });
  child.stdin.end(`${JSON.stringify(request)}\n`);
  child.kill("SIGKILL");
  await new Promise((resolve) => child.once("exit", resolve));
}

const payload = path.resolve(option("payload"));
const evidencePath = option("evidence", false);
const repositoryRoot = path.resolve(import.meta.dirname, "..");
const [contract, corpus] = await Promise.all([
  fs.readFile(path.join(payload, "CAPABILITY-CONTRACT.json"), "utf8").then(JSON.parse),
  fs.readFile(path.join(repositoryRoot, "capabilities", "qualification-corpus.json"), "utf8").then(JSON.parse),
]);
const program = path.join(payload, ...contract.entrypoint.split("/"));
const corpusRoot = path.join(repositoryRoot, corpus.root);
const root = await fs.mkdtemp(path.join(os.tmpdir(), `llm-wiki-release-${contract.capabilityId}-`));
const cases = [];
try {
  const staging = path.join(root, "staging");
  await fs.mkdir(staging);
  for (const extension of contract.formats.extensions) {
    const fixture = corpus.fixtureByExtension[extension];
    assert.ok(fixture, `${extension} has no redistributable fixture`);
    const source = path.join(corpusRoot, fixture);
    assert.equal((await fs.stat(source)).isFile(), true);

    const normal = `normal.${extension}`;
    await fs.copyFile(source, path.join(staging, normal));
    const normalResult = await invoke(program, contract.entrypointArgs, requestFor(root, normal, `${extension}-normal`), payload);
    await assertContainedOutputs(response(normalResult, `${extension}-normal`), staging);
    cases.push({ extension, case: "normal", status: "passed" });

    const masquerade = `${extension}-masquerade.txt`;
    await fs.copyFile(source, path.join(staging, masquerade));
    const masqueradeResult = await invoke(program, contract.entrypointArgs, requestFor(root, masquerade, `${extension}-masquerade`), payload);
    assert.ok(response(masqueradeResult, `${extension}-masquerade`)?.error, `${extension} masquerade was accepted`);
    cases.push({ extension, case: "extension-masquerade", status: "passed" });

    const corrupt = `${extension}-corrupt.${extension}`;
    await fs.writeFile(path.join(staging, corrupt), Buffer.from([0, 255, 0, 255]));
    const corruptResult = await invoke(program, contract.entrypointArgs, requestFor(root, corrupt, `${extension}-corrupt`), payload);
    assert.ok(response(corruptResult, `${extension}-corrupt`)?.error, `${extension} corruption was accepted`);
    cases.push({ extension, case: "corrupt", status: "passed" });

    const boundaryDirectory = path.join(staging, "边界-qualification", extension);
    await fs.mkdir(boundaryDirectory, { recursive: true });
    const boundary = path.join("边界-qualification", extension, `source.${extension}`);
    await fs.copyFile(source, path.join(staging, boundary));
    const boundaryResult = await invoke(program, contract.entrypointArgs, requestFor(root, boundary, `${extension}-boundary`), payload);
    await assertContainedOutputs(response(boundaryResult, `${extension}-boundary`), staging);
    cases.push({ extension, case: "boundary", status: "passed" });

    await killForCancellation(program, contract.entrypointArgs, requestFor(root, normal, `${extension}-cancel`), payload);
    assert.equal((await fs.stat(path.join(staging, "candidate.md")).catch(() => null)), null);
    cases.push({ extension, case: "cancel", status: "passed" });
  }
  const evidence = {
    schemaVersion: 1, capabilityId: contract.capabilityId, targetTriple: contract.targetTriple,
    routes: contract.routes, formats: contract.formats, cases,
    networkCases: contract.formats.platformContentTypes.length ? corpus.networkCases : [],
  };
  if (evidencePath) await fs.writeFile(path.resolve(evidencePath), `${JSON.stringify(evidence, null, 2)}\n`, { flag: "wx" });
  process.stdout.write(`${JSON.stringify({ qualified: true, caseCount: cases.length, capabilityId: contract.capabilityId })}\n`);
} finally {
  await fs.rm(root, { recursive: true, force: true });
}
