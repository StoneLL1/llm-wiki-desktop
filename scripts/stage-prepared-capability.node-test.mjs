import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { stagePreparedCapability } from "./stage-prepared-capability.mjs";

test("stages a manifest-derived signed payload contract without links", async () => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-prepared-stage-"));
  try {
    const prepared = path.join(root, "prepared");
    const output = path.join(root, "output");
    await fs.mkdir(path.join(prepared, "runtime"), { recursive: true });
    await Promise.all([
      fs.writeFile(path.join(prepared, "runtime", "node.exe"), "runtime"),
      fs.writeFile(path.join(prepared, "NOTICE.md"), "notice"),
      fs.writeFile(path.join(prepared, "SBOM.spdx.json"), "{}"),
      fs.writeFile(path.join(prepared, "BUILD-PROVENANCE.json"), JSON.stringify({
        packId: "media-metadata", target: "x86_64-pc-windows-msvc", runtimeNetwork: false,
      })),
    ]);
    const result = await stagePreparedCapability({
      pack: "media-metadata", target: "x86_64-pc-windows-msvc",
      preparedRoot: prepared, output, entrypoint: "runtime/node.exe", entrypointArgs: ["runner/index.mjs"],
    });
    assert.deepEqual(result.contract.routes, ["web.bilibili.metadata"]);
    assert.deepEqual(result.contract.formats.extensions, []);
    assert.equal(result.contract.targetTriple, "x86_64-pc-windows-msvc");
    assert.equal(JSON.parse(await fs.readFile(path.join(output, "CAPABILITY-CONTRACT.json"), "utf8")).capabilityId, "media-metadata");
  } finally {
    await fs.rm(root, { recursive: true, force: true });
  }
});
