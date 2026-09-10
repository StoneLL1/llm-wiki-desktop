import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { promisify } from "node:util";

import { prepareMacosRuntime } from "./prepare-macos-runtime.mjs";
import { copyTreePreservingExecutables } from "./prepare-release-capability.mjs";

const run = promisify(execFile);
const macosOnly = { skip: process.platform !== "darwin" };

test("flattened frameworks retain valid Mach-O signatures and repair unsigned copies", macosOnly, async (context) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-framework-test-"));
  context.after(() => fs.rm(root, { recursive: true, force: true }));
  const source = path.join(root, "source", "Fixture.framework");
  const version = path.join(source, "Versions", "A");
  await fs.mkdir(path.join(version, "Resources"), { recursive: true });
  const code = path.join(root, "fixture.c");
  await fs.writeFile(code, "int fixture(void) { return 42; }\n");
  await run("clang", ["-dynamiclib", code, "-o", path.join(version, "Fixture")]);
  await run("codesign", ["--force", "--sign", "-", "--timestamp=none", path.join(version, "Fixture")]);
  await fs.writeFile(path.join(version, "Resources", "Info.plist"), `<?xml version="1.0"?>
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>Fixture</string>
<key>CFBundleIdentifier</key><string>org.example.fixture</string>
<key>CFBundlePackageType</key><string>FMWK</string>
<key>CFBundleVersion</key><string>1</string>
</dict></plist>`);
  await fs.symlink("A", path.join(source, "Versions", "Current"));
  await fs.symlink("Versions/Current/Fixture", path.join(source, "Fixture"));
  await fs.symlink("Versions/Current/Resources", path.join(source, "Resources"));
  const output = path.join(root, "payload", "Fixture.framework");
  await copyTreePreservingExecutables(source, output);
  const original = path.join(output, "Fixture");
  assert.equal((await fs.lstat(original)).isSymbolicLink(), false);
  await assert.rejects(run("codesign", ["--verify", "--strict", original]), /code has no resources|bundle format is ambiguous/);

  const signedBytes = await fs.readFile(original);
  await prepareMacosRuntime(output);
  assert.deepEqual(await fs.readFile(original), signedBytes, "preserve the valid upstream signature");

  // Removing a signature outside the bundle reproduces a genuinely unsigned
  // runtime, separately from codesign's path-dependent bundle diagnosis.
  const standalone = path.join(root, "standalone");
  await fs.copyFile(original, standalone);
  await run("codesign", ["--remove-signature", standalone]);
  await fs.copyFile(standalone, original);
  await fs.chmod(original, 0o755);
  await assert.rejects(run("codesign", ["--force", "--sign", "-", "--timestamp=none", original]), /bundle format is ambiguous/);
  await prepareMacosRuntime(output);
  await fs.copyFile(original, standalone);
  await run("codesign", ["--verify", "--strict", standalone]);
  assert.equal((await fs.stat(original)).mode & 0o777, 0o755);
  const repairedBytes = await fs.readFile(original);
  await prepareMacosRuntime(output);
  assert.deepEqual(await fs.readFile(original), repairedBytes, "normalization must be repeatable");
});

test("macOS runtime normalization still rejects symbolic links", macosOnly, async (context) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-macos-link-test-"));
  context.after(() => fs.rm(root, { recursive: true, force: true }));
  await fs.writeFile(path.join(root, "target"), "ordinary resource");
  await fs.symlink("target", path.join(root, "link"));
  await assert.rejects(prepareMacosRuntime(root), /must be staged without symlinks/);
});
