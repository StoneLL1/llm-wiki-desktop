import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

import {
  prepareProjectFactsPackagedFixtures,
  verifyProjectFactsPackagedFixtures,
} from "./prepare-project-facts-packaged-fixtures.mjs";

test("prepares deterministic native, markerless, and fake-Agent fixtures", async () => {
  const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), "llm-wiki-project-facts-"));
  const outputRoot = path.join(temporaryRoot, "fixture");
  try {
    const manifest = await prepareProjectFactsPackagedFixtures(outputRoot);
    assert.equal(manifest.native.wikiPages, 3);
    assert.equal(manifest.native.supportFiles, 240);
    assert.equal(manifest.native.trackedFiles, 251);
    assert.equal(manifest.schemaVersion, 2);
    assert.equal(manifest.fileInventory.length, 281);
    assert.match(manifest.fixtureHash, /^[0-9a-f]{64}$/u);
    await verifyProjectFactsPackagedFixtures(outputRoot, manifest);
    assert.equal(manifest.markerless.markdownFiles, 3);
    await assert.rejects(stat(path.join(outputRoot, "markerless-control", ".git")));
    await assert.rejects(stat(path.join(outputRoot, "markerless-control", ".app")));

    const tracked = spawnSync(
      "git",
      ["-C", path.join(outputRoot, "native-git-3-pages"), "ls-files"],
      { encoding: "utf8", windowsHide: true },
    );
    assert.equal(tracked.status, 0);
    assert.equal(tracked.stdout.trim().split(/\r?\n/u).length, 251);
    assert.equal(manifest.native.initialBranch, "main");
    assert.equal(manifest.native.gitTree, "6f9dc5508efda511d3d2ee3da1775daeed0daf80");
    for (const directory of manifest.native.requiredDirectories) {
      assert.equal(
        (await stat(path.join(outputRoot, "native-git-3-pages", directory))).isDirectory(),
        true,
      );
    }
    assert.deepEqual(
      JSON.parse(await readFile(
        path.join(outputRoot, "native-git-3-pages", ".app", "graph-cache.json"),
        "utf8",
      )),
      {
        nodes: [],
        edges: [],
        contentHash: "",
        builtAt: "2026-08-28T00:00:00Z",
      },
    );
    const cleanEnvironment = Object.fromEntries(Object.entries({
      PATH: process.env.PATH,
      Path: process.env.Path,
      PATHEXT: process.env.PATHEXT,
      SystemRoot: process.env.SystemRoot,
      ComSpec: process.env.ComSpec,
    }).filter(([, value]) => value !== undefined));
    const wrapperName = process.platform === "win32" ? "claude.cmd" : "claude";
    const runFakeAgent = (mode) => {
      const command = path.join(outputRoot, `fake-agent-${mode}-bin`, wrapperName);
      const options = {
        encoding: "utf8",
        windowsHide: true,
        shell: process.platform === "win32",
        env: cleanEnvironment,
      };
      return process.platform === "win32"
        ? spawnSync(command, options)
        : spawnSync(command, ["--help"], options);
    };
    assert.equal(runFakeAgent("healthy").status, 0);
    assert.equal(runFakeAgent("fail").status, 23);
    const slow = spawnSync(
      process.execPath,
      [path.join(outputRoot, "fake-agent-slow-bin", "fake-agent.mjs"), "slow", "--help"],
      { encoding: "utf8", windowsHide: true, timeout: 100, env: cleanEnvironment },
    );
    assert.equal(slow.error?.code, "ETIMEDOUT");
    assert.match(
      await readFile(path.join(outputRoot, "fake-agent-slow-bin", wrapperName), "utf8"),
      /fake-agent\.mjs["']? slow/u,
    );
    await writeFile(
      path.join(outputRoot, "fake-agent-slow-bin", "fake-agent.mjs"),
      "tampered\n",
      "utf8",
    );
    await assert.rejects(
      verifyProjectFactsPackagedFixtures(outputRoot, manifest),
      /no longer match/u,
    );
  } finally {
    await rm(temporaryRoot, { recursive: true, force: true });
  }
});
