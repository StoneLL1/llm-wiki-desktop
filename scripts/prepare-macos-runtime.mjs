import { execFile } from "node:child_process";
import fs from "node:fs/promises";
import os from "node:os";
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
      // Payloads materialize framework symlinks as ordinary files. codesign
      // infers a bundle from the surrounding Resources/Versions directories,
      // then rejects that flattened layout as ambiguous. Inspect the actual
      // Mach-O outside its bundle so valid upstream signatures remain intact.
      const temporary = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-macos-sign-"));
      try {
        const standalone = path.join(temporary, "runtime");
        await fs.copyFile(file, standalone);
        try { await run("codesign", ["--verify", "--strict", standalone]); }
        catch {
          await run("codesign", ["--force", "--sign", "-", "--timestamp=none", standalone]);
          await run("codesign", ["--verify", "--strict", standalone]);
          await fs.copyFile(standalone, file);
        }
      } finally {
        await fs.rm(temporary, { recursive: true, force: true });
      }
    }
  }
}
