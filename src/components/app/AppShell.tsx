import { type CSSProperties, useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";

import {
  PANE_WIDTH_LIMITS,
  SIDEBAR_COLLAPSE_THRESHOLD,
} from "../../hooks/useResizablePane";
import { useModalDialog } from "../../hooks/useModalDialog";
import { useNavigationStore } from "../../stores/navigationStore";
import { BottomStatusBar } from "./BottomStatusBar";
import { LeftSidebar } from "./LeftSidebar";
import { ProjectConfirmationController } from "./ProjectConfirmationController";
import { ResizableSplitter } from "./ResizableSplitter";
import { RightContextPanel } from "./RightContextPanel";
import { RightPanelModalContext } from "./RightPanelHeader";
import { TaskLogDrawer } from "./TaskLogDrawer";
import { Toaster } from "./Toaster";
import { TopBar } from "./TopBar";
import { WorkspaceController } from "./WorkspaceController";

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
  const paneSizes = useNavigationStore((state) => state.paneSizes);
  const setPaneSize = useNavigationStore((state) => state.setPaneSize);
  const resetPaneSize = useNavigationStore((state) => state.resetPaneSize);
  const toggleSettings = useNavigationStore((state) => state.toggleSettings);
  const sidebarCollapsed =
    paneSizes.sidebar <= SIDEBAR_COLLAPSE_THRESHOLD;
  const narrowDesktop = useNarrowDesktop();
  const showRightPanel = rightPanelOpen && workspaceFocus === null;
  const showRightPanelDialog = showRightPanel && narrowDesktop;
  const rightPanelDialogRef = useModalDialog<HTMLDivElement>({
    open: showRightPanelDialog,
    onClose: () => setRightPanelOpen(false),
    returnFocusSelector: '[aria-controls="right-context-panel"][aria-label]',
  });
  const shellStyle = {
    "--sidebar-w-current": `${paneSizes.sidebar}px`,
    "--rightpanel-w-current": `${paneSizes.rightPanel}px`,
  } as CSSProperties;

  useEffect(() => {
    if (narrowDesktop) setRightPanelOpen(false);
  }, [narrowDesktop, setRightPanelOpen]);

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
          value={paneSizes.sidebar}
          onChange={(value) => setPaneSize("sidebar", value)}
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
            value={paneSizes.rightPanel}
            direction={-1}
            onChange={(value) => setPaneSize("rightPanel", value)}
            onReset={() => resetPaneSize("rightPanel")}
          />
        ) : null}
        {showRightPanel && !narrowDesktop ? <RightContextPanel /> : null}
      </div>

      <BottomStatusBar />
      <Toaster />
      <ProjectConfirmationController />
      <TaskLogDrawer />
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
