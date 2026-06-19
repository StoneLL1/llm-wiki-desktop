import { create } from "zustand";

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

interface NavigationState {
  activeView: AppView;
  rightPanelOpen: boolean;
  setActiveView: (view: AppView) => void;
  setRightPanelOpen: (open: boolean) => void;
}

export const useNavigationStore = create<NavigationState>((set) => ({
  activeView: "dashboard",
  rightPanelOpen: true,
  setActiveView: (activeView) => set({ activeView }),
  setRightPanelOpen: (rightPanelOpen) => set({ rightPanelOpen }),
}));
