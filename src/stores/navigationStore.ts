import { create } from "zustand";
import type {
  WorkflowKind,
  WorkflowRouteSelection,
  WorkflowScope,
  WorkflowScopePreset,
} from "../types/workflow";
import {
  type ResizablePaneId,
  PANE_WIDTH_LIMITS,
  SIDEBAR_COLLAPSE_THRESHOLD,
  readLayoutPreferenceSnapshot,
  sanitizeLayoutPreferences,
  writeLayoutPreferenceSnapshot,
} from "../hooks/useResizablePane";

export type AppView =
  | "dashboard"
  | "wiki"
  | "chat"
  | "graph"
  | "workflows"
  | "import"
  | "lint"
  | "exports";

export type RightPanelMode = "default" | "wikiAssistant";

export type WorkspaceFocus = "exportPreview";

export type SettingsSectionKey =
  | "general"
  | "appearance"
  | "language"
  | "ai"
  | "security"
  | "compatibility"
  | "background"
  | "updates";

export type WorkflowLaunchOrigin =
  | "workflows"
  | "dashboard"
  | "import"
  | "wiki"
  | "lint"
  | "exports";

export interface WorkflowLaunchIntent {
  projectId: string;
  projectRootPath: string;
  kind: WorkflowKind;
  origin: WorkflowLaunchOrigin;
  scopePreset: WorkflowScopePreset | null;
}

export interface WorkflowSettingsReturnIntent {
  projectId: string;
  projectRootPath: string;
  kind: WorkflowKind;
  scope: WorkflowScope;
  routeSelection: WorkflowRouteSelection | null;
  source: "prerequisite" | "adjust";
  expectedSurface: "preparation" | "detail";
  expectedCanonicalIdentityKey: string;
  expectedIdentityRevision: string;
  expectedPreparationId: string | null;
  expectedPreparationRevision: string | null;
  expectedTaskId: string | null;
}

export interface ImportSuccessNotice {
  projectId: string;
  name: string;
}

export interface PendingImportPath {
  projectId: string;
  path: string;
}

export interface NavigationState {
  activeView: AppView;
  rightPanelOpen: boolean;
  rightPanelMode: RightPanelMode;
  wikiAssistantPagePath: string | null;
  workspaceFocus: WorkspaceFocus | null;
  rightPanelOpenBeforeFocus: boolean | null;
  sidebarCollapsed: boolean;
  paneSizes: Record<ResizablePaneId, number>;
  settingsOpen: boolean;
  settingsSection: SettingsSectionKey;
  workflowSettingsReturnIntent: WorkflowSettingsReturnIntent | null;
  workflowLaunchIntent: WorkflowLaunchIntent | null;
  importSuccessNotice: ImportSuccessNotice | null;
  pendingImportPath: PendingImportPath | null;
  setActiveView: (view: AppView) => void;
  setRightPanelOpen: (open: boolean) => void;
  openWikiAssistant: (path: string) => void;
  setWikiAssistantPagePath: (path: string | null) => void;
  closeWikiAssistant: () => void;
  focusWorkspace: (focus: WorkspaceFocus) => void;
  clearWorkspaceFocus: () => void;
  setSidebarCollapsed: (collapsed: boolean) => void;
  toggleSidebarCollapsed: () => void;
  setPaneSize: (pane: ResizablePaneId, width: number) => void;
  resetPaneSize: (pane: ResizablePaneId) => void;
  openSettings: (
    section?: SettingsSectionKey,
    workflowReturnIntent?: WorkflowSettingsReturnIntent | null,
  ) => void;
  closeSettings: () => void;
  toggleSettings: () => void;
  clearWorkflowSettingsReturnIntent: () => void;
  requestWorkflowLaunch: (intent: WorkflowLaunchIntent) => void;
  clearWorkflowLaunchIntent: () => void;
  setImportSuccessNotice: (notice: ImportSuccessNotice) => void;
  clearImportSuccessNotice: () => void;
  setPendingImportPath: (path: PendingImportPath) => void;
  clearPendingImportPath: () => void;
}

const initialLayoutPreferences = readLayoutPreferenceSnapshot();

