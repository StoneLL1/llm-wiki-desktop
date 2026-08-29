import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";

import {
  expectedReleaseMatrix,
  loadFormatPipelineFixtures,
  loadProductCapabilityManifest,
  repositoryRoot,
  verifyProductCapabilities,
} from "./verify-product-capabilities.mjs";

const manifestPath = path.join(repositoryRoot, "capabilities", "product-manifest.json");

test("the repository product capability manifest is the valid release source of truth", async () => {
  const manifest = await loadProductCapabilityManifest(manifestPath);
  const { errors } = await verifyProductCapabilities({ manifest, repositoryRoot });
  assert.deepEqual(errors, []);

  const published = manifest.definitions.filter((definition) => definition.distributionTier === "published");
  const matrix = expectedReleaseMatrix(manifest);
  assert.equal(matrix.length, published.length * manifest.supportedTargets.length);
  assert.equal(new Set(matrix.map(({ capabilityId, targetTriple }) => `${capabilityId}\0${targetTriple}`)).size, matrix.length);
});

test("the product manifest closes the release-blocking routes, formats, and profiles", async () => {
  const manifest = await loadProductCapabilityManifest(manifestPath);
  const published = manifest.definitions.filter((definition) => definition.distributionTier === "published");
  const routes = new Set(published.flatMap((definition) => definition.routes));
  const formats = new Set(published.flatMap((definition) => definition.formats.extensions));
  const asrProfiles = new Set(published.flatMap((definition) => definition.profiles.asr));

  for (const route of ["pack.office-legacy", "pack.markitdown", "media.asr", "web.x.post"]) {
    assert.equal(routes.has(route), true, `missing published route ${route}`);
  }
  for (const extension of ["heic", "heif", "wma", "wmv", "gif"]) {
    assert.equal(formats.has(extension), true, `missing published format ${extension}`);
  }
  assert.equal(asrProfiles.has("accurate"), true);
  assert.equal(published.find(({ capabilityId }) => capabilityId === "browser-runtime")?.runtime.network, true);
  assert.equal(published.find(({ capabilityId }) => capabilityId === "browser-runtime-lite")?.runtime.network, false);
});

test("release expectations are manifest-derived rather than fixed five-pack counts", async () => {
  const catalogVerifier = await fs.readFile(path.join(repositoryRoot, "scripts", "verify-capability-catalog.mjs"), "utf8");
  const embeddedVerifier = await fs.readFile(path.join(repositoryRoot, "scripts", "verify-embedded-capability-catalog.mjs"), "utf8");
  const releaseVerifier = await fs.readFile(path.join(repositoryRoot, "scripts", "verify-release-assets.mjs"), "utf8");
  const capabilityWorkflow = await fs.readFile(path.join(repositoryRoot, ".github", "workflows", "capability-release.yml"), "utf8");

  for (const source of [catalogVerifier, embeddedVerifier, releaseVerifier]) {
    assert.equal(source.includes("entries.length !== 20"), false);
    assert.equal(source.includes("archives.length !== 20"), false);
  }
  assert.equal(capabilityWorkflow.includes("verify-product-capabilities.mjs --print-matrix"), true);
  assert.equal(capabilityWorkflow.includes("verify-product-capabilities.mjs --require-release-ready"), true);
  assert.equal(capabilityWorkflow.includes("matrix.target"), false);
  assert.equal(capabilityWorkflow.includes("merge-catalog"), false);
  assert.match(capabilityWorkflow, /Capability publication remains quarantined[\s\S]*exit 1/);
});

test("an undeclared fixture extension cannot widen release evidence", async () => {
  const manifest = await loadProductCapabilityManifest(manifestPath);
  const fixture = {
    capabilityId: "batch9-runtime-fixture",
    routes: [{ route: "media.asr", extensions: ["mp3", "made-up-container"] }],
  };
  const { errors } = await verifyProductCapabilities({ manifest, repositoryRoot, fixtures: [fixture] });
  assert.equal(errors.some((error) => error.includes("made-up-container")), true);
});

test("schema-only drift is rejected by the executable verifier", async () => {
  const manifest = await loadProductCapabilityManifest(manifestPath);
  const invalid = structuredClone(manifest);
  invalid.definitions[0].category = "typo";
  invalid.definitions[0].unexpectedReleaseSwitch = true;
  invalid.definitions[0].supportedTargets.push(invalid.definitions[0].supportedTargets[0]);
  const { errors } = await verifyProductCapabilities({ manifest: invalid, repositoryRoot, fixtures: [] });
  assert.equal(errors.some((error) => error.includes("category is outside the schema enum")), true);
  assert.equal(errors.some((error) => error.includes("unexpectedReleaseSwitch is not allowed")), true);
  assert.equal(errors.some((error) => error.includes("supportedTargets must contain unique items")), true);
});

test("the checked-in all-format fixture remains inside the product format surface", async () => {
  const manifest = await loadProductCapabilityManifest(manifestPath);
  const fixtures = loadFormatPipelineFixtures(repositoryRoot);
  const extensions = new Set(fixtures.flatMap((fixture) => fixture.routes.flatMap((route) => route.extensions)));
  for (const extension of ["heic", "heif", "gif", "wma", "wmv"]) assert.equal(extensions.has(extension), true);
  const { errors } = await verifyProductCapabilities({ manifest, repositoryRoot });
  assert.deepEqual(errors, []);
});

test("distributable identity is bound to exact workflow provenance", async () => {
  const buildScript = await fs.readFile(path.join(repositoryRoot, "src-tauri", "build.rs"), "utf8");
  const desktopWorkflow = await fs.readFile(path.join(repositoryRoot, ".github", "workflows", "desktop-release.yml"), "utf8");
  for (const name of [
    "LLM_WIKI_DISTRIBUTION_TAG",
    "LLM_WIKI_DISTRIBUTION_COMMIT",
    "LLM_WIKI_DISTRIBUTION_RUN_ID",
  ]) {
    assert.equal(buildScript.includes(name), true, `${name} is not checked by build.rs`);
    assert.equal(desktopWorkflow.includes(name), true, `${name} is not injected by desktop-release.yml`);
  }
  assert.equal(desktopWorkflow.includes("LLM_WIKI_CAPABILITY_CATALOG_MODE=distributable"), true);
  assert.equal(buildScript.includes("catalog-provenance.json"), true);
});
