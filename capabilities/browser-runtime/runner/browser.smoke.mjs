import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { chromium } from "playwright";

const profile = await fs.mkdtemp(path.join(os.tmpdir(), "llm-wiki-browser-smoke-"));
try {
  const context = await chromium.launchPersistentContext(profile, {
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
  let popupSeen = false;
  page.on("popup", async (popup) => {
    popupSeen = true;
    await popup.close();
  });
  await page.setContent('<a id="popup" target="_blank" href="about:blank">open</a><input type="file" id="upload">');
  await page.click("#popup");
  await page.waitForTimeout(100);
  assert.equal(popupSeen, true);
  assert.equal(await page.locator("input[type=file]").count(), 1);
  await context.close();
} finally {
  await fs.rm(profile, { recursive: true, force: true });
}
assert.equal(await fs.stat(profile).then(() => true, () => false), false);
