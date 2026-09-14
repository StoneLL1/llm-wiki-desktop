import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { capabilityReleaseLocation, publishDesktopRelease, releaseUploads, uploadedAssetErrors } from "./publish-desktop-release.mjs";

const mutations = (calls) => calls.filter((args) => ["create", "upload", "edit", "delete"].includes(args[1]));
const latestQueries = (calls) => calls.filter((args) => args[1] === "view" && args[2] === "--repo");

async function fixture(context) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "release-publish-"));
  context.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.writeFileSync(path.join(root, "release-notes.md"), "Release 0.2.1");
  fs.writeFileSync(path.join(root, "app.exe"), "signed artifact fixture");
  fs.writeFileSync(path.join(root, "CHECKSUMS.sha256"), "checksum fixture");
  const uploads = await releaseUploads(root);
  const state = { exists: true, draft: true, assets: [], latest: "app-v0.2.0", pageSize: 100, nextId: 100, omitDigests: false, interruptUpload: null, latestError: null, downloadError: null };
  const calls = [];
  const remote = (upload, overrides = {}) => ({ id: state.nextId++, ...upload, state: "uploaded", ...overrides });
  const gh = (args) => {
    calls.push(args);
    if (args[0] === "api" && args[1].includes("/assets")) {
      const pages = [];
      for (let i = 0; i < state.assets.length; i += state.pageSize) pages.push(state.assets.slice(i, i + state.pageSize));
      return JSON.stringify(pages);
    }
    if (args[1] === "view") {
      if (args[2] === "--repo") {
        if (state.latestError) throw state.latestError;
        if (state.latest == null) throw new Error("release not found");
        return JSON.stringify({ tagName: state.latest });
      }
      if (!state.exists) throw new Error("release not found");
      return JSON.stringify({ databaseId: 7, isDraft: state.draft });
    }
    if (args[1] === "create") { state.exists = true; state.draft = true; return ""; }
    if (args[1] === "upload") {
      const upload = uploads.find((asset) => asset.file === args[3]);
      if (state.interruptUpload === upload.name) {
        state.interruptUpload = null;
        throw new Error("connection reset during upload");
      }
      state.assets = [...state.assets.filter((asset) => asset.name !== upload.name), remote(upload, state.omitDigests ? { digest: null } : {})];
      return "";
    }
    if (args[1] === "download") {
      if (state.downloadError) throw state.downloadError;
      const name = args[args.indexOf("--pattern") + 1];
      const asset = state.assets.find((candidate) => candidate.name === name);
      fs.writeFileSync(args[args.indexOf("--output") + 1], asset.bytes ?? fs.readFileSync(uploads.find((upload) => upload.name === name).file));
      return "";
    }
    if (args[1] === "edit") { state.draft = false; return ""; }
    throw new Error(`unexpected gh command: ${args.join(" ")}`);
  };
  return { root, uploads, calls, gh, state, remote };
}

const publish = (f, options = {}) => publishDesktopRelease({ root: f.root, tag: "app-v0.2.1", gh: f.gh, ...options });

test("new stable release uploads and verifies every asset before promoting latest", async (context) => {
  const f = await fixture(context);
  f.state.exists = false;
  await publish(f);
  assert.equal(f.calls.filter((args) => args[1] === "upload").length, f.uploads.length);
  const create = f.calls.find((args) => args[1] === "create");
  assert.ok(create.includes("--verify-tag") && create.includes("--draft") && create.includes("--latest=false"));
  assert.ok(!create.includes("--target"));
  assert.equal(f.calls.at(-1)[1], "edit");
  assert.ok(f.calls.at(-1).includes("--latest"));
  assert.ok(f.calls.at(-1).includes("--prerelease=false"));
  assert.equal(latestQueries(f.calls).length, 1);
  assert.ok(f.calls.findIndex((args) => args[1] === "upload") < f.calls.indexOf(latestQueries(f.calls)[0]));
});

