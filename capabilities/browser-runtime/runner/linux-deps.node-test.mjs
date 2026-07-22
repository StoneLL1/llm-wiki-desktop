import assert from "node:assert/strict";
import test from "node:test";

import { assertLinuxBrowserDependencies } from "./linux-deps.mjs";

test("skips ldd outside Linux", () => {
  let called = false;
  assertLinuxBrowserDependencies("chromium", () => { called = true; }, "darwin");
  assert.equal(called, false);
});

test("reports the exact missing Linux Chromium libraries", () => {
  assert.throws(
    () => assertLinuxBrowserDependencies(
      "chromium",
      () => ({ status: 0, stdout: "libnss3.so => not found\nlibX11.so.6 => /lib/libX11.so.6", stderr: "" }),
      "linux",
    ),
    /IMPORT_WEB_BROWSER_DEPENDENCY_MISSING.*libnss3\.so/,
  );
});

test("accepts a complete Linux dynamic dependency set", () => {
  assert.doesNotThrow(() => assertLinuxBrowserDependencies(
    "chromium",
    () => ({ status: 0, stdout: "libnss3.so => /lib/libnss3.so", stderr: "" }),
    "linux",
  ));
});
