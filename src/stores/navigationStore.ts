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
  setActiveView: (view: AppView) => void;
}

export const useNavigationStore = create<NavigationState>((set) => ({
  activeView: "dashboard",
  setActiveView: (activeView) => set({ activeView }),
}));

