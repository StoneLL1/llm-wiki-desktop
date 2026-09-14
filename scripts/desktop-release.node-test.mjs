import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import yaml from "js-yaml";
import { stageDesktopRelease, assembleDesktopDownloads } from "./stage-desktop-release.mjs";
import { RELEASE_PLATFORMS } from "./release-assets-contract.mjs";
import { releaseCoordinate, repositoryRoot } from "./check-release-version.mjs";

const workflow = yaml.load(fs.readFileSync(new URL("../.github/workflows/desktop-release.yml", import.meta.url), "utf8"));
const commands = (job) => job.steps.map((step) => step.run ?? "").join("\n");

test("stable and RC use one native pipeline with early resource availability and one publisher", () => {
  assert.ok(workflow.on.push.tags.includes("app-v*.*.*"));
  assert.equal(workflow.on.push.tags.some((tag) => tag.startsWith("!")), false);
  assert.equal(fs.existsSync(new URL("../.github/workflows/desktop-prerelease.yml", import.meta.url)), false);
  assert.deepEqual(Object.keys(workflow.jobs), ["preflight", "desktop-build", "publish"]);
  assert.match(commands(workflow.jobs.preflight), /verify-published-capability-assets\.mjs[^\n]+--availability-only/u);
  const build = workflow.jobs["desktop-build"];
  assert.equal(build.needs, "preflight");
  assert.deepEqual(new Set(build.strategy.matrix.include.map((row) => row.platform)), new Set(Object.keys(RELEASE_PLATFORMS)));
  assert.match(commands(build), /LLM_WIKI_CAPABILITY_CATALOG_MODE=distributable/u);
  assert.match(commands(build), /Resolve-Path capabilities/u);
  assert.match(commands(build), /verify-updater-signatures\.mjs/u);
  assert.match(commands(build), /verify-embedded-capability-catalog\.mjs/u);
  assert.deepEqual(workflow.jobs.publish.needs, ["preflight", "desktop-build"]);
  assert.match(commands(workflow.jobs.publish), /stage-desktop-release\.mjs --candidate/u);
  assert.match(commands(workflow.jobs.publish), /publish-desktop-release\.mjs/u);
  assert.doesNotMatch(commands(workflow.jobs.publish), /verify-published-capability-assets|gh release create/u);
});

test("release coordinates bind the tag to checkout and use reproducible commit time", () => {
  const git = (_root, args) => args[0] === "show" ? "2026-09-14T01:00:00+00:00" : "a".repeat(40);
  const coordinate = releaseCoordinate(repositoryRoot, "app-v0.2.1-rc.3", git);
  assert.equal(coordinate.channel, "prerelease");
  assert.equal(coordinate.version, "0.2.1-rc.3");
  assert.deepEqual(releaseCoordinate(repositoryRoot, "app-v0.2.1-rc.3", git), coordinate);
  assert.equal(coordinate.published_at, "2026-09-14T01:00:00+00:00");
  assert.throws(() => releaseCoordinate(repositoryRoot, "app-v0.2.1", (_root, args) => args[1] === "HEAD" ? "a".repeat(40) : "b".repeat(40)), /checkout does not match/u);
});

function candidateFixture(t, version) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "wiki-release-中文-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const candidate = path.join(root, "candidate");
  for (const platform of Object.keys(RELEASE_PLATFORMS)) {
    const source = path.join(root, "source", platform);
    fs.mkdirSync(source, { recursive: true });
    const updater = platform.startsWith("darwin") ? "LLM Wiki Desktop.app.tar.gz"
      : platform.startsWith("windows") ? "LLM Wiki Desktop-setup.exe" : "LLM Wiki Desktop.AppImage";
    fs.writeFileSync(path.join(source, updater), `binary bytes for ${platform}`);
    fs.writeFileSync(path.join(source, `${updater}.sig`), "bounded-signature-fixture".repeat(4));
    if (platform.startsWith("darwin")) fs.writeFileSync(path.join(source, "LLM Wiki Desktop.dmg"), "DMG bytes");
    stageDesktopRelease({ source, output: path.join(candidate, "desktop", platform), platform,
      releaseTag: `app-v${version}`, version, commitSha: "a".repeat(40) });
  }
  return { candidate, output: path.join(root, "public"), releaseTag: `app-v${version}`, version,
    notes: "Usable desktop release", pubDate: "2026-09-14T01:00:00Z" };
}

test("stable assembly selects real declared files and produces all four updater URLs", (t) => {
  const options = candidateFixture(t, "0.2.2");
  fs.writeFileSync(path.join(options.candidate, "desktop", "windows-x86_64", "build-log.txt"), "not a download");
  assert.deepEqual(assembleDesktopDownloads(options), { channel: "stable", platformCount: 4 });
  const files = fs.readdirSync(options.output);
  assert.equal(files.length, 8);
  assert.equal(files.some((file) => file.endsWith(".sig") || file === "build-log.txt"), false);
  const manifest = JSON.parse(fs.readFileSync(path.join(options.output, "latest.json"), "utf8"));
  assert.equal(Object.keys(manifest.platforms).length, 4);
  for (const entry of Object.values(manifest.platforms)) assert.ok(fs.existsSync(path.join(options.output, decodeURIComponent(new URL(entry.url).pathname.split("/").at(-1)))));
});

test("RC assembly includes signatures and never creates a stable update feed", (t) => {
  const options = candidateFixture(t, "0.2.2-rc.1");
  assert.deepEqual(assembleDesktopDownloads(options), { channel: "prerelease", platformCount: 4 });
  assert.equal(fs.existsSync(path.join(options.output, "latest.json")), false);
  assert.equal(fs.readdirSync(options.output).filter((file) => file.endsWith(".sig")).length, 4);
});

test("missing platforms, mixed commits and stale output cannot produce a public candidate", (t) => {
  const options = candidateFixture(t, "0.2.2");
  const descriptorPath = path.join(options.candidate, "desktop", "linux-x86_64", "release-entry.json");
  const descriptor = JSON.parse(fs.readFileSync(descriptorPath, "utf8"));
  fs.writeFileSync(descriptorPath, JSON.stringify({ ...descriptor, commitSha: "b".repeat(40) }));
  assert.throws(() => assembleDesktopDownloads(options), /does not match/u);
  assert.equal(fs.existsSync(options.output), false);
  fs.rmSync(descriptorPath);
  assert.throws(() => assembleDesktopDownloads(options), /ENOENT/u);
  fs.writeFileSync(descriptorPath, JSON.stringify(descriptor));
  fs.mkdirSync(options.output);
  fs.writeFileSync(path.join(options.output, "latest.json"), "previous version");
  assert.throws(() => assembleDesktopDownloads(options), /must be empty/u);
  assert.equal(fs.readFileSync(path.join(options.output, "latest.json"), "utf8"), "previous version");
});