test("an interrupted RC upload resumes its draft without repeating completed files or touching latest", async (context) => {
  const f = await fixture(context);
  f.state.exists = false;
  f.state.interruptUpload = f.uploads[1].name;
  await assert.rejects(publish(f, { tag: "app-v0.2.1-rc.2" }), /connection reset/);
  assert.equal(f.state.draft, true);
  assert.equal(f.state.assets.length, 1);
  await publish(f, { tag: "app-v0.2.1-rc.2" });
  assert.equal(f.calls.filter((args) => args[1] === "create").length, 1);
  assert.equal(f.calls.filter((args) => args[1] === "upload" && args[3] === f.uploads[0].file).length, 1);
  assert.ok(f.calls.find((args) => args[1] === "create").includes("--prerelease"));
  assert.ok(f.calls.at(-1).includes("--latest=false"));
  assert.ok(f.calls.at(-1).includes("--prerelease=true"));
  assert.equal(latestQueries(f.calls).length, 0);
});

test("retry replaces only incomplete or changed draft assets and keeps unknown attachments", async (context) => {
  const f = await fixture(context);
  f.state.assets = f.uploads.map((asset, index) => f.remote(asset, index === 1 ? { state: "starter" } : {}));
  f.state.assets.push({ name: "maintainer-notes.txt", state: "uploaded", size: 10 });
  await publish(f);
  assert.equal(f.calls.filter((args) => args[1] === "upload").length, 1);
  assert.ok(f.state.assets.some((asset) => asset.name === "maintainer-notes.txt"));
  assert.ok(f.calls.every((args) => args[1] !== "delete"));
});

test("public same-byte stable and RC releases succeed without writes or latest queries", async (context) => {
  for (const tag of ["app-v0.2.1", "app-v0.2.1-rc.2"]) {
    const f = await fixture(context);
    f.state.draft = false;
    f.state.pageSize = 1;
    f.state.assets = f.uploads.map((asset) => f.remote(asset));
    f.state.assets.push({ name: "unrelated.zip" });
    await publish(f, { tag });
    assert.deepEqual(mutations(f.calls), []);
    assert.deepEqual(latestQueries(f.calls), []);
    assert.ok(f.calls.some((args) => args.includes("--paginate")));
  }
});

test("public changed or missing assets are never overwritten", async (context) => {
  for (const assets of ["missing", "different-digest"]) {
    const f = await fixture(context);
    f.state.draft = false;
    f.state.assets = assets === "missing" ? [] : f.uploads.map((asset) => f.remote(asset, { digest: "sha256:wrong" }));
    await assert.rejects(publish(f), /already public/);
    assert.deepEqual(mutations(f.calls), []);
    assert.equal(f.calls.some((args) => args[1] === "download"), false, "present digest mismatch does not use fallback");
  }
});

test("missing GitHub digests use single-file download verification and clean temporary files", async (context) => {
  const f = await fixture(context);
  f.state.assets = f.uploads.map((asset) => f.remote(asset, { digest: null }));
  await publish(f);
  const downloads = f.calls.filter((args) => args[1] === "download");
  assert.equal(downloads.length, f.uploads.length, "unchanged assets are not downloaded a second time during final verification");
  assert.equal(f.calls.filter((args) => args[1] === "upload").length, 0);
  for (const args of downloads) {
    assert.ok(f.uploads.some((upload) => upload.name === args[args.indexOf("--pattern") + 1]));
    assert.equal(fs.existsSync(path.dirname(args[args.indexOf("--output") + 1])), false);
  }
});

test("a public same-byte release with missing digests is downloaded and accepted without writes", async (context) => {
  const f = await fixture(context);
  f.state.draft = false;
  f.state.assets = f.uploads.map((asset) => f.remote(asset, { digest: null }));
  await publish(f);
  assert.equal(f.calls.filter((args) => args[1] === "download").length, f.uploads.length);
  assert.deepEqual(mutations(f.calls), []);
  assert.deepEqual(latestQueries(f.calls), []);
});

