export type ProjectResourceKind =
  | "wiki"
  | "exports"
  | "chat-sessions"
  | "graph"
  | "lint-ignores"
  | "lint-history"
  | "settings-chat-authorization";

export function projectResourceKey(projectId: string, rootPath: string): string {
  return `${projectId}\0${rootPath}`;
}

interface ProjectResourceScope {
  projectId: string;
  rootPath: string;
}

export function createProjectResourceController<T>(kind: ProjectResourceKind) {
  const ttl = kind === "graph" ? 60_000 : kind === "settings-chat-authorization" ? 30_000 : 15_000;
  let requestEpoch = 0;
  let inFlight: { key: string; promise: Promise<T>; dirty: boolean } | null = null;
  let loadedProjectKey = "";
  let updatedAt = 0;

  return {
    beginRequest(): number {
      return ++requestEpoch;
    },
    isCurrent(epoch: number): boolean {
      return epoch === requestEpoch;
    },
    epoch(): number {
      return requestEpoch;
    },
    ensure(
      scope: ProjectResourceScope,
      load: () => Promise<T>,
      current?: T,
    ): Promise<T> {
      const key = projectResourceKey(scope.projectId, scope.rootPath);
      if (loadedProjectKey === key && updatedAt > 0 && Date.now() - updatedAt < ttl) {
        return Promise.resolve(current as T);
      }
      if (inFlight?.key === key) return inFlight.promise;
      const active: { key: string; promise: Promise<T>; dirty: boolean } = {
        key,
        promise: null as unknown as Promise<T>,
        dirty: false,
      };
      active.promise = (async () => {
        let result: T;
        do {
          active.dirty = false;
          result = await load();
        } while (active.dirty);
        return result;
      })();
      inFlight = active;
      void active.promise.finally(() => {
        if (inFlight === active) inFlight = null;
      }).catch(() => undefined);
      return active.promise;
    },
    markLoaded(projectId: string, rootPath: string): void {
      loadedProjectKey = projectResourceKey(projectId, rootPath);
      updatedAt = Date.now();
    },
    invalidate(
      scope: ProjectResourceScope,
    ): boolean | undefined {
      const key = projectResourceKey(scope.projectId, scope.rootPath);
      if (loadedProjectKey !== key && inFlight?.key !== key) return;
      ++requestEpoch;
      updatedAt = 0;
      if (inFlight?.key === key) {
        inFlight.dirty = true;
        return true;
      }
    },
    reset(): void {
      inFlight = null;
      loadedProjectKey = "";
      ++requestEpoch;
    },
  };
}
