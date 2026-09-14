import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import test from "node:test";
import yaml from "js-yaml";
import { buildCapabilityReleasePlan } from "./capability-release-plan.mjs";

const workflow = yaml.load(fs.readFileSync(new URL("../.github/workflows/capability-release.yml", import.meta.url), "utf8"));
const runText = (job) => workflow.jobs[job].steps.map((step) => step.run ?? "").join("\n");
const python = process.platform === "win32" ? "python" : "python3";

test("resource jobs use the complete native platform plan without desktop tags or signing credentials", async () => {
  const plan = await buildCapabilityReleasePlan();
  assert.deepEqual(plan.errors, []);
  const native = { "x86_64-pc-windows-msvc": "windows-2025", "aarch64-apple-darwin": "macos-15", "x86_64-apple-darwin": "macos-15-intel", "x86_64-unknown-linux-gnu": "ubuntu-24.04" };
  for (const entry of plan.entries) assert.equal(entry.os, native[entry.targetTriple]);
  assert.equal(workflow.jobs["build-capability"]["runs-on"], "${{ matrix.os }}");
  assert.equal(workflow.jobs["build-capability"].strategy.matrix, "${{ fromJSON(needs.release-preflight.outputs.matrix) }}");
  assert.equal(workflow.on.workflow_dispatch.inputs.base_url.required, true);
  const preflight = runText("release-preflight");
  assert.match(preflight, /catalogUrlErrors/u);
  const build = runText("build-capability");
  const stages = ["prepare-release-capability.mjs", "& $tool @arguments", "split-capability-models.py", "verify-install --catalog", "qualify-staged-capability.mjs", "qualify-release-corpus.mjs"];
  for (let i = 1; i < stages.length; i++) assert.ok(build.indexOf(stages[i - 1]) < build.indexOf(stages[i]));
  assert.doesNotMatch(build, /--key-id|LLM_WIKI_CAPABILITY_SIGNING_KEY|--expected-tag/u);
  assert.match(build, /verify-install --catalog \$fragment --archives resource-dist --output \$installRoot/u);
  assert.match(build, /\$qualified = \$verified\.installed\[0\]\.payload/u);
  assert.match(build, /\$verified\.restarted/u);
  assert.doesNotMatch(build, /unzip -q \$archive|tar -xf \$archive/u);
  assert.match(build, /PLAYWRIGHT_BROWSERS_PATH/u);
  assert.match(build, /runner\/browser\.smoke\.mjs/u);
  assert.match(build, /if \(\$env:LLM_WIKI_X_PRODUCTION_SAMPLE_URL\)/u);
  assert.match(build, /if \(\$env:LLM_WIKI_WECHAT_PRODUCTION_SAMPLE_URL\)/u);
});

test("artifact merge retains the splitter directory and verifies the full product catalog", () => {
  const upload = workflow.jobs["build-capability"].steps.find((step) => step.uses?.startsWith("actions/upload-artifact@"));
  assert.equal(upload.with.name, "resource-${{ matrix.capabilityId }}-${{ matrix.targetTriple }}");
  assert.equal(upload.with.path, "resource-dist/");
  assert.match(runText("build-capability"), /Move-Item resource-dist\/install-catalog\.json 'resource-dist\/\$\{\{ matrix.capabilityId \}\}-\$\{\{ matrix.targetTriple \}\}\.catalog\.json'/u);
  const merge = workflow.jobs["merge-catalog"];
  assert.deepEqual(merge.needs, ["release-preflight", "build-capability"]);
  const download = merge.steps.find((step) => step.uses?.startsWith("actions/download-artifact@"));
  assert.deepEqual(download.with, { pattern: "resource-*", path: "capability-dist", "merge-multiple": true });
  const commands = runText("merge-catalog");
  assert.match(commands, /merge-catalog --input capability-dist --output capability-dist\/install-catalog\.json/u);
  assert.match(commands, /verify-capability-catalog\.mjs[^\n]+--mode release/u);
  const final = merge.steps.find((step) => step.uses?.startsWith("actions/upload-artifact@"));
  assert.equal(final.with.name, "capability-resources");
  assert.equal(final.with.path, "capability-dist/");
});

