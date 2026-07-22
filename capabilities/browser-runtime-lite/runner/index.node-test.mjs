/* global process, URL */
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

test("extracts a staged offline HTML fixture through the JSON-RPC entrypoint", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "llm-wiki-browser-lite-"));
  try {
    const staging = path.join(root, "staging");
    fs.mkdirSync(staging);
    fs.writeFileSync(
      path.join(staging, "fetched.html"),
      "<!doctype html><html><head><title>Fixture article</title></head><body><article><h1>Fixture article</h1><p>This is a sufficiently long local article fixture used to qualify the offline extractor runtime.</p></article></body></html>",
    );
    const url = "https://example.test/article";
    const rpc = {
      jsonrpc: "2.0",
      id: "r1",
      method: "import.execute",
      params: {
        protocolVersion: "2",
        requestId: "r1",
        sessionId: "s",
        itemId: "i",
        taskId: "t",
        operation: "extract",
        input: { kind: "url", displayName: "fixture", locator: url, normalizedLocator: url, sourceIdentity: null },
        projectRoot: root,
        stagingRoot: "staging",
        chainedInput: "fetched.html",
      },
    };
    const runner = fileURLToPath(new URL("./index.mjs", import.meta.url));
    const result = spawnSync(process.execPath, [runner], {
      input: `${JSON.stringify(rpc)}\n`,
      encoding: "utf8",
      timeout: 30_000,
    });
    assert.equal(result.status, 0, result.stderr);
    const response = result.stdout
      .trim()
      .split(/\r?\n/)
      .map((line) => JSON.parse(line))
      .find((message) => message.id === "r1");
    assert.equal(response?.error, null);
    assert.equal(response?.result?.markdownPath, "candidate.md");
    assert.match(fs.readFileSync(path.join(staging, "candidate.md"), "utf8"), /Fixture article/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});
