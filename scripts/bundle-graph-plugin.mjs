import path from "node:path";
import { gzipSync } from "node:zlib";

const normalizeSlashes = (value) => value.replaceAll("\\", "/");

const byteLength = (value) => Buffer.byteLength(
  typeof value === "string" ? value : Buffer.from(value),
);

const gzipByteLength = (value) => gzipSync(
  typeof value === "string" ? value : Buffer.from(value),
).byteLength;

export const normalizeModuleId = (moduleId, repositoryRoot = process.cwd()) => {
  if (!moduleId) {
    return null;
  }

  const withoutNullPrefix = moduleId.replace(/^\0/, "virtual:");
  if (withoutNullPrefix.startsWith("virtual:")) {
    return withoutNullPrefix;
  }

  const withoutQuery = withoutNullPrefix.split("?", 1)[0];
  const relative = path.relative(repositoryRoot, withoutQuery);
  if (relative && !relative.startsWith("..") && !path.isAbsolute(relative)) {
    return normalizeSlashes(relative);
  }

  const normalized = normalizeSlashes(withoutQuery);
  const nodeModulesMarker = "/node_modules/";
  const nodeModulesIndex = normalized.lastIndexOf(nodeModulesMarker);
  if (nodeModulesIndex >= 0) {
    return normalized.slice(nodeModulesIndex + 1);
  }

  return `<external>/${path.posix.basename(normalized)}`;
};

const sortedUnique = (values) => [...new Set(values)].sort();

const sourceForOutput = (output) => (
  output.type === "chunk" ? output.code : output.source
);

const collectInitialClosure = (files, entries) => {
  const jsFiles = new Set();
  const cssFiles = new Set();
  const pending = [...entries];

  while (pending.length > 0) {
    const fileName = pending.pop();
    if (jsFiles.has(fileName)) {
      continue;
    }

    const file = files[fileName];
    if (!file || file.type !== "chunk") {
      continue;
    }

    jsFiles.add(fileName);
    for (const importedFile of file.imports) {
      pending.push(importedFile);
    }
    for (const cssFile of file.importedCss) {
      if (files[cssFile]?.type === "asset") {
        cssFiles.add(cssFile);
      }
    }
  }

  return {
    jsFiles: [...jsFiles].sort(),
    cssFiles: [...cssFiles].sort(),
  };
};

const summarizeFiles = (fileNames, files) => ({
  fileCount: fileNames.length,
  rawBytes: fileNames.reduce((total, fileName) => total + files[fileName].rawBytes, 0),
  gzipBytes: fileNames.reduce((total, fileName) => total + files[fileName].gzipBytes, 0),
});

const collectModuleContributors = (jsFiles, files) => {
  const contributors = new Map();

  for (const fileName of jsFiles) {
    for (const module of files[fileName].modules) {
      const current = contributors.get(module.id) ?? {
        moduleId: module.id,
        renderedBytes: 0,
        chunks: [],
      };
      current.renderedBytes += module.renderedBytes;
      current.chunks.push(fileName);
      contributors.set(module.id, current);
    }
  }

  return [...contributors.values()]
    .map((contributor) => ({
      ...contributor,
      chunks: sortedUnique(contributor.chunks),
    }))
    .sort((left, right) => (
      right.renderedBytes - left.renderedBytes
      || left.moduleId.localeCompare(right.moduleId)
    ));
};

export const createBundleGraph = (bundle, options = {}) => {
  const repositoryRoot = path.resolve(options.root ?? process.cwd());
  const files = {};

  for (const fileName of Object.keys(bundle).sort()) {
    const output = bundle[fileName];
    const source = sourceForOutput(output);

    if (output.type === "chunk") {
      const modules = Object.entries(output.modules)
        .map(([moduleId, details]) => ({
          id: normalizeModuleId(moduleId, repositoryRoot),
          renderedBytes: details.renderedLength ?? 0,
        }))
        .filter((module) => module.id)
        .sort((left, right) => left.id.localeCompare(right.id));
      files[fileName] = {
        type: "chunk",
        rawBytes: byteLength(source),
        gzipBytes: gzipByteLength(source),
        isEntry: output.isEntry,
        name: output.name,
        facadeModuleId: normalizeModuleId(output.facadeModuleId, repositoryRoot),
        imports: sortedUnique(output.imports),
        dynamicImports: sortedUnique(output.dynamicImports),
        importedCss: sortedUnique(output.viteMetadata?.importedCss ?? []),
        moduleIds: modules.map(({ id }) => id),
        modules,
      };
      continue;
    }

    if (fileName.endsWith(".css")) {
      files[fileName] = {
        type: "asset",
        rawBytes: byteLength(source),
        gzipBytes: gzipByteLength(source),
      };
    }
  }

  const entries = Object.entries(files)
    .filter(([, file]) => file.type === "chunk" && file.isEntry)
    .map(([fileName]) => fileName)
    .sort();
  const closure = collectInitialClosure(files, entries);

  return {
    schemaVersion: 1,
    entries,
    files,
    initial: {
      jsFiles: closure.jsFiles,
      cssFiles: closure.cssFiles,
      js: summarizeFiles(closure.jsFiles, files),
      css: summarizeFiles(closure.cssFiles, files),
      moduleContributors: collectModuleContributors(closure.jsFiles, files),
    },
  };
};

export const bundleGraphPlugin = (options = {}) => ({
  name: "llm-wiki-bundle-graph",
  apply: "build",
  enforce: "post",
  generateBundle(_outputOptions, bundle) {
    const graph = createBundleGraph(bundle, options);
    this.emitFile({
      type: "asset",
      fileName: options.fileName ?? "bundle-graph.json",
      source: `${JSON.stringify(graph, null, 2)}\n`,
    });
  },
});
