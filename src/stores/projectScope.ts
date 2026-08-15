import type { ProjectResourceKind } from "../lib/projectResourceFreshness";

let projectScopeEpoch = 0;

export function captureProjectScope(): number {
  return projectScopeEpoch;
}

export function invalidateProjectScope(): void {
  ++projectScopeEpoch;
}

export function isProjectScopeCurrent(epoch: number): boolean {
  return epoch === projectScopeEpoch;
}

export interface ProjectResourceScope {
  projectId: string;
  rootPath: string;
}

export interface ProjectResourceHandler {
  invalidate: (scope: ProjectResourceScope) => unknown;
}

type Revalidate = (scope: ProjectResourceScope) => void | Promise<void>;

const resourceHandlers = new Map<ProjectResourceKind, [ProjectResourceHandler, Revalidate]>();
const resourceObservers = new Set<ProjectResourceKind>();

export function registerProjectResource(
  kind: ProjectResourceKind,
  handler: ProjectResourceHandler,
  revalidate: Revalidate,
): () => void {
  resourceHandlers.set(kind, [handler, revalidate]);
  return () => resourceHandlers.delete(kind);
}

export function observeProjectResources(
  scope: ProjectResourceScope,
  kinds: readonly ProjectResourceKind[],
): () => void {
  for (const kind of kinds) resourceObservers.add(kind);
  return () => {
    for (const kind of kinds) resourceObservers.delete(kind);
  };
}

export function invalidateProjectResources(
  scope: ProjectResourceScope,
  kinds: readonly ProjectResourceKind[],
  revalidateObserved = false,
): void {
  for (const kind of kinds) {
    const registered = resourceHandlers.get(kind);
    if (!registered) continue;
    registered[0].invalidate(scope);
    if (revalidateObserved && resourceObservers.has(kind)) {
      void Promise.resolve(registered[1](scope)).catch(() => undefined);
    }
  }
}

const ALL_PROJECT_RESOURCES: readonly ProjectResourceKind[] = [
  "wiki",
  "exports",
  "chat-sessions",
  "graph",
  "lint-ignores",
  "lint-history",
  "settings-chat-authorization",
];

export function invalidateObservedProjectResourcesOnFocus(scope: ProjectResourceScope): void {
  invalidateProjectResources(scope, ALL_PROJECT_RESOURCES, true);
}