export const useNavigationStore = create<NavigationState>((set) => ({
  activeView: "dashboard",
  rightPanelOpen: true,
  rightPanelMode: "default",
  wikiAssistantPagePath: null,
  workspaceFocus: null,
  rightPanelOpenBeforeFocus: null,
  sidebarCollapsed: initialLayoutPreferences.sidebarCollapsed,
  paneSizes: initialLayoutPreferences.paneSizes,
  settingsOpen: false,
  settingsSection: "general",
  workflowSettingsReturnIntent: null,
  workflowLaunchIntent: null,
  importSuccessNotice: null,
  pendingImportPath: null,
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
  focusWorkspace: (workspaceFocus) =>
    set((state) => {
      if (state.workspaceFocus === workspaceFocus) {
        return state;
      }
      return {
        workspaceFocus,
        rightPanelOpenBeforeFocus: state.rightPanelOpen,
        rightPanelOpen: false,
      };
    }),
  clearWorkspaceFocus: () =>
    set((state) => ({
      workspaceFocus: null,
      rightPanelOpenBeforeFocus: null,
      rightPanelOpen: state.rightPanelOpenBeforeFocus ?? state.rightPanelOpen,
    })),
  setSidebarCollapsed: (sidebarCollapsed) =>
    set((state) => {
      const nextSidebarWidth = sidebarCollapsed
        ? PANE_WIDTH_LIMITS.sidebar.min
        : state.paneSizes.sidebar <= SIDEBAR_COLLAPSE_THRESHOLD
          ? PANE_WIDTH_LIMITS.sidebar.defaultValue
          : state.paneSizes.sidebar;
      const snapshot = sanitizeLayoutPreferences({
        sidebarCollapsed,
        paneSizes: {
          ...state.paneSizes,
          sidebar: nextSidebarWidth,
        },
      });
      writeLayoutPreferenceSnapshot(snapshot);

      return {
        sidebarCollapsed: snapshot.sidebarCollapsed,
        paneSizes: snapshot.paneSizes,
      };
    }),
  toggleSidebarCollapsed: () =>
    set((state) => {
      const sidebarCollapsed = state.paneSizes.sidebar > SIDEBAR_COLLAPSE_THRESHOLD;
      const nextSidebarWidth = sidebarCollapsed
        ? PANE_WIDTH_LIMITS.sidebar.min
        : PANE_WIDTH_LIMITS.sidebar.defaultValue;
      const snapshot = sanitizeLayoutPreferences({
        sidebarCollapsed,
        paneSizes: {
          ...state.paneSizes,
          sidebar: nextSidebarWidth,
        },
      });
      writeLayoutPreferenceSnapshot(snapshot);

      return {
        sidebarCollapsed: snapshot.sidebarCollapsed,
        paneSizes: snapshot.paneSizes,
      };
    }),
  setPaneSize: (pane, width) =>
    set((state) => {
      const sanitized = sanitizeLayoutPreferences({
        sidebarCollapsed: pane === "sidebar" ? false : state.sidebarCollapsed,
        paneSizes: {
          ...state.paneSizes,
          [pane]: width,
        },
      });
      const snapshot =
        pane === "sidebar"
          ? {
              ...sanitized,
              sidebarCollapsed: sanitized.paneSizes.sidebar <= SIDEBAR_COLLAPSE_THRESHOLD,
            }
          : sanitized;
      writeLayoutPreferenceSnapshot(snapshot);

      return {
        sidebarCollapsed: snapshot.sidebarCollapsed,
        paneSizes: snapshot.paneSizes,
      };
    }),
  resetPaneSize: (pane) =>
    set((state) => {
      const sanitized = sanitizeLayoutPreferences({
        sidebarCollapsed: pane === "sidebar" ? false : state.sidebarCollapsed,
        paneSizes: {
          ...state.paneSizes,
          [pane]: PANE_WIDTH_LIMITS[pane].defaultValue,
        },
      });
      const snapshot =
        pane === "sidebar"
          ? {
              ...sanitized,
              sidebarCollapsed: sanitized.paneSizes.sidebar <= SIDEBAR_COLLAPSE_THRESHOLD,
            }
          : sanitized;
      writeLayoutPreferenceSnapshot(snapshot);

      return {
        sidebarCollapsed: snapshot.sidebarCollapsed,
        paneSizes: snapshot.paneSizes,
      };
    }),
  openSettings: (settingsSection = "general", workflowSettingsReturnIntent = null) =>
    set({ settingsOpen: true, settingsSection, workflowSettingsReturnIntent }),
  closeSettings: () => set({ settingsOpen: false }),
  toggleSettings: () =>
    set((state) => ({
      settingsOpen: !state.settingsOpen,
      settingsSection: state.settingsOpen ? state.settingsSection : "general",
      workflowSettingsReturnIntent: state.settingsOpen
        ? state.workflowSettingsReturnIntent
        : null,
    })),
  clearWorkflowSettingsReturnIntent: () => set({ workflowSettingsReturnIntent: null }),
  requestWorkflowLaunch: (workflowLaunchIntent) =>
    set({ workflowLaunchIntent, activeView: "workflows" }),
  clearWorkflowLaunchIntent: () => set({ workflowLaunchIntent: null }),
  setImportSuccessNotice: (importSuccessNotice) => set({ importSuccessNotice }),
  clearImportSuccessNotice: () => set({ importSuccessNotice: null }),
  setPendingImportPath: (pendingImportPath) => set({ pendingImportPath }),
  clearPendingImportPath: () => set({ pendingImportPath: null }),
}));
