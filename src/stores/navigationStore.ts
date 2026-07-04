import { create } from "zustand";
import {
  type ResizablePaneId,
  PANE_WIDTH_LIMITS,
  readLayoutPreferenceSnapshot,
  sanitizeLayoutPreferences,
  writeLayoutPreferenceSnapshot,
} from "../hooks/useResizablePane";

export type AppView =
  | "dashboard"
  | "wiki"
  | "chat"
  | "graph"
  | "agent"
  | "import"
  | "lint"
  | "exports"
  | "settings";

export type RightPanelMode = "default" | "wikiAssistant";

export interface NavigationState {
  activeView: AppView;
  rightPanelOpen: boolean;
  rightPanelMode: RightPanelMode;
  wikiAssistantPagePath: string | null;
  sidebarCollapsed: boolean;
  paneSizes: Record<ResizablePaneId, number>;
  setActiveView: (view: AppView) => void;
  setRightPanelOpen: (open: boolean) => void;
  openWikiAssistant: (path: string) => void;
  setWikiAssistantPagePath: (path: string | null) => void;
  closeWikiAssistant: () => void;
  setSidebarCollapsed: (collapsed: boolean) => void;
  toggleSidebarCollapsed: () => void;
  setPaneSize: (pane: ResizablePaneId, width: number) => void;
  resetPaneSize: (pane: ResizablePaneId) => void;
}

const initialLayoutPreferences = readLayoutPreferenceSnapshot();

export const useNavigationStore = create<NavigationState>((set) => ({
  activeView: "dashboard",
  rightPanelOpen: true,
  rightPanelMode: "default",
  wikiAssistantPagePath: null,
  sidebarCollapsed: initialLayoutPreferences.sidebarCollapsed,
  paneSizes: initialLayoutPreferences.paneSizes,
  setActiveView: (activeView) =>
    set((state) => {
      if (activeView === "wiki") {
        return { activeView };
      }
      if (state.rightPanelMode === "default" && state.wikiAssistantPagePath === null) {
        return { activeView };
      }
      return {
        activeView,
        rightPanelMode: "default",
        wikiAssistantPagePath: null,
      };
    }),
  setRightPanelOpen: (rightPanelOpen) => set({ rightPanelOpen }),
  openWikiAssistant: (wikiAssistantPagePath) =>
    set({
      activeView: "wiki",
      rightPanelOpen: true,
      rightPanelMode: "wikiAssistant",
      wikiAssistantPagePath,
    }),
  setWikiAssistantPagePath: (wikiAssistantPagePath) => set({ wikiAssistantPagePath }),
  closeWikiAssistant: () =>
    set({
      rightPanelMode: "default",
      wikiAssistantPagePath: null,
    }),
  setSidebarCollapsed: (sidebarCollapsed) =>
    set((state) => {
      const snapshot = sanitizeLayoutPreferences({
        sidebarCollapsed,
        paneSizes: state.paneSizes,
      });
      writeLayoutPreferenceSnapshot(snapshot);

      return {
        sidebarCollapsed: snapshot.sidebarCollapsed,
        paneSizes: snapshot.paneSizes,
      };
    }),
  toggleSidebarCollapsed: () =>
    set((state) => {
      const snapshot = sanitizeLayoutPreferences({
        sidebarCollapsed: !state.sidebarCollapsed,
        paneSizes: state.paneSizes,
      });
      writeLayoutPreferenceSnapshot(snapshot);

      return {
        sidebarCollapsed: snapshot.sidebarCollapsed,
        paneSizes: snapshot.paneSizes,
      };
    }),
  setPaneSize: (pane, width) =>
    set((state) => {
      const snapshot = sanitizeLayoutPreferences({
        sidebarCollapsed: state.sidebarCollapsed,
        paneSizes: {
          ...state.paneSizes,
          [pane]: width,
        },
      });
      writeLayoutPreferenceSnapshot(snapshot);

      return {
        sidebarCollapsed: snapshot.sidebarCollapsed,
        paneSizes: snapshot.paneSizes,
      };
    }),
  resetPaneSize: (pane) =>
    set((state) => {
      const snapshot = sanitizeLayoutPreferences({
        sidebarCollapsed: state.sidebarCollapsed,
        paneSizes: {
          ...state.paneSizes,
          [pane]: PANE_WIDTH_LIMITS[pane].defaultValue,
        },
      });
      writeLayoutPreferenceSnapshot(snapshot);

      return {
        sidebarCollapsed: snapshot.sidebarCollapsed,
        paneSizes: snapshot.paneSizes,
      };
    }),
}));
