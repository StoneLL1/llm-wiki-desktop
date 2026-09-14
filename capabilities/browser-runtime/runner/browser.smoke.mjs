import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { chromium } from "playwright";

const profile = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-browser-smoke-"));
let context;
try {
  context = await chromium.launchPersistentContext(profile, {
    headless: true,
    acceptDownloads: false,
    args: [
      "--disable-extensions",
      "--disable-background-networking",
      "--disable-component-update",
      "--disable-sync",
    ],
  });
  const page = await context.newPage();
  await page.setContent('<a id="popup" target="_blank" href="about:blank">open</a><input type="file" id="upload">');
  const [popup] = await Promise.all([
    page.waitForEvent("popup", { timeout: 10_000 }),
    page.click("#popup"),
  ]);
  await popup.close();
  assert.equal(await page.locator("input[type=file]").count(), 1);
} finally {
  await context?.close();
  await fs.rm(profile, { recursive: true, force: true });
}
assert.equal(await fs.stat(profile).then(() => true, () => false), false);
