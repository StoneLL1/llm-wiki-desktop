import fs from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { verifyCapabilityCatalog } from "./verify-capability-catalog.mjs";

// Development consumes the same signed archives as the desktop installer.
// Downloading a catalog never grants a new publisher key or executes a pack.
export async function prepareImportDevelopment({ root = path.resolve(import.meta.dirname, ".."), tag, fetchImpl = fetch } = {}) {
  const pkg = JSON.parse(await fs.readFile(path.join(root, "package.json"), "utf8"));
  tag ??= `app-v${pkg.version}`;
  if (!/^app-v\d+\.\d+\.\d+(?:-rc\.\d+)?$/.test(tag)) throw new Error("Invalid desktop release tag");
  const trustedKeys = JSON.parse(await fs.readFile(path.join(root, "capabilities/trusted-keys.json"), "utf8"));
  const url = `https://github.com/StoneLL1/llm-wiki-desktop/releases/download/${tag}/install-catalog.json`;
  const response = await fetchImpl(url, { signal: AbortSignal.timeout(60_000) });
  if (!response.ok) throw new Error(`Cannot fetch the capability catalog: HTTP ${response.status}`);
  const text = await response.text();
  if (Buffer.byteLength(text) > 2 * 1024 * 1024) throw new Error("Capability catalog exceeds 2 MiB");
  const catalog = JSON.parse(text);
  const { errors } = verifyCapabilityCatalog({ catalog, trustedKeys, mode: "source" });
  if (!catalog.entries?.length || errors.length) throw new Error(`Invalid capability catalog: ${errors.join("; ") || "empty"}`);
  const destination = path.join(root, ".dev-capabilities/catalog");
  await fs.mkdir(destination, { recursive: true });
  await fs.writeFile(path.join(destination, "trusted-keys.json"), JSON.stringify(trustedKeys, null, 2) + "\n");
  const temporary = path.join(destination, `install-catalog.${process.pid}.tmp`);
  await fs.writeFile(temporary, text);
  await fs.rename(temporary, path.join(destination, "install-catalog.json"));
  process.stdout.write(`Prepared ${catalog.entries.length} signed capability entries from ${tag}. Development builds will embed this catalog.\n`);
  return destination;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  prepareImportDevelopment({ tag: process.argv[2] }).catch((error) => {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  });
}
