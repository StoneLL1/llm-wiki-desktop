import { execFile } from "node:child_process";
import fs from "node:fs/promises";
import path from "node:path";
import { promisify } from "node:util";

const run = promisify(execFile);
const magic = new Set(["cffaedfe", "cefaedfe", "feedfacf", "feedface", "cafebabe", "bebafeca"]);

// Upstream dylibs can carry stale ad-hoc signatures. Normalize them before
// calculating the pack's signed inventory, never after installation.
export async function prepareMacosRuntime(root) {
  if (process.platform !== "darwin") return;
  for (const entry of await fs.readdir(root, { withFileTypes: true })) {
    const file = path.join(root, entry.name);
    if (entry.isSymbolicLink()) throw new Error(`Runtime must be staged without symlinks: ${file}`);
    if (entry.isDirectory()) { await prepareMacosRuntime(file); continue; }
    if (!entry.isFile()) continue;
    const handle = await fs.open(file, "r");
    let header;
    try { header = await handle.read(Buffer.alloc(4), 0, 4, 0); } finally { await handle.close(); }
    if (!magic.has(header.buffer.toString("hex"))) continue;
    try { await run("codesign", ["--verify", "--strict", file]); }
    catch {
      await run("codesign", ["--force", "--sign", "-", "--timestamp=none", file]);
      await run("codesign", ["--verify", "--strict", file]);
    }
  }
}
