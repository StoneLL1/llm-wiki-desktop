import { lazy, Suspense, type CSSProperties, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";

import {
  PANE_WIDTH_LIMITS,
  SIDEBAR_COLLAPSE_THRESHOLD,
} from "../../hooks/useResizablePane";
import { useModalDialog } from "../../hooks/useModalDialog";
import { useNavigationStore } from "../../stores/navigationStore";
import {
  bindProjectFactsAuthority,
  ensureProjectFacts,
  invalidateProjectFacts,
  projectFactsAuthorityKey,
  projectFactsAuthorityMatches,
  projectFactsKey,
  pruneProjectFacts,
} from "../../stores/projectFactsStore";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import { BottomStatusBar } from "./BottomStatusBar";
import { LeftSidebar } from "./LeftSidebar";
import { ProjectConfirmationController } from "./ProjectConfirmationController";
import { ResizableSplitter } from "./ResizableSplitter";
import { RightContextPanel } from "./RightContextPanel";
import { RightPanelModalContext } from "./RightPanelHeader";
import { Toaster } from "./Toaster";
import { TopBar } from "./TopBar";
import { UpdateController } from "./UpdateController";
import { ViewErrorBoundary } from "./ViewErrorBoundary";
import { WorkspaceController } from "./WorkspaceController";

const TaskLogDrawer = lazy(async () => {
  const module = await import("./TaskLogDrawer");
  return { default: module.TaskLogDrawer };
});

function useNarrowDesktop() {
  const [narrow, setNarrow] = useState(() =>
    typeof window.matchMedia === "function"
      ? window.matchMedia("(max-width: 1180px)").matches
      : false,
  );

  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const media = window.matchMedia("(max-width: 1180px)");
    const update = () => setNarrow(media.matches);
    update();
    if (typeof media.addEventListener === "function") {
      media.addEventListener("change", update);
      return () => media.removeEventListener("change", update);
    }
    if (typeof media.addListener === "function") {
      media.addListener(update);
      return () => media.removeListener(update);
    }
    return undefined;
  }, []);

  return narrow;
}

