/* global process, URL */
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

for (const [name, body, expectedError] of [
  ["ordinary article", '<article><h1>Fixture article</h1><p>This is a sufficiently long local article fixture used to qualify the offline extractor runtime.</p></article>', null],
  ["authentication vocabulary in an article", '<article><h1>Fixture article</h1><p>This article explains captcha challenges and login required protocols. 登录后可以阅读安全验证的技术说明。 It remains readable source content.</p></article>', null],
  ["actual challenge wall", '<form id="challenge-form">Please complete this captcha challenge</form>', "IMPORT_WEB_CHALLENGE_DETECTED"],
  ["actual login wall", '<div class="SignFlow">login required</div>', "IMPORT_WEB_LOGIN_REQUIRED"],
]) test(`extracts or classifies ${name} through the real JSON-RPC entrypoint`, () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "llm-wiki-browser-lite-"));
  try {
    const staging = path.join(root, "staging");
    fs.mkdirSync(staging);
    fs.writeFileSync(
      path.join(staging, "fetched.html"),
      `<!doctype html><html><head><title>Fixture article</title></head><body>${body}</body></html>`,
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
    if (expectedError) {
      assert.equal(response?.error?.data?.code, expectedError);
      assert.equal(fs.existsSync(path.join(staging, "candidate.md")), false);
      return;
    }
    assert.equal(response?.error, null);
    assert.equal(response?.result?.markdownPath, "candidate.md");
    assert.match(fs.readFileSync(path.join(staging, "candidate.md"), "utf8"), /Fixture article/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});
