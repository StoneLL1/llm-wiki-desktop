import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { publishDesktopRelease, releaseUploads, uploadedAssetErrors } from "./publish-desktop-release.mjs";

async function fixture(context) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "release-publish-"));
  context.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.writeFileSync(path.join(root, "release-notes.md"), "Release 0.2.1");
  fs.writeFileSync(path.join(root, "app.exe"), "signed artifact fixture");
  fs.writeFileSync(path.join(root, "CHECKSUMS.sha256"), "checksum fixture");
  const uploads = await releaseUploads(root);
  let exists = true;
  let draft = true;
  let assets = [];
  let createdTag = null;
  let pageSize = 100;
  const calls = [];
  const gh = (args) => {
    calls.push(args);
    if (args[0] === "api" && args[1].includes("/assets")) {
      const pages = [];
      for (let index = 0; index < assets.length; index += pageSize) pages.push(assets.slice(index, index + pageSize));
      return JSON.stringify(pages);
    }
    if (args[0] === "release" && args[1] === "view") {
      if (!exists) throw new Error("release not found");
      return JSON.stringify({ databaseId: 7, isDraft: draft, tagName: createdTag ?? "app-v0.2.1" });
    }
    if (args[1] === "create") { exists = true; createdTag = args[2]; return ""; }
    if (args[1] === "upload") {
      const upload = uploads.find((asset) => asset.file === args[3]);
      assets = [...assets.filter((asset) => asset.name !== upload.name), { ...upload, state: "uploaded" }];
      return "";
    }
    if (args[1] === "edit") { draft = false; return ""; }
    throw new Error(`unexpected gh command: ${args.join(" ")}`);
  };
  return { root, uploads, calls, gh, setExists: (value) => { exists = value; }, setDraft: (value) => { draft = value; }, setAssets: (value) => { assets = value; }, setPageSize: (value) => { pageSize = value; } };
}

test("new release uploads all assets and verifies digests before publishing", async (context) => {
  const f = await fixture(context);
  f.setExists(false);
  await publishDesktopRelease({ root: f.root, tag: "app-v0.2.1", gh: f.gh });
  assert.equal(f.calls.filter((args) => args[1] === "upload").length, 3);
  const create = f.calls.find((args) => args[1] === "create");
  assert.ok(create.includes("--verify-tag") && create.includes("--draft"));
  assert.ok(!create.includes("--target"));
  assert.equal(f.calls.at(-1)[1], "edit");
  assert.ok(f.calls.some((args) => args.includes("--paginate")));
});

test("a missing draft is created and then resolved from the release list", async (context) => {
  const f = await fixture(context);
  f.setExists(false);
  await publishDesktopRelease({ root: f.root, tag: "app-v0.2.1", gh: f.gh });
  assert.ok(f.calls.some((args) => args[1] === "create"));
});

test("retry keeps matching draft uploads and replaces an incomplete upload", async (context) => {
  const f = await fixture(context);
  f.setAssets(f.uploads.map((asset, index) => ({ ...asset, state: index === 1 ? "starter" : "uploaded" })));
  await publishDesktopRelease({ root: f.root, tag: "app-v0.2.1", gh: f.gh });
  assert.equal(f.calls.filter((args) => args[1] === "upload").length, 1);
});

test("a public release cannot be overwritten or deleted", async (context) => {
  const f = await fixture(context);
  f.setDraft(false);
  await assert.rejects(publishDesktopRelease({ root: f.root, tag: "app-v0.2.1", gh: f.gh }), /already public/);
  assert.ok(f.calls.every((args) => !["create", "upload", "edit", "delete"].includes(args[1])));
});

test("all pages of an already matching public release succeed without writes", async (context) => {
  const f = await fixture(context);
  f.setDraft(false);
  f.setPageSize(1);
  f.setAssets(f.uploads.map((asset) => ({ ...asset, state: "uploaded" })));
  await publishDesktopRelease({ root: f.root, tag: "app-v0.2.1", gh: f.gh });
  assert.ok(f.calls.every((args) => !["create", "upload", "edit", "delete"].includes(args[1])));
});

test("unexpected assets leave the draft unpublished, without deleting anything", async (context) => {
  const f = await fixture(context);
  f.setAssets([{ name: "stale.zip", state: "uploaded" }]);
  await assert.rejects(publishDesktopRelease({ root: f.root, tag: "app-v0.2.1", gh: f.gh }), /unexpected draft asset: stale.zip/);
  assert.ok(f.calls.every((args) => !["edit", "delete"].includes(args[1])));
});

test("upload validation catches missing, renamed, truncated, and changed bytes", () => {
  const upload = { name: "app.exe", size: 20, digest: "sha256:abc" };
  const asset = { ...upload, state: "uploaded" };
  assert.deepEqual(uploadedAssetErrors([upload], [asset]), []);
  for (const delta of [{ name: "renamed.exe" }, { size: 19 }, { digest: "sha256:tampered" }, { digest: null }]) {
    assert.ok(uploadedAssetErrors([upload], [{ ...asset, ...delta }]).length > 0);
  }
  assert.ok(uploadedAssetErrors([upload], []).length > 0);
});

test("permission and network errors do not create a release", async (context) => {
  const f = await fixture(context);
  let calls = 0;
  await assert.rejects(publishDesktopRelease({ root: f.root, tag: "app-v0.2.1", gh: () => {
    calls++;
    throw Object.assign(new Error("forbidden"), { stderr: "HTTP 403" });
  } }), /forbidden/);
  assert.equal(calls, 1);
});

test("capability channel stays a prerelease and never takes over latest", async (context) => {
  const f = await fixture(context);
  f.setExists(false);
  await publishDesktopRelease({ root: f.root, tag: "capabilities-v0.2.1", channel: "capabilities", gh: f.gh });
  const create = f.calls.find((args) => args[1] === "create");
  assert.ok(create.includes("--prerelease"));
  assert.ok(f.calls.at(-1).includes("--latest=false"));
  assert.ok(!f.calls.at(-1).includes("--latest"));
  await assert.rejects(publishDesktopRelease({ root: f.root, tag: "app-v0.2.1", channel: "capabilities", gh: f.gh }), /selected channel/);
});
