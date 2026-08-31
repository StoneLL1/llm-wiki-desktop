#!/usr/bin/env node
// Verifies that the committed Tauri schemas under src-tauri/gen/schemas
// match a fresh regeneration (run `npm run check:rust:gui` first).
//
// The comparison is structural with \r\n -> \n normalization inside
// every string value, not byte-for-byte: tauri's ACL pipeline can emit
// CRLF inside embedded permission descriptions on some build hosts
// (observed: the updater default permission set on a Windows runner)
// while producing LF elsewhere, so a byte gate is not portable across
// CI platforms. Formatting differences (pretty-printing) are likewise
// not staleness; missing or changed permissions, permission sets, and
// capabilities still fail this check.

import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

const SCHEMA_FILES = [
  "acl-manifests.json",
  "desktop-schema.json",
  "windows-schema.json",
];

const normalize = (value) => {
  if (typeof value === "string") {
    return value.replace(/\r\n/g, "\n");
  }
  if (Array.isArray(value)) {
    return value.map(normalize);
  }
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value).map(([key, entry]) => [key, normalize(entry)]),
    );
  }
  return value;
};

let failed = false;
for (const file of SCHEMA_FILES) {
  const path = `src-tauri/gen/schemas/${file}`;
  const committed = JSON.parse(
    execFileSync("git", ["show", `HEAD:${path}`], {
      encoding: "utf8",
      maxBuffer: 64 * 1024 * 1024,
    }),
  );
  const regenerated = JSON.parse(readFileSync(path, "utf8"));
  const same =
    JSON.stringify(normalize(committed)) ===
    JSON.stringify(normalize(regenerated));
  if (!same) {
    console.error(
      `::error::${path} does not match regeneration. Run 'npm run check:rust:gui' locally and commit the regenerated schemas in the same change.`,
    );
    failed = true;
  } else {
    console.log(`${path}: matches regeneration (newline-normalized compare)`);
  }
}
process.exit(failed ? 1 : 0);
