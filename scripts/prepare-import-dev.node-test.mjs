import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { prepareImportDevelopment } from "./prepare-import-dev.mjs";

test("development catalog validates before replacing an existing prepared catalog", async (context) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "import-catalog-test-"));
  context.after(() => fs.rm(root, { recursive: true, force: true }));
  await fs.mkdir(path.join(root, "capabilities"));
  await fs.writeFile(path.join(root, "package.json"), JSON.stringify({ version: "0.2.0" }));
  await fs.writeFile(path.join(root, "capabilities/trusted-keys.json"), JSON.stringify({ publisher: "a".repeat(64) }));
  const entry = { capabilityId: "browser-runtime-lite", version: "1.0.0", targetTriple: "aarch64-apple-darwin",
    url: "https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v0.2.0/browser-runtime-lite-1.0.0-aarch64-apple-darwin.zip",
    archiveSha256: "b".repeat(64), manifestSha256: "c".repeat(64), signingKeyId: "publisher", compressedBytes: 100,
    installedBytes: 200, license: "Apache-2.0 AND MIT AND BSD-2-Clause AND BSD-3-Clause AND ISC AND MIT-0 AND LicenseRef-Bundled-Third-Party-Notices" };
  const fetchCatalog = (entries) => async () => ({ ok: true, text: async () => JSON.stringify({ schemaVersion: 1, entries }) });
  const destination = await prepareImportDevelopment({ root, fetchImpl: fetchCatalog([entry]) });
  const saved = await fs.readFile(path.join(destination, "install-catalog.json"), "utf8");
  for (const entries of [[], [{ ...entry, signingKeyId: "unknown-publisher" }]]) {
    await assert.rejects(prepareImportDevelopment({ root, fetchImpl: fetchCatalog(entries) }), /Invalid capability catalog/);
    assert.equal(await fs.readFile(path.join(destination, "install-catalog.json"), "utf8"), saved);
  }
  await assert.rejects(prepareImportDevelopment({ root, fetchImpl: async () => ({ ok: false, status: 503 }) }), /HTTP 503/);
  assert.equal(await fs.readFile(path.join(destination, "install-catalog.json"), "utf8"), saved);
});
