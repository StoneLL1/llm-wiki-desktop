import { readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";

import { describe, expect, it } from "vitest";

/**
 * Rust services hand the UI translation keys (`message_key`) instead of prose.
 * `t()` renders an unknown key verbatim, so a key that exists only in Rust
 * surfaces raw dotted identifiers to users. Nothing else in the gate compares
 * the two sides, so this contract does.
 */
const rootPath = (relativePath: string) => path.join(process.cwd(), relativePath);

function collectFiles(directory: string, extension: string): string[] {
  const entries = readdirSync(directory);
  return entries.flatMap((entry) => {
    const full = path.join(directory, entry);
    if (statSync(full).isDirectory()) return collectFiles(full, extension);
    return full.endsWith(extension) ? [full] : [];
  });
}

function flatten(value: unknown, prefix = "", out: Record<string, unknown> = {}) {
  if (value && typeof value === "object" && !Array.isArray(value)) {
    for (const [key, nested] of Object.entries(value)) {
      flatten(nested, prefix ? `${prefix}.${key}` : key, out);
    }
    return out;
  }
  out[prefix] = value;
  return out;
}

function localeKeys(locale: string): Set<string> {
  const parsed: unknown = JSON.parse(
    readFileSync(rootPath(`src/i18n/locales/${locale}.json`), "utf8"),
  );
  return new Set(Object.keys(flatten(parsed)));
}

const LOCALES = ["en", "zh-CN"];

/** Namespaces whose keys the backend emits as `message_key` values. */
const BACKEND_KEY_PATTERN = /"((?:workflows|projectAssessment|projectAuthority)\.[A-Za-z0-9_.]+)"/g;

describe("backend-emitted i18n keys", () => {
  const emitted = new Set<string>();
  for (const file of collectFiles(rootPath("src-tauri/src"), ".rs")) {
    const source = readFileSync(file, "utf8");
    for (const match of source.matchAll(BACKEND_KEY_PATTERN)) emitted.add(match[1]);
  }

  it("finds keys to check", () => {
    expect(emitted.size).toBeGreaterThan(0);
  });

  for (const locale of LOCALES) {
    it(`resolves every backend key in ${locale}`, () => {
      const keys = localeKeys(locale);
      expect([...emitted].filter((key) => !keys.has(key)).sort()).toEqual([]);
    });
  }

  it("keeps both locales in sync for those keys", () => {
    const [en, zh] = LOCALES.map(localeKeys);
    const onlyEn = [...emitted].filter((key) => en.has(key) && !zh.has(key));
    const onlyZh = [...emitted].filter((key) => zh.has(key) && !en.has(key));
    expect({ onlyEn, onlyZh }).toEqual({ onlyEn: [], onlyZh: [] });
  });
});
