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

export interface NavigationState {
  activeView: AppView;
  rightPanelOpen: boolean;
  sidebarCollapsed: boolean;
  paneSizes: Record<ResizablePaneId, number>;
  setActiveView: (view: AppView) => void;
  setRightPanelOpen: (open: boolean) => void;
  setSidebarCollapsed: (collapsed: boolean) => void;
  toggleSidebarCollapsed: () => void;
  setPaneSize: (pane: ResizablePaneId, width: number) => void;
  resetPaneSize: (pane: ResizablePaneId) => void;
}

const initialLayoutPreferences = readLayoutPreferenceSnapshot();

export const useNavigationStore = create<NavigationState>((set) => ({
  activeView: "dashboard",
  rightPanelOpen: true,
  sidebarCollapsed: initialLayoutPreferences.sidebarCollapsed,
  paneSizes: initialLayoutPreferences.paneSizes,
  setActiveView: (activeView) => set({ activeView }),
  setRightPanelOpen: (rightPanelOpen) => set({ rightPanelOpen }),
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
