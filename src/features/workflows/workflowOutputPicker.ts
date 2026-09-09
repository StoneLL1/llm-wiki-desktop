import { save } from "@tauri-apps/plugin-dialog";

export interface WorkflowOutputPickerOptions {
  projectRoot: string;
  exportRoot?: string;
  currentPath: string;
  title: string;
}

// Rust's canonical Windows roots can use an extended prefix that native dialogs omit.
function dialogPath(value: string): string {
  return value.replaceAll("\\", "/").replace(/^\/\/\?\/UNC\//i, "//").replace(/^\/\/\?\//, "");
}

/** Presentation conversion only; the workflow backend still validates paths and writes. */
export function workflowOutputRelativePath(absolutePath: string, projectRoot: string, exportRoot: string): string {
  const root = dialogPath(projectRoot).replace(/\/+$/, "");
  const path = dialogPath(absolutePath);
  const windows = /^[a-z]:\//i.test(root) || root.startsWith("//");
  const comparable = (value: string) => windows ? value.toLowerCase() : value;
  if (!comparable(path).startsWith(`${comparable(root)}/`)) throw new Error("outsideExportRoot");
  const relative = path.slice(root.length + 1);
  const directory = exportRoot.replaceAll("\\", "/").replace(/\/+$/, "");
  if (!relative.startsWith(`${directory}/`) || relative.split("/").some((part) => !part || part === "." || part === "..")) {
    throw new Error("outsideExportRoot");
  }
  if (!/\.html$/i.test(relative)) throw new Error("htmlOnly");
  return relative;
}

export async function pickWorkflowOutputPath(options: WorkflowOutputPickerOptions): Promise<string | null> {
  if (!options.exportRoot) throw new Error("exportRootUnavailable");
  const root = dialogPath(options.projectRoot).replace(/\/+$/, "");
  const initial = options.currentPath || `${options.exportRoot}/report.html`;
  const selected = await save({
    title: options.title,
    defaultPath: `${root}/${initial}`,
    filters: [{ name: "HTML", extensions: ["html"] }],
  });
  return selected === null ? null : workflowOutputRelativePath(selected, options.projectRoot, options.exportRoot);
}
