import type { WorkflowScope } from "../types/workflow";

function sameSet(a: readonly string[], b: readonly string[]): boolean {
  const selected = new Set(a);
  return a.length === b.length && b.every((value) => selected.has(value));
}

export function workflowScopeEqual(a: WorkflowScope, b: WorkflowScope): boolean {
  if (a.kind === "health_check" && b.kind === "health_check") return a.mode === b.mode;
  if (a.kind === "update_wiki" && b.kind === "update_wiki") {
    const key = (source: { sourceId: string; versionId: string }) => `${source.sourceId}\0${source.versionId}`;
    return a.mode === b.mode && sameSet(a.sourceVersions.map(key), b.sourceVersions.map(key));
  }
  if (a.kind === "generate_content" && b.kind === "generate_content") {
    return a.artifactType === b.artifactType && a.outputPath === b.outputPath && sameSet(a.pagePaths, b.pagePaths);
  }
  return false;
}
