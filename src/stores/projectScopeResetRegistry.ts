export type ProjectScopeResetHandler = () => void | Promise<void>;

export interface ProjectScopeResetRegistry {
  register: (id: string, handler: ProjectScopeResetHandler) => () => void;
  reset: () => void;
}

function reportResetFailure(id: string, error: unknown): void {
  console.error(`[project-scope] reset handler failed: ${id}`, error);
}

export function createProjectScopeResetRegistry(): ProjectScopeResetRegistry {
  const handlers = new Map<string, ProjectScopeResetHandler>();

  return {
    register(id, handler) {
      handlers.set(id, handler);
      return () => {
        if (handlers.get(id) === handler) handlers.delete(id);
      };
    },
    reset() {
      for (const [id, handler] of [...handlers.entries()]) {
        try {
          const result = handler();
          if (result instanceof Promise) {
            void result.catch((error: unknown) => reportResetFailure(id, error));
          }
        } catch (error) {
          reportResetFailure(id, error);
        }
      }
    },
  };
}

const projectScopeResetRegistry = createProjectScopeResetRegistry();

export const registerProjectScopeResetHandler = projectScopeResetRegistry.register;
export const resetProjectScopedStores = projectScopeResetRegistry.reset;
