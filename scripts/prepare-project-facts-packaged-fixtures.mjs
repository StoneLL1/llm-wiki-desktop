import { createHash } from "node:crypto";
import { chmod, mkdir, stat, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";

const SUPPORT_FILE_COUNT = 240;
const NATIVE_PROJECT_ID = "00000000-0000-4000-8000-000000000001";
const NATIVE_DIRECTORIES = [
  "raw/sources/pdfs",
  "raw/sources/docs",
  "raw/sources/slides",
  "raw/sources/sheets",
  "raw/sources/markdown",
  "raw/sources/links",
  "raw/sources/other",
  "raw/extracted",
  "raw/assets",
  "wiki/entities",
  "wiki/concepts",
  "wiki/sources",
  "wiki/queries",
  "wiki/synthesis",
  "wiki/comparisons",
  "exports/html",
  "skills",
  ".app/chats",
  ".app/tasks",
];

function normalized(relativePath) {
  return relativePath.split(path.sep).join("/");
}

async function writeFixtureFile(root, relativePath, contents, files) {
  const target = path.join(root, relativePath);
  await mkdir(path.dirname(target), { recursive: true });
  await writeFile(target, contents, "utf8");
  files.push({ path: normalized(relativePath), contents });
}

function runGit(args, options = {}) {
  const result = spawnSync("git", args, {
    encoding: "utf8",
    windowsHide: true,
    ...options,
  });
  if (result.status !== 0) {
    const detail = result.error?.message
      ?? result.stderr?.trim()
      ?? result.stdout?.trim()
      ?? `exit status ${String(result.status)}`;
    throw new Error(`git ${args[0]} failed: ${detail}`);
  }
  return result;
}

function fixtureHash(files) {
  const hash = createHash("sha256");
  for (const file of [...files].sort((left, right) =>
    left.path < right.path ? -1 : left.path > right.path ? 1 : 0
  )) {
    hash.update(file.path);
    hash.update("\0");
    hash.update(file.contents);
    hash.update("\0");
  }
  return hash.digest("hex");
}

async function prepareNativeProject(root) {
  const files = [];
  for (const directory of NATIVE_DIRECTORIES) {
    await mkdir(path.join(root, directory), { recursive: true });
  }
  await writeFixtureFile(root, "purpose.md", "# Purpose\n\nProject Facts performance fixture.\n", files);
  await writeFixtureFile(root, "schema.md", "# Schema\n\n- Concept\n- Source\n", files);
  for (const [name, title] of [["index.md", "Index"], ["log.md", "Log"], ["overview.md", "Overview"]]) {
    await writeFixtureFile(root, path.join("wiki", name), `# ${title}\n\nDeterministic page fixture.\n`, files);
  }
  const appFiles = {
    ".app/project.json": `${JSON.stringify({ projectId: NATIVE_PROJECT_ID }, null, 2)}\n`,
    ".app/settings.json": `${JSON.stringify({ template: "general" }, null, 2)}\n`,
    ".app/agent-config.json": "{}\n",
    ".app/bookmarks.json": "[]\n",
    ".app/graph-cache.json": `${JSON.stringify({
      nodes: [],
      edges: [],
      contentHash: "",
      builtAt: "2026-08-28T00:00:00Z",
    }, null, 2)}\n`,
    ".app/import-conflicts.json": `${JSON.stringify({ conflicts: [] }, null, 2)}\n`,
  };
  for (const [relativePath, contents] of Object.entries(appFiles)) {
    await writeFixtureFile(root, relativePath, contents, files);
  }
  for (let index = 0; index < SUPPORT_FILE_COUNT; index += 1) {
    const name = `source-${String(index).padStart(3, "0")}.txt`;
    await writeFixtureFile(
      root,
      path.join("raw", "extracted", name),
      `fixture source ${String(index).padStart(3, "0")}\n`,
      files,
    );
  }

  runGit(["init", "--quiet", "--initial-branch=main", "--template=", root]);
  runGit(["-C", root, "-c", "core.autocrlf=false", "-c", "core.eol=lf", "add", "--all"]);
  runGit([
    "-C", root,
    "-c", "commit.gpgSign=false",
    "-c", "user.name=LLM Wiki Fixture",
    "-c", "user.email=fixture@example.invalid",
    "commit", "--quiet", "--no-gpg-sign", "--no-verify",
    "--date=2026-08-28T00:00:00Z",
    "-m", "Project Facts packaged baseline fixture",
  ], {
    env: {
      ...process.env,
      GIT_AUTHOR_DATE: "2026-08-28T00:00:00Z",
      GIT_COMMITTER_DATE: "2026-08-28T00:00:00Z",
    },
  });
  const tree = runGit(["-C", root, "rev-parse", "HEAD^{tree}"]);
  return { files, directories: NATIVE_DIRECTORIES, tree: tree.stdout.trim() };
}

async function prepareMarkerlessDirectory(root) {
  const files = [];
  for (const [name, title] of [["首页.md", "首页"], ["资料.md", "资料"], ["笔记.md", "笔记"]]) {
    await writeFixtureFile(root, name, `# ${title}\n\nMarkerless compatible control fixture.\n`, files);
  }
  return files;
}

async function prepareFakeAgents(root, mode) {
  const files = [];
  const implementation = `const mode = process.argv[2];
const forwardedArgs = process.argv.slice(3);
if (mode === "slow") await new Promise((resolve) => setTimeout(resolve, 5000));
if (mode === "fail") {
  process.stderr.write("controlled fake Agent failure\\n");
  process.exit(23);
}
if (forwardedArgs.includes("--version")) process.stdout.write("fake-agent 1.0.0\\n");
else process.stdout.write("--print --output-format --verbose --permission-mode --settings --bare --safe-mode --disable-slash-commands --no-session-persistence --no-chrome --prompt-suggestions --strict-mcp-config --tools --allowedTools --json-schema --json --ephemeral --sandbox --ignore-user-config --ignore-rules --output-schema --output-last-message --skip-git-repo-check --cd --message-file --cwd --no-auth-env-only -z\\n");
`;
  await writeFixtureFile(root, "fake-agent.mjs", implementation, files);
  for (const command of ["claude", "codex", "openclaw", "hermes"]) {
    await writeFixtureFile(
      root,
      `${command}.cmd`,
      `@echo off\r\nnode "%~dp0fake-agent.mjs" ${mode} %*\r\n`,
      files,
    );
    await writeFixtureFile(
      root,
      command,
      `#!/usr/bin/env sh\nexec node "$(dirname "$0")/fake-agent.mjs" ${mode} "$@"\n`,
      files,
    );
    await chmod(path.join(root, command), 0o755);
  }
  return files;
}

export async function prepareProjectFactsPackagedFixtures(outputRoot) {
  try {
    await stat(outputRoot);
    throw new Error("--output-root must not already exist.");
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }

  const nativeRoot = path.join(outputRoot, "native-git-3-pages");
  const markerlessRoot = path.join(outputRoot, "markerless-control");
  await mkdir(outputRoot, { recursive: false });
  const native = await prepareNativeProject(nativeRoot);
  const markerlessFiles = await prepareMarkerlessDirectory(markerlessRoot);
  const fakeAgents = [];
  for (const mode of ["slow", "fail", "healthy"]) {
    const directory = `fake-agent-${mode}-bin`;
    const files = await prepareFakeAgents(path.join(outputRoot, directory), mode);
    fakeAgents.push({ directory, files, mode });
  }
  const allFiles = [
    ...native.files.map((file) => ({ ...file, path: `native-git-3-pages/${file.path}` })),
    ...native.directories.map((directory) => ({
      path: `native-git-3-pages/${normalized(directory)}/`,
      contents: "directory",
    })),
    ...markerlessFiles.map((file) => ({ ...file, path: `markerless-control/${file.path}` })),
    ...fakeAgents.flatMap(({ directory, files }) =>
      files.map((file) => ({ ...file, path: `${directory}/${file.path}` }))
    ),
  ];
  const manifest = {
    schemaVersion: 1,
    fixtureHash: fixtureHash(allFiles),
    native: {
      wikiPages: 3,
      supportFiles: SUPPORT_FILE_COUNT,
      trackedFiles: native.files.length,
      requiredDirectories: native.directories,
      gitRepository: true,
      initialBranch: "main",
      gitTree: native.tree,
    },
    markerless: {
      markdownFiles: markerlessFiles.length,
      gitRepository: false,
      appMarkers: false,
    },
    fakeAgent: {
      commands: ["claude", "codex", "openclaw", "hermes"],
      modes: fakeAgents.map(({ mode }) => mode),
      slowDelayMs: 5_000,
      failureExitCode: 23,
    },
  };
  await writeFile(
    path.join(outputRoot, "fixture-manifest.json"),
    `${JSON.stringify(manifest, null, 2)}\n`,
    "utf8",
  );
  return manifest;
}

function argument(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const outputRoot = argument("--output-root");
  if (!outputRoot) throw new Error("--output-root is required.");
  const manifest = await prepareProjectFactsPackagedFixtures(path.resolve(outputRoot));
  process.stdout.write(`${JSON.stringify(manifest, null, 2)}\n`);
}