export function AppShell() {
  const { t } = useTranslation();
  const rightPanelOpen = useNavigationStore((state) => state.rightPanelOpen);
  const setRightPanelOpen = useNavigationStore(
    (state) => state.setRightPanelOpen,
  );
  const workspaceFocus = useNavigationStore((state) => state.workspaceFocus);
  const activeView = useNavigationStore((state) => state.activeView);
  const clearWorkspaceFocus = useNavigationStore(
    (state) => state.clearWorkspaceFocus,
  );
  const sidebarWidth = useNavigationStore((state) => state.paneSizes.sidebar);
  const rightPanelWidth = useNavigationStore((state) => state.paneSizes.rightPanel);
  const setPaneSize = useNavigationStore((state) => state.setPaneSize);
  const resetPaneSize = useNavigationStore((state) => state.resetPaneSize);
  const toggleSettings = useNavigationStore((state) => state.toggleSettings);
  const currentProject = useProjectStore((state) => state.currentProject);
  const taskDrawerOpen = useTaskStore((state) => state.drawerOpen);
  const [taskDrawerLoaded, setTaskDrawerLoaded] = useState(taskDrawerOpen);
  const authority = useProjectStore((state) => state.authority);
  const sidebarCollapsed = sidebarWidth <= SIDEBAR_COLLAPSE_THRESHOLD;
  const shellRef = useRef<HTMLDivElement>(null);
  const narrowDesktop = useNarrowDesktop();
  const showRightPanel = rightPanelOpen && workspaceFocus === null;
  const showRightPanelDialog = showRightPanel && narrowDesktop;
  const rightPanelDialogRef = useModalDialog<HTMLDivElement>({
    open: showRightPanelDialog,
    onClose: () => setRightPanelOpen(false),
    returnFocusSelector: '[aria-controls="right-context-panel"][aria-label]',
  });
  const shellStyle = {
    "--sidebar-w-current": `${sidebarWidth}px`,
    "--rightpanel-w-current": `${rightPanelWidth}px`,
  } as CSSProperties;

  useEffect(() => {
    if (narrowDesktop) setRightPanelOpen(false);
  }, [narrowDesktop, setRightPanelOpen]);

  useEffect(() => {
    if (taskDrawerOpen) setTaskDrawerLoaded(true);
  }, [taskDrawerOpen]);

  useEffect(() => {
    if (!currentProject.projectId || !currentProject.rootPath) {
      pruneProjectFacts(null);
      return;
    }
    const scope = {
      projectId: currentProject.projectId,
      rootPath: currentProject.rootPath,
    };
    const authorityIdentityKey = authority ? projectFactsAuthorityKey(authority) : null;
    bindProjectFactsAuthority(scope, authorityIdentityKey);
    pruneProjectFacts(projectFactsKey(scope));
  }, [
    authority?.canonicalIdentityKey,
    authority?.identityRevision,
    authority?.authorityRevision,
    currentProject.projectId,
    currentProject.rootPath,
  ]);

  useEffect(() => {
    let lastForegroundRefreshAt = Number.NEGATIVE_INFINITY;
    const refreshActiveProjectGit = () => {
      if (document.visibilityState === "hidden") return;
      const now = Date.now();
      if (now - lastForegroundRefreshAt < 250) return;
      const state = useProjectStore.getState();
      const project = state.currentProject;
      if (!project.projectId || !project.rootPath) return;
      if (state.authority?.projectId !== project.projectId) return;
      const scope = { projectId: project.projectId, rootPath: project.rootPath };
      const authorityIdentityKey = projectFactsAuthorityKey(state.authority);
      if (!projectFactsAuthorityMatches(scope, authorityIdentityKey)) return;
      lastForegroundRefreshAt = now;
      invalidateProjectFacts(
        scope,
        ["git"],
        "window_focus",
      );
      void ensureProjectFacts(scope, ["git"]).catch(() => undefined);
    };
    const refreshVisibleProjectGit = () => {
      if (document.visibilityState === "visible") refreshActiveProjectGit();
    };
    window.addEventListener("focus", refreshActiveProjectGit);
    document.addEventListener("visibilitychange", refreshVisibleProjectGit);
    return () => {
      window.removeEventListener("focus", refreshActiveProjectGit);
      document.removeEventListener("visibilitychange", refreshVisibleProjectGit);
    };
  }, []);

  useEffect(() => {
    const closeOnEscape = (event: KeyboardEvent) => {
      if (
        event.key === "Escape" &&
        !event.defaultPrevented &&
        !document.querySelector('[aria-modal="true"]')
      ) {
        if (workspaceFocus !== null) {
          clearWorkspaceFocus();
          return;
        }
        if (rightPanelOpen) {
          setRightPanelOpen(false);
        }
      }
    };
    document.addEventListener("keydown", closeOnEscape);
    return () => document.removeEventListener("keydown", closeOnEscape);
  }, [clearWorkspaceFocus, rightPanelOpen, setRightPanelOpen, workspaceFocus]);

  useEffect(() => {
    const toggleSettingsOnComma = (event: KeyboardEvent) => {
      if (
        (event.ctrlKey || event.metaKey) &&
        event.key === "," &&
        !event.defaultPrevented &&
        !document.querySelector('[aria-modal="true"]')
      ) {
        const target = event.target as HTMLElement | null;
        const tag = target?.tagName?.toLowerCase();
        if (
          tag === "input" ||
          tag === "textarea" ||
          target?.isContentEditable
        ) {
          return;
        }
        event.preventDefault();
        toggleSettings();
      }
    };
    document.addEventListener("keydown", toggleSettingsOnComma);
    return () =>
      document.removeEventListener("keydown", toggleSettingsOnComma);
  }, [toggleSettings]);

  return (
    <div
      ref={shellRef}
      className={[
        "app-shell",
        showRightPanel ? "is-right-open" : "is-right-collapsed",
        workspaceFocus !== null ? "is-workspace-focused" : "",
        sidebarCollapsed ? "is-sidebar-collapsed" : "",
      ]
        .filter(Boolean)
        .join(" ")}
      style={shellStyle}
      inert={showRightPanelDialog ? true : undefined}
    >
      <TopBar />

      <div className="app-shell__workbench">
        <LeftSidebar />
        <ResizableSplitter
          paneId="sidebar"
          label={t("shell.splitter.sidebar")}
          min={PANE_WIDTH_LIMITS.sidebar.min}
          max={PANE_WIDTH_LIMITS.sidebar.max}
          value={sidebarWidth}
          previewTargetRef={shellRef}
          previewCssVariable="--sidebar-w-current"
          onCommit={(value) => setPaneSize("sidebar", value)}
          onReset={() => resetPaneSize("sidebar")}
        />
        <main className="app-shell__main">
          <WorkspaceController />
        </main>
        {showRightPanel && !narrowDesktop ? (
          <ResizableSplitter
            paneId="rightPanel"
            label={t("shell.splitter.rightPanel")}
            min={PANE_WIDTH_LIMITS.rightPanel.min}
            max={PANE_WIDTH_LIMITS.rightPanel.max}
            value={rightPanelWidth}
            direction={-1}
            previewTargetRef={shellRef}
            previewCssVariable="--rightpanel-w-current"
            onCommit={(value) => setPaneSize("rightPanel", value)}
            onReset={() => resetPaneSize("rightPanel")}
          />
        ) : null}
        {showRightPanel && !narrowDesktop ? <RightContextPanel /> : null}
      </div>

      <BottomStatusBar />
      <Toaster />
      <ProjectConfirmationController />
      {taskDrawerLoaded ? (
        <ViewErrorBoundary>
          <Suspense fallback={null}>
            <TaskLogDrawer />
          </Suspense>
        </ViewErrorBoundary>
      ) : null}
      <UpdateController />
      {showRightPanelDialog
        ? createPortal(
            <div
              aria-labelledby="right-context-panel-title"
              aria-modal="true"
              className="right-panel-overlay"
              onClick={(event) => {
                if (event.target === event.currentTarget) setRightPanelOpen(false);
              }}
              ref={rightPanelDialogRef}
              role="dialog"
              tabIndex={-1}
            >
              <div
                className={`right-panel-overlay__surface${activeView === "workflows" ? " is-workflows" : ""}`}
              >
                <RightPanelModalContext.Provider value>
                  <RightContextPanel />
                </RightPanelModalContext.Provider>
              </div>
            </div>,
            document.body,
          )
        : null}
    </div>
  );
}
