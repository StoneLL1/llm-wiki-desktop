import { readdir, readFile } from "node:fs/promises";
import path from "node:path";

const roots = ["src", path.join("src-tauri", "src")];
const extensions = new Set([".cjs", ".js", ".jsx", ".mjs", ".ts", ".tsx"]);
const pattern = /\bconsole\.log\s*\(/g;

const findFiles = async (dir) => {
  const entries = await readdir(dir, { withFileTypes: true });
  const files = [];

  for (const entry of entries) {
    const entryPath = path.join(dir, entry.name);

    if (entry.isDirectory()) {
      files.push(...(await findFiles(entryPath)));
      continue;
    }

    if (entry.isFile() && extensions.has(path.extname(entry.name))) {
      files.push(entryPath);
    }
  }

  return files;
};

const lineAndColumn = (content, index) => {
  const before = content.slice(0, index);
  const lines = before.split(/\r\n|\r|\n/);

  return {
    line: lines.length,
    column: lines.at(-1).length + 1,
  };
};

const matches = [];

for (const root of roots) {
  const files = await findFiles(root);

  for (const file of files) {
    const content = await readFile(file, "utf8");

    for (const match of content.matchAll(pattern)) {
      const location = lineAndColumn(content, match.index ?? 0);
      matches.push(`${file}:${location.line}:${location.column}`);
    }
  }
}

if (matches.length > 0) {
  console.error("Unexpected console.log calls found:");
  for (const match of matches) {
    console.error(`- ${match}`);
  }
  process.exitCode = 1;
} else {
  console.log("No unexpected console.log calls found.");
}