test("new uploads can publish when the server still omits their digests", async (context) => {
  const f = await fixture(context);
  f.state.omitDigests = true;
  await publish(f);
  assert.equal(f.calls.filter((args) => args[1] === "download").length, f.uploads.length);
  assert.equal(f.state.draft, false);
});

test("fallback detects same-size different bytes and only replaces draft assets", async (context) => {
  for (const draft of [true, false]) {
    const f = await fixture(context);
    f.state.draft = draft;
    f.state.assets = f.uploads.map((asset) => f.remote(asset));
    f.state.assets[0] = f.remote(f.uploads[0], { digest: null, bytes: Buffer.alloc(f.uploads[0].size, 1) });
    if (draft) {
      await publish(f);
      assert.equal(f.calls.filter((args) => args[1] === "upload").length, 1);
    } else {
      await assert.rejects(publish(f), /already public/);
      assert.deepEqual(mutations(f.calls), []);
    }
  }
});

test("present mismatching digest replaces a draft without falling back to downloads", async (context) => {
  const f = await fixture(context);
  f.state.assets = f.uploads.map((asset) => f.remote(asset));
  f.state.assets[0].digest = "sha256:wrong";
  await publish(f);
  assert.equal(f.calls.filter((args) => args[1] === "upload").length, 1);
  assert.equal(f.calls.filter((args) => args[1] === "download").length, 0);
});

test("fallback download errors leave the draft intact and remove the temporary directory", async (context) => {
  const f = await fixture(context);
  f.state.assets = f.uploads.map((asset) => f.remote(asset, { digest: null }));
  f.state.downloadError = Object.assign(new Error("download forbidden"), { stderr: "HTTP 403" });
  await assert.rejects(publish(f), /download forbidden/);
  assert.deepEqual(mutations(f.calls), []);
  const download = f.calls.find((args) => args[1] === "download");
  assert.equal(fs.existsSync(path.dirname(download[download.indexOf("--output") + 1])), false);
});

test("older stable releases publish without rolling latest backward, with numeric version ordering", async (context) => {
  for (const [tag, current, expected] of [["app-v0.2.1", "app-v0.3.0", false], ["app-v0.2.10", "app-v0.2.9", true], ["app-v1.0.0", null, true], ["app-v1.0.0", "custom-tag", false]]) {
    const f = await fixture(context);
    f.state.latest = current;
    await publish(f, { tag });
    assert.ok(f.calls.at(-1).includes(expected ? "--latest" : "--latest=false"));
  }
});

test("permission and transport failures are never interpreted as absent releases", async (context) => {
  for (const error of [Object.assign(new Error("release not found"), { stderr: "HTTP 403" }), new Error("network connection reset"), Object.assign(new Error("forbidden"), { stderr: "HTTP 500" })]) {
    const f = await fixture(context);
    await assert.rejects(publish(f, { gh: (args) => { f.calls.push(args); throw error; } }), (actual) => actual === error);
    assert.deepEqual(mutations(f.calls), []);
  }
});

test("a failed final latest query preserves verified uploads for retry", async (context) => {
  const f = await fixture(context);
  f.state.latestError = Object.assign(new Error("latest forbidden"), { stderr: "HTTP 403" });
  await assert.rejects(publish(f), /latest forbidden/);
  assert.equal(f.state.assets.length, f.uploads.length);
  assert.equal(f.state.draft, true);
  f.state.latestError = null;
  await publish(f);
  assert.equal(f.calls.filter((args) => args[1] === "upload").length, f.uploads.length);
});

test("upload validation checks only required files and rejects duplicates, wrong sizes and missing digests", () => {
  const upload = { name: "app.exe", size: 20, digest: "sha256:abc" };
  const asset = { ...upload, state: "uploaded" };
  assert.deepEqual(uploadedAssetErrors([upload], [asset, { name: "notes.txt" }]), []);
  for (const delta of [{ name: "renamed.exe" }, { size: 19 }, { digest: "sha256:tampered" }, { digest: null }]) {
    assert.ok(uploadedAssetErrors([upload], [{ ...asset, ...delta }]).length > 0);
  }
  assert.ok(uploadedAssetErrors([upload], []).length > 0);
  assert.ok(uploadedAssetErrors([upload], [asset, asset]).length > 0);
});

