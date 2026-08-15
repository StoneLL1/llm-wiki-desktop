import type { AppView } from "../../stores/navigationStore";

type AsyncLoader = () => Promise<unknown>;

export function createWorkspaceViewLoaderRegistry<Loaders extends Record<string, AsyncLoader>>(
  loaders: Loaders,
) {
  const requests = new Map<keyof Loaders, Promise<unknown>>();

  const load = <View extends keyof Loaders>(view: View): ReturnType<Loaders[View]> => {
    const existing = requests.get(view);
    if (existing) return existing as ReturnType<Loaders[View]>;

    const request = loaders[view]();
    requests.set(view, request);
    request.catch(() => requests.delete(view));
    return request as ReturnType<Loaders[View]>;
  };

  return { load, preload: load };
}

const workspaceViewRegistry = createWorkspaceViewLoaderRegistry({
  wiki: () => import("../../features/wiki/WikiView"),
  chat: () => import("../../features/chat/ChatView"),
  graph: () => import("../../features/graph/GraphView"),
  workflows: () => import("../../features/workflows/WorkflowsView"),
  import: () => import("../../features/import/ImportView"),
  lint: () => import("../../features/lint/LintView"),
  exports: () => import("../../features/exports/ExportsView"),
});

export const loadWorkspaceView = workspaceViewRegistry.load;

export function preloadWorkspaceView(view: AppView): Promise<unknown> {
  if (view === "dashboard") return Promise.resolve();
  return workspaceViewRegistry.preload(view);
}
