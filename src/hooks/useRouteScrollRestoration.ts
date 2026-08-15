import {
  useCallback,
  useLayoutEffect,
  useRef,
  type RefCallback,
  type RefObject,
} from "react";

import { projectResourceKey } from "../lib/projectResourceFreshness";
import type { ChatRoutePreference } from "../types/chat";

let activeProjectKey: string | null = null;
let scrollPositions = new Map<string, number>();
let graphCameraSnapshot: GraphCameraSnapshot | null = null;
let chatRoutePreference: ChatRoutePreference = "auto";

export interface GraphCameraSnapshot {
  contentHash: string;
  x: number;
  y: number;
  ratio: number;
  angle: number;
}

export function activateRoutePresentationProject(projectKey: string): void {
  if (activeProjectKey === projectKey) return;
  activeProjectKey = projectKey;
  scrollPositions = new Map();
  graphCameraSnapshot = null;
  chatRoutePreference = "auto";
}

export function saveRouteScrollPosition(projectKey: string, routeKey: string, scrollTop: number): void {
  if (activeProjectKey !== projectKey || !Number.isFinite(scrollTop) || scrollTop < 0) return;
  scrollPositions.set(routeKey, scrollTop);
}

export function readRouteScrollPosition(projectKey: string, routeKey: string): number | null {
  if (activeProjectKey !== projectKey) return null;
  const value = scrollPositions.get(routeKey);
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? value : null;
}

export function resetRoutePresentation(): void {
  activeProjectKey = null;
  scrollPositions = new Map();
  graphCameraSnapshot = null;
  chatRoutePreference = "auto";
}

export function saveGraphCameraSnapshot(snapshot: GraphCameraSnapshot | null): void {
  graphCameraSnapshot = snapshot;
}

export function readGraphCameraSnapshot(): GraphCameraSnapshot | null {
  return graphCameraSnapshot;
}

export function saveChatRoutePreference(route: ChatRoutePreference): void {
  chatRoutePreference = route;
}

export function readChatRoutePreference(): ChatRoutePreference {
  return chatRoutePreference;
}

export function useRouteScrollRestoration(
  projectId: string,
  rootPath: string,
  routeKey: string,
): RefObject<HTMLDivElement | null> {
  const ref = useRef<HTMLDivElement>(null);
  const projectKey = projectResourceKey(projectId, rootPath);

  useLayoutEffect(() => {
    activateRoutePresentationProject(projectKey);
    const restored = readRouteScrollPosition(projectKey, routeKey);
    if (restored !== null && ref.current) ref.current.scrollTop = restored;

    return () => {
      const scrollTop = ref.current?.scrollTop;
      if (typeof scrollTop === "number") {
        saveRouteScrollPosition(projectKey, routeKey, scrollTop);
      }
    };
  }, [projectKey, routeKey]);

  return ref;
}

/**
 * Callback-ref variant for scroll owners that are swapped inside a retained
 * route (for example Wiki read/edit modes). Detachment saves the old surface
 * before React replaces it, and attachment restores the new surface.
 */
export function useRouteScrollCallbackRestoration<T extends HTMLElement>(
  projectId: string,
  rootPath: string,
  routeKey: string,
): RefCallback<T> {
  const ref = useRef<T | null>(null);
  const projectKey = projectResourceKey(projectId, rootPath);

  return useCallback((node: T | null) => {
    const previous = ref.current;
    if (previous && previous !== node) {
      saveRouteScrollPosition(projectKey, routeKey, previous.scrollTop);
    }

    ref.current = node;
    if (!node) return;
    activateRoutePresentationProject(projectKey);
    const restored = readRouteScrollPosition(projectKey, routeKey);
    if (restored !== null) node.scrollTop = restored;
  }, [projectKey, routeKey]);
}
