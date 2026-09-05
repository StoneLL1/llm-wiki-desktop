import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { pathToFileURL } from "node:url";

const repositoryRoot = path.join(import.meta.dirname, "..");

/**
 * Utility selectors that only `src/features/wiki/**` components use today.
 * They act as a tripwire for Tailwind v4 source-scanning regressions: the
 * scanner honors .gitignore, so an unanchored ignore pattern (historically
 * `wiki/`, which also matched `src/features/wiki/`) silently removed every
 * wiki-only utility from dev and production CSS while all unit tests kept
 * passing. Keep this list in sync when these classes are renamed or stop
 * being wiki-only; selectors appear in minified CSS with Tailwind's escaping
 * (for example `.py-\[4px\]`).
 */
const WIKI_ONLY_UTILITY_SELECTORS = [
  ".overflow-x-auto",
  ".pr-7",
  ".min-w-\\[132px\\]",
  ".py-\\[4px\\]",
];

export const evaluateTailwindCoverage = ({ cssText, requiredSelectors }) => {
  const missing = requiredSelectors.filter((selector) => !cssText.includes(selector));
  return {
    checkedCount: requiredSelectors.length,
    missing,
  };
};

export const formatCoverageFailure = (result) => [
  "Tailwind utility coverage failed:",
  ...result.missing.map((selector) => `- missing from dist CSS: ${selector}`),
  "",
  "These selectors are only referenced by src/features/wiki components. If they",
  "vanished from the built CSS, a .gitignore pattern is most likely excluding the",
  "wiki feature directory from Tailwind v4's automatic source scanning (it honors",
  ".gitignore). Anchor directory ignores to the repository root (for example",
  "`/wiki/`, never `wiki/`) and check `git check-ignore -v <file>` for the",
  "affected source file.",
  "",
].join("\n");

const collectDistCssText = async (distDir) => {
  const assetsDir = path.join(distDir, "assets");
  let entries;
  try {
    entries = await fs.readdir(assetsDir, { withFileTypes: true });
  } catch (error) {
    throw new Error(
      `Cannot read ${assetsDir}: run \`npm run build\` before this check (${error.message}).`,
    );
  }

  const cssFiles = entries
    .filter((entry) => entry.isFile() && entry.name.endsWith(".css"))
    .map((entry) => path.join(assetsDir, entry.name));
  if (cssFiles.length === 0) {
    throw new Error(`No CSS assets found under ${assetsDir}; run \`npm run build\` first.`);
  }

  const contents = await Promise.all(cssFiles.map((file) => fs.readFile(file, "utf8")));
  return { cssFiles, cssText: contents.join("\n") };
};

const run = async () => {
  const { cssFiles, cssText } = await collectDistCssText(path.join(repositoryRoot, "dist"));
  const result = evaluateTailwindCoverage({
    cssText,
    requiredSelectors: WIKI_ONLY_UTILITY_SELECTORS,
  });

  if (result.missing.length > 0) {
    process.stderr.write(formatCoverageFailure(result));
    process.exitCode = 1;
    return;
  }

  process.stdout.write(
    [
      "Tailwind utility coverage passed:",
      `- inspected ${cssFiles.length} dist CSS asset(s)`,
      `- verified ${result.checkedCount} wiki-only utility selector(s)`,
      "",
    ].join("\n"),
  );
};

const isMain = process.argv[1]
  && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url;
if (isMain) {
  await run();
}
