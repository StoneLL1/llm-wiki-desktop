// Build an isolated acceptance pack from already available runtimes. This does
// not install software or modify an installed capability. The caller must record
// the actual Playwright/Chromium versions; this is not release qualification.
import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath, URL } from "node:url";

const [destination, playwrightRoot, chromiumExecutable] = process.argv.slice(2);
if (!destination || !playwrightRoot || !chromiumExecutable) throw new Error("Expected isolated output directory, existing Playwright directory, Chromium executable");
const root = path.resolve(destination);
await fs.mkdir(root, { recursive: false });
await fs.cp(fileURLToPath(new URL(".", import.meta.url)), path.join(root, "runner"), { recursive: true });
await fs.copyFile(process.execPath, path.join(root, "node"));
await fs.chmod(path.join(root, "node"), 0o755);
await fs.mkdir(path.join(root, "node_modules"));
const repositoryModules = fileURLToPath(new URL("../../../node_modules/", import.meta.url));
for (const dependency of ["@mozilla", "dompurify", "jsdom", "turndown"]) {
  await fs.symlink(path.join(repositoryModules, dependency), path.join(root, "node_modules", dependency));
}
await fs.symlink(path.resolve(playwrightRoot), path.join(root, "node_modules/playwright"));
const bootstrap = `import { chromium } from "playwright";
const launch = chromium.launchPersistentContext.bind(chromium);
chromium.executablePath = () => ${JSON.stringify(path.resolve(chromiumExecutable))};
chromium.launchPersistentContext = (profile, options) => launch(profile, { ...options, executablePath: chromium.executablePath() });
export { chromium };
`;
await fs.writeFile(path.join(root, "runner/acceptance-runtime.mjs"), bootstrap);
await fs.writeFile(path.join(root, "runner/acceptance-entry.mjs"), 'import "./acceptance-runtime.mjs";\nawait import("./index.mjs");\n');
await fs.writeFile(path.join(root, "runner/acceptance-seed.mjs"), `import { chromium } from "./acceptance-runtime.mjs";
const [profile, url] = process.argv.slice(2);
const context = await chromium.launchPersistentContext(profile, { headless: true });
try { await context.addCookies([{ name: "acceptance_session", value: "authenticated", url, expires: Date.now() / 1000 + 3600 }]); }
finally { await context.close(); }
`);
const manifest = JSON.parse(await fs.readFile(fileURLToPath(new URL("../manifest.json", import.meta.url)), "utf8"));
manifest.entrypoint = "node";
manifest.entrypointArgs = ["runner/acceptance-entry.mjs"];
await fs.writeFile(path.join(root, "manifest.json"), JSON.stringify(manifest));
const runtime = JSON.parse(await fs.readFile(path.join(playwrightRoot, "package.json"), "utf8"));
process.stdout.write(JSON.stringify({ root, playwrightVersion: runtime.version, chromiumExecutable, sourceManifestVersion: manifest.version }) + "\n");
