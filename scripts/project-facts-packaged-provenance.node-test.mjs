import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

import {
  createProjectFactsPackagedProvenance,
  verifyProjectFactsPackagedProvenance,
} from "./project-facts-packaged-provenance.mjs";

function git(repository, args) {
  const result = spawnSync("git", ["-C", repository, ...args], {
    encoding: "utf8",
    windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr);
  return result.stdout.trim();
}

test("binds the installed executable and MSI evidence to one Git tree", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "project-facts-provenance-"));
  try {
    git(root, ["init", "--quiet", "--initial-branch=main"]);
    await writeFile(path.join(root, "source.txt"), "source\n");
    git(root, ["add", "source.txt"]);
    git(root, ["-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "--quiet", "-m", "fixture"]);
    const sourceCommit = git(root, ["rev-parse", "HEAD"]);
    const installer = path.join(root, "candidate.msi");
    const builtExecutable = path.join(root, "built.exe");
    const installedExecutable = path.join(root, "installed.exe");
    const output = path.join(root, "provenance.json");
    await writeFile(installer, "msi payload\n");
    await writeFile(builtExecutable, "same executable\n");
    await writeFile(installedExecutable, "same executable\n");
    const provenance = await createProjectFactsPackagedProvenance({
      repository: root,
      sourceCommit,
      installer,
      builtExecutable,
      expectedVersion: "0.1.0",
      output,
    });

    await verifyProjectFactsPackagedProvenance({
      provenance,
      repository: root,
      sourceCommit,
      installer,
      builtExecutable,
      installedExecutable,
      expectedVersion: "0.1.0",
    });
    await writeFile(installedExecutable, "stale executable\n");
    await assert.rejects(
      verifyProjectFactsPackagedProvenance({
        provenance,
        repository: root,
        sourceCommit,
        installer,
        builtExecutable,
        installedExecutable,
        expectedVersion: "0.1.0",
      }),
      /does not match/u,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
