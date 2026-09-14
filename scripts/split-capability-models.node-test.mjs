import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import test from "node:test";

const python = process.platform === "win32" ? "python" : "python3";
const splitter = path.join(import.meta.dirname, "split-capability-models.py");

function fixture(t, { target = "aarch64-apple-darwin", extraFiles = [] } = {}) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "capability-models-中文-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  execFileSync(python, ["-c", `
import pathlib, json, hashlib, zipfile, sys
root = pathlib.Path(sys.argv[1])
options = json.loads(sys.argv[2])
model = b'platform independent model data'
program = b'platform specific runtime'
files = [('runner/index.mjs',program), ('models/model.int8.onnx',model)] + [(name, data.encode()) for name,data in options['extraFiles']]
manifest = dict(schemaVersion=2, packId='asr-sensevoice-small', version='1.2.3', signature='old-signature', signingKeyId='old-key', files=[dict(path=name,sha256=hashlib.sha256(data).hexdigest(),bytes=len(data)) for name,data in files])
with zipfile.ZipFile(root/'pack.zip','w') as out:
    out.writestr('manifest.json',json.dumps(manifest))
    for name,data in files: out.writestr(name,data)
entry = dict(capabilityId='asr-sensevoice-small',version='1.2.3',targetTriple=options['target'],url='https://example.com/pack.zip',archiveSha256=hashlib.sha256((root/'pack.zip').read_bytes()).hexdigest(),archiveChunks=[{}],compressedBytes=(root/'pack.zip').stat().st_size,installedBytes=100,signingKeyId='old-key')
(root/'catalog.json').write_text(json.dumps(dict(schemaVersion=1,entries=[entry])))
`, root, JSON.stringify({ target, extraFiles })], { stdio: "pipe" });
  return root;
}

test("splits models into cross-platform objects and a complete offline directory", (t) => {
  const root = fixture(t);
  const output = path.join(root, "output");
  execFileSync(python, [splitter, "--catalog", path.join(root, "catalog.json"), "--archives", root, "--output", output, "--base-url", "https://cdn.example.com/capabilities/", "--model-base-url", "https://models.example.com/"]);
  const { entries: [entry] } = JSON.parse(fs.readFileSync(path.join(output, "install-catalog.json"), "utf8"));
  assert.equal(entry.signingKeyId, "");
  assert.equal(entry.archiveChunks, undefined);
  assert.equal(entry.url, "https://cdn.example.com/capabilities/pack.zip");
  const model = entry.modelFiles[0];
  assert.equal(model.path, "models/model.int8.onnx");
  assert.equal(model.urls[0], `https://models.example.com/models/${model.sha256}/model.int8.onnx`);
  assert.equal(fs.readFileSync(path.join(output, "models", model.sha256, "model.int8.onnx"), "utf8"), "platform independent model data");
  const archive = JSON.parse(execFileSync(python, ["-c", "import zipfile,json,sys; z=zipfile.ZipFile(sys.argv[1]); print(json.dumps(dict(installedBytes=sum(i.file_size for i in z.infolist() if not i.is_dir()),names=z.namelist(),manifest=json.loads(z.read('manifest.json')))))", path.join(output, "pack.zip")], { encoding: "utf8" }));
  assert.deepEqual(archive.names, ["manifest.json", "runner/index.mjs"]);
  assert.equal(entry.installedBytes, archive.installedBytes);
  assert.equal(archive.manifest.schemaVersion, 3);
  assert.equal(archive.manifest.signature, "");
  assert.equal(archive.manifest.files.length, 2, "model declarations remain available to existing runners");
});

function split(root) {
  execFileSync(python, [splitter, "--catalog", path.join(root, "catalog.json"), "--archives", root,
    "--output", path.join(root, "output"), "--base-url", "https://example.com/"], { stdio: "pipe" });
}

test("Linux program paths differing only by case survive splitting with distinct bytes", (t) => {
  // The locked Linux python-build-standalone archive contains these distinct
  // terminfo paths; both document-standard and OCR redistribute this runtime.
  const files = [["runtime/python/share/terminfo/2/2621A", "uppercase"],
    ["runtime/python/share/terminfo/2/2621a", "lowercase"],
    ["runtime/python/share/terminfo/E/Eterm", "Eterm"],
    ["runtime/python/share/terminfo/e/eterm", "eterm"],
    ["runner/中文/A.txt", "A"], ["runner/中文/a.txt", "a"]];
  const root = fixture(t, { target: "x86_64-unknown-linux-gnu", extraFiles: files });
  split(root);
  const retained = JSON.parse(execFileSync(python, ["-c", `
import json,sys,zipfile
with zipfile.ZipFile(sys.argv[1]) as archive:
    print(json.dumps({name: archive.read(name).decode() for name in json.loads(sys.argv[2])}))
`, path.join(root, "output", "pack.zip"), JSON.stringify(files.map(([name]) => name))], { encoding: "utf8" }));
  assert.deepEqual(retained, Object.fromEntries(files));
});

test("macOS and Windows still reject paths differing only by case", (t) => {
  for (const target of ["aarch64-apple-darwin", "x86_64-apple-darwin", "x86_64-pc-windows-msvc"]) {
    const root = fixture(t, { target, extraFiles: [["runner/INDEX.mjs", "collision"]] });
    assert.throws(() => split(root), /duplicate paths/u);
    assert.equal(fs.existsSync(path.join(root, "output", "install-catalog.json")), false);
  }
});

test("Linux still rejects exact duplicates, file-directory aliases, unsafe paths and model case collisions", (t) => {
  const cases = [
    { extraFiles: [["runner/index.mjs", "replacement"]], error: /duplicate paths/u },
    { extraFiles: [["runner/index.mjs/", ""]], error: /duplicate paths/u },
    { extraFiles: [["../escape", "escape"]], error: /unsafe paths/u },
    { extraFiles: [["/absolute", "escape"]], error: /unsafe paths/u },
    { extraFiles: [["C:\\escape", "escape"]], error: /unsafe paths/u },
    { extraFiles: [["models/MODEL.int8.onnx", "another model"]], error: /model files have case-insensitive duplicate paths/u },
  ];
  for (const { extraFiles, error } of cases) {
    const root = fixture(t, { target: "x86_64-unknown-linux-gnu", extraFiles });
    assert.throws(() => split(root), error);
    assert.equal(fs.existsSync(path.join(root, "output", "install-catalog.json")), false);
  }
});

test("rejects a source ZIP that does not match the existing catalog", (t) => {
  const root = fixture(t);
  fs.appendFileSync(path.join(root, "pack.zip"), "corruption");
  assert.throws(() => execFileSync(python, [splitter, "--catalog", path.join(root, "catalog.json"), "--archives", root, "--output", path.join(root, "output"), "--base-url", "https://example.com/"], { stdio: "pipe" }), /checksum does not match/u);
  assert.equal(fs.existsSync(path.join(root, "output", "install-catalog.json")), false);
});
