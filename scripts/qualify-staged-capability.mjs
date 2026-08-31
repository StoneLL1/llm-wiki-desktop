import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";

function option(name) {
  const index = process.argv.indexOf(`--${name}`);
  if (index < 0 || !process.argv[index + 1]) throw new Error(`--${name} is required`);
  return process.argv[index + 1];
}

function invoke(program, args, request, cwd) {
  return new Promise((resolve, reject) => {
    const child = spawn(program, args, { cwd, shell: false, windowsHide: true, stdio: ["pipe", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8"); child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.once("error", reject);
    child.once("exit", (code) => code === 0 ? resolve(stdout) : reject(new Error(`runner exited ${code}: ${stderr.slice(0, 500)}`)));
    child.stdin.end(JSON.stringify(request));
  });
}

const payload = path.resolve(option("payload"));
const contract = JSON.parse(await fs.readFile(path.join(payload, "CAPABILITY-CONTRACT.json"), "utf8"));
const program = path.join(payload, ...contract.entrypoint.split("/"));
for (const [index, route] of contract.routes.entries()) {
  const stdout = await invoke(program, contract.entrypointArgs, {
    jsonrpc: "2.0", id: index + 1, method: "capability.health",
    params: { protocolVersion: contract.protocolVersion, capabilityId: contract.capabilityId, route },
  }, payload);
  const response = stdout.trim().split(/\r?\n/u).map(JSON.parse).find((message) => message.id === index + 1);
  assert.equal(response?.error, null, `${contract.capabilityId}/${route} health failed`);
  assert.equal(response?.result?.healthy, true);
  assert.equal(response?.result?.route, route);
}
process.stdout.write(`${JSON.stringify({ qualified: true, capabilityId: contract.capabilityId, routes: contract.routes })}\n`);
