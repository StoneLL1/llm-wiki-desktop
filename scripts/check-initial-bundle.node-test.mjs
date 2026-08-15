import assert from "node:assert/strict";
import path from "node:path";
import test from "node:test";

import { createBundleGraph } from "./bundle-graph-plugin.mjs";
import {
  evaluateBundleBudget,
  formatBudgetFailure,
} from "./check-initial-bundle.mjs";

const chunk = ({
  fileName,
  code,
  isEntry = false,
  imports = [],
  dynamicImports = [],
  modules = {},
  importedCss = [],
}) => ({
  type: "chunk",
  fileName,
  name: fileName.replace(/\..+$/, ""),
  code,
  isEntry,
  isDynamicEntry: false,
  facadeModuleId: null,
  imports,
  dynamicImports,
  implicitlyLoadedBefore: [],
  importedBindings: {},
  exports: [],
  modules,
  map: null,
  preliminaryFileName: fileName,
  referencedFiles: [],
  sourcemapFileName: null,
  viteMetadata: { importedCss: new Set(importedCss) },
});

const css = (fileName, source) => ({
  type: "asset",
  fileName,
  names: [],
  originalFileNames: [],
  source,
});

const moduleDetails = (renderedLength) => ({ renderedLength });
const fixtureRoot = path.resolve("fixture-repository");

const fixture = (hash) => {
  const entry = `assets/index-${hash}.js`;
  const vendor = `assets/vendor-${hash}.js`;
  const route = `assets/GraphView-${hash}.js`;
  const shellCss = `assets/index-${hash}.css`;
  const routeCss = `assets/GraphView-${hash}.css`;
  return {
    [entry]: chunk({
      fileName: entry,
      code: "import './vendor.js';\nconsole.info('entry');",
      isEntry: true,
      imports: [vendor],
      dynamicImports: [route],
      importedCss: [shellCss],
      modules: { [path.join(fixtureRoot, "src", "main.tsx")]: moduleDetails(24) },
    }),
    [vendor]: chunk({
      fileName: vendor,
      code: "export const vendor = true;",
      modules: { [path.join(fixtureRoot, "node_modules", "react", "index.js")]: moduleDetails(27) },
    }),
    [route]: chunk({
      fileName: route,
      code: "export const graph = true;",
      importedCss: [routeCss],
      modules: {
        [path.join(fixtureRoot, "src", "features", "graph", "GraphView.tsx")]: moduleDetails(128),
      },
    }),
    [shellCss]: css(shellCss, ":root{color:black}"),
    [routeCss]: css(routeCss, ".graph{display:block}"),
  };
};

test("initial bundle statistics do not depend on generated chunk hashes", () => {
  const first = createBundleGraph(fixture("aaaa"), { root: fixtureRoot });
  const second = createBundleGraph(fixture("bbbb"), { root: fixtureRoot });

  assert.deepEqual(first.initial.js, second.initial.js);
  assert.deepEqual(first.initial.css, second.initial.css);
  assert.deepEqual(first.files[first.entries[0]].moduleIds, ["src/main.tsx"]);
  assert.deepEqual(
    first.initial.moduleContributors.map(({ moduleId, renderedBytes }) => ({ moduleId, renderedBytes })),
    second.initial.moduleContributors.map(({ moduleId, renderedBytes }) => ({ moduleId, renderedBytes })),
  );
});

test("dynamic route chunks and their CSS are excluded from the initial static closure", () => {
  const graph = createBundleGraph(fixture("route"), { root: fixtureRoot });

  assert.equal(graph.initial.jsFiles.some((file) => file.includes("GraphView")), false);
  assert.equal(graph.initial.cssFiles.some((file) => file.includes("GraphView")), false);
  assert.equal(
    graph.initial.moduleContributors.some(({ moduleId }) => moduleId.includes("GraphView.tsx")),
    false,
  );
});

test("an oversized initial import fails the budget and reports its module source", () => {
  const bundle = fixture("large");
  const entry = Object.values(bundle).find((output) => output.type === "chunk" && output.isEntry);
  const largeFile = "assets/accidental-large-module-large.js";
  entry.imports.push(largeFile);
  bundle[largeFile] = chunk({
    fileName: largeFile,
    code: `export default "${"x".repeat(1_700_000)}";`,
    modules: {
      [path.join(fixtureRoot, "src", "accidental-large-module.ts")]: moduleDetails(1_700_000),
    },
  });
  const graph = createBundleGraph(bundle, { root: fixtureRoot });
  const result = evaluateBundleBudget(graph, {
    maxInitialJsRawBytes: 1_610_000,
    maxInitialJsGzipBytes: 470_000,
    maxInitialJsFiles: 45,
  });

  assert.deepEqual(result.violations.map(({ metric }) => metric), ["initialJsRawBytes"]);
  assert.match(formatBudgetFailure(graph, result), /src\/accidental-large-module\.ts/);
});

test("denied feature and dependency sources fail even when byte budgets pass", () => {
  const bundle = fixture("denied");
  const entry = Object.values(bundle).find((output) => output.type === "chunk" && output.isEntry);
  const deniedFile = "assets/ImportView-denied.js";
  entry.imports.push(deniedFile);
  bundle[deniedFile] = chunk({
    fileName: deniedFile,
    code: "export const importView = true;",
    modules: {
      [path.join(fixtureRoot, "src", "features", "import", "ImportView.tsx")]: moduleDetails(31),
    },
  });

  const graph = createBundleGraph(bundle, { root: fixtureRoot });
  const result = evaluateBundleBudget(graph, {
    maxInitialJsRawBytes: 1_610_000,
    maxInitialJsGzipBytes: 470_000,
    maxInitialJsFiles: 45,
    deniedInitialModulePatterns: ["src/features/import/ImportView.tsx"],
  });

  assert.deepEqual(result.violations, [{
    metric: "deniedInitialModule",
    moduleId: "src/features/import/ImportView.tsx",
    pattern: "src/features/import/ImportView.tsx",
  }]);
  assert.match(formatBudgetFailure(graph, result), /denied initial module.*ImportView\.tsx/i);
});