test("legacy capability channel remains a prerelease without taking latest", async (context) => {
  const f = await fixture(context);
  f.state.exists = false;
  await publish(f, { tag: "capabilities-v0.2.1", channel: "capabilities" });
  assert.ok(f.calls.find((args) => args[1] === "create").includes("--prerelease"));
  assert.ok(f.calls.at(-1).includes("--latest=false"));
  assert.equal(latestQueries(f.calls).length, 0);
  await assert.rejects(publish(f, { channel: "capabilities" }), /selected channel/);
});

test("independent dated resource releases create their own tag and resume uploads without taking latest", async (context) => {
  const f = await fixture(context);
  f.state.exists = false;
  f.state.interruptUpload = f.uploads[1].name;
  const options = { tag: "capabilities-2026-09-14", channel: "capabilities", repository: "owner/resources", target: "a".repeat(40) };
  await assert.rejects(publish(f, options), /connection reset/u);
  await publish(f, options);
  assert.equal(f.calls.filter((args) => args[1] === "create").length, 1);
  assert.equal(f.calls.filter((args) => args[1] === "upload" && args[3] === f.uploads[0].file).length, 1);
  const create = f.calls.find((args) => args[1] === "create");
  assert.equal(create[create.indexOf("--target") + 1], options.target);
  assert.ok(!create.includes("--verify-tag"));
  assert.ok(create.includes("--draft") && create.includes("--prerelease"));
  for (const args of f.calls) {
    if (args[0] === "api") assert.match(args[1], /^repos\/owner\/resources\/releases\//u);
    else assert.equal(args[args.indexOf("--repo") + 1], options.repository);
  }
  assert.ok(f.calls.at(-1).includes("--prerelease=true") && f.calls.at(-1).includes("--latest=false"));
  assert.deepEqual(latestQueries(f.calls), []);
  f.calls.length = 0;
  await publish(f, options);
  assert.deepEqual(mutations(f.calls), [], "public matching resources are a read-only retry");
  f.state.assets[0].digest = "sha256:changed";
  f.calls.length = 0;
  await assert.rejects(publish(f, options), /already public/u);
  assert.deepEqual(mutations(f.calls), [], "public resources are never overwritten");
});

test("resource hosting coordinates are derived from the exact repository download URL", () => {
  assert.deepEqual(capabilityReleaseLocation("https://github.com/owner/resources/releases/download/capabilities-2026-09-14/", "owner/resources"), {
    repository: "owner/resources", tag: "capabilities-2026-09-14",
  });
  for (const value of [
    "https://github.com/elsewhere/resources/releases/download/capabilities-2026-09-14/",
    "https://github.com/owner/resources/releases/download/app-v0.2.2/",
    "https://github.com/owner/resources/releases/download/capabilities-2026-09-14/?token=secret",
    "https://github.com/owner/resources/releases/download/capabilities-2026-09-14/models/",
    "https://cdn.llmwiki.cn/capabilities-2026-09-14/",
  ]) assert.throws(() => capabilityReleaseLocation(value, "owner/resources"), /GitHub resource publishing/u);
});

test("resource target creation does not weaken the desktop tag contract", async (context) => {
  const f = await fixture(context);
  await assert.rejects(publish(f, { target: "a".repeat(40) }), /only supported for resource releases/u);
  await assert.rejects(publish(f, { tag: "capabilities-2026-09-14", channel: "capabilities", target: "main" }), /full commit SHA/u);
  await assert.rejects(publish(f, { tag: "capabilities-2026-09-14" }), /selected channel/u);
  assert.deepEqual(f.calls, []);
});