test("optional runner publication stages flat assets, resumes drafts and verifies anonymous full downloads", () => {
  const input = workflow.on.workflow_dispatch.inputs.publish_github;
  assert.equal(input.type, "boolean");
  assert.equal(input.default, false);
  assert.equal(workflow.permissions.contents, "read");
  const merge = workflow.jobs["merge-catalog"];
  assert.equal(merge.permissions.contents, "write");
  const steps = merge.steps;
  const stage = steps.findIndex((step) => step.run?.includes("stage-capability-downloads.mjs"));
  const publish = steps.findIndex((step) => step.run?.includes("publish-desktop-release.mjs"));
  const verify = steps.findIndex((step) => step.run?.includes("verify-published-capability-assets.mjs"));
  assert.ok(stage >= 0 && stage < publish && publish < verify);
  for (const index of [stage, publish, verify]) assert.equal(steps[index].if, "inputs.publish_github");
  assert.match(steps[stage].run, /--input capability-dist --output github-capability-dist/u);
  assert.match(steps[publish].run, /--channel capabilities/u);
  assert.match(steps[publish].run, /--tag "\$RESOURCE_TAG" --repository "\$RESOURCE_REPOSITORY" --target "\$RESOURCE_COMMIT"/u);
  assert.match(steps[verify].run, /--catalog github-capability-dist\/install-catalog.json/u);
  assert.doesNotMatch(steps[verify].run, /availability-only/u);
  assert.equal(steps[verify].env?.GH_TOKEN, undefined);
  const catalog = steps.find((step) => step.with?.name === "public-capability-catalog");
  assert.ok(steps.indexOf(catalog) > verify);
  assert.equal(catalog.if, "inputs.publish_github");
  assert.match(catalog.with.path, /github-capability-dist\/install-catalog.json/u);
  const preflight = runText("release-preflight");
  assert.match(preflight, /capabilityReleaseLocation\(process.env.BASE_URL, process.env.GITHUB_REPOSITORY\)/u);
  assert.match(preflight, /process.env.PUBLISH_GITHUB === "true"/u);
});

test("two native artifacts merge distinct fragments and one shared offline model object", (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "wiki-resource-workflow-中文-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const targets = ["aarch64-apple-darwin", "x86_64-pc-windows-msvc"];
  const merged = path.join(root, "merged");
  for (const target of targets) {
    const source = path.join(root, target);
    fs.mkdirSync(source);
    execFileSync(python, ["-c", `
import hashlib,json,pathlib,sys,zipfile
root=pathlib.Path(sys.argv[1]); target=sys.argv[2]
name='asr-sensevoice-small-1.2.3-'+target+'.zip'
files={'runner/index.mjs':target.encode(),'models/model.int8.onnx':b'shared model'}
manifest=dict(schemaVersion=3,packId='asr-sensevoice-small',version='1.2.3',files=[dict(path=p,bytes=len(b),sha256=hashlib.sha256(b).hexdigest()) for p,b in files.items()])
with zipfile.ZipFile(root/name,'w') as z:
 z.writestr('manifest.json',json.dumps(manifest))
 for p,b in files.items():z.writestr(p,b)
entry=dict(capabilityId='asr-sensevoice-small',version='1.2.3',targetTriple=target,url='https://downloads.llmwiki.cn/resources/'+name,archiveSha256=hashlib.sha256((root/name).read_bytes()).hexdigest())
(root/'input.catalog.json').write_text(json.dumps(dict(schemaVersion=1,entries=[entry])))
`, source, target]);
    const output = path.join(source, "resource-dist");
    execFileSync(python, [path.join(import.meta.dirname, "split-capability-models.py"), "--catalog", path.join(source, "input.catalog.json"), "--archives", source, "--output", output, "--base-url", "https://downloads.llmwiki.cn/resources/", "--model-base-url", "https://models.llmwiki.cn/"]);
    fs.renameSync(path.join(output, "install-catalog.json"), path.join(output, `asr-sensevoice-small-${target}.catalog.json`));
    fs.cpSync(output, merged, { recursive: true });
  }
  const fragments = fs.readdirSync(merged).filter((name) => name.endsWith(".catalog.json"));
  assert.equal(fragments.length, 2);
  const entries = fragments.flatMap((name) => JSON.parse(fs.readFileSync(path.join(merged, name), "utf8")).entries);
  assert.deepEqual(new Set(entries.map((entry) => entry.targetTriple)), new Set(targets));
  assert.equal(fs.readdirSync(path.join(merged, "models")).length, 1);
  for (const entry of entries) {
    const archive = path.join(merged, path.basename(new URL(entry.url).pathname));
    assert.equal(createHash("sha256").update(fs.readFileSync(archive)).digest("hex"), entry.archiveSha256);
    const model = entry.modelFiles[0];
    const localModel = path.join(merged, "models", model.sha256, path.basename(model.path));
    assert.equal(createHash("sha256").update(fs.readFileSync(localModel)).digest("hex"), model.sha256);
    assert.equal(new URL(model.urls[0]).pathname, `/models/${model.sha256}/model.int8.onnx`);
  }
});
