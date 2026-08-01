import { beforeEach, describe, expect, it } from "vitest";
import { useNavigationStore } from "./navigationStore";

describe("navigationStore layout preferences", () => {
  beforeEach(() => {
    window.localStorage.clear();
    useNavigationStore.setState({
      activeView: "dashboard",
      rightPanelOpen: true,
      rightPanelMode: "default",
      wikiAssistantPagePath: null,
      workspaceFocus: null,
      rightPanelOpenBeforeFocus: null,
      sidebarCollapsed: false,
      paneSizes: {
        sidebar: 240,
        rightPanel: 320,
        wikiTree: 260,
        exportsList: 360,
        lintDetails: 320,
      },
      workflowLaunchIntent: null,
    });
  });

  it("routes a structured workflow intent to preparation without starting work", () => {
    useNavigationStore.getState().requestWorkflowLaunch({
      projectId: "project-a",
      projectRootPath: "D:/wiki-a",
      kind: "health_check",
      origin: "lint",
      scopePreset: { kind: "health_check", mode: "complete" },
    });

    expect(useNavigationStore.getState().activeView).toBe("workflows");
    expect(useNavigationStore.getState().workflowLaunchIntent).toEqual(
      expect.objectContaining({ origin: "lint", kind: "health_check" }),
    );
  });

  it("starts with the default view and right panel mode", () => {
    expect(useNavigationStore.getState().activeView).toBe("dashboard");
    expect(useNavigationStore.getState().rightPanelOpen).toBe(true);
    expect(useNavigationStore.getState().rightPanelMode).toBe("default");
    expect(useNavigationStore.getState().wikiAssistantPagePath).toBeNull();
    expect(useNavigationStore.getState().workspaceFocus).toBeNull();
  });

  it("opens the wiki assistant for a page", () => {
    useNavigationStore.getState().openWikiAssistant("wiki/concepts/react-pattern.md");

    expect(useNavigationStore.getState().activeView).toBe("wiki");
    expect(useNavigationStore.getState().rightPanelOpen).toBe(true);
    expect(useNavigationStore.getState().rightPanelMode).toBe("wikiAssistant");
    expect(useNavigationStore.getState().wikiAssistantPagePath).toBe(
      "wiki/concepts/react-pattern.md",
    );
  });

  it("resets wiki assistant mode when leaving wiki without closing the panel", () => {
    useNavigationStore.getState().openWikiAssistant("wiki/concepts/react-pattern.md");
    useNavigationStore.getState().setActiveView("chat");

    expect(useNavigationStore.getState().activeView).toBe("chat");
    expect(useNavigationStore.getState().rightPanelOpen).toBe(true);
    expect(useNavigationStore.getState().rightPanelMode).toBe("default");
    expect(useNavigationStore.getState().wikiAssistantPagePath).toBeNull();
  });

  it("updates the wiki assistant page path without closing the panel", () => {
    useNavigationStore.getState().openWikiAssistant("wiki/concepts/react-pattern.md");
    useNavigationStore.getState().setRightPanelOpen(false);
    useNavigationStore.getState().setWikiAssistantPagePath("wiki/concepts/agent-memory.md");

    expect(useNavigationStore.getState().rightPanelOpen).toBe(false);
    expect(useNavigationStore.getState().rightPanelMode).toBe("wikiAssistant");
    expect(useNavigationStore.getState().wikiAssistantPagePath).toBe(
      "wiki/concepts/agent-memory.md",
    );
  });

  it("persists sidebar collapse without touching active view", () => {
    useNavigationStore.getState().setActiveView("graph");
    useNavigationStore.getState().toggleSidebarCollapsed();

    expect(useNavigationStore.getState().activeView).toBe("graph");
    expect(useNavigationStore.getState().sidebarCollapsed).toBe(true);
    expect(useNavigationStore.getState().paneSizes.sidebar).toBe(56);
    expect(window.localStorage.getItem("llm-wiki-desktop.layout.v1")).toContain(
      "sidebarCollapsed",
    );
  });

  it("expands the sidebar to its default width through the compatibility API", () => {
    useNavigationStore.getState().setSidebarCollapsed(true);
    useNavigationStore.getState().setSidebarCollapsed(false);

    expect(useNavigationStore.getState().sidebarCollapsed).toBe(false);
    expect(useNavigationStore.getState().paneSizes.sidebar).toBe(240);
  });

  it("allows dragging the sidebar back out after it collapsed to the icon rail", () => {
    useNavigationStore.getState().setPaneSize("sidebar", 56);
    useNavigationStore.getState().setPaneSize("sidebar", 160);

    expect(useNavigationStore.getState().sidebarCollapsed).toBe(false);
    expect(useNavigationStore.getState().paneSizes.sidebar).toBe(160);
  });

  it("clamps and persists pane size changes", () => {
    useNavigationStore.getState().setPaneSize("rightPanel", 900);
    expect(useNavigationStore.getState().paneSizes.rightPanel).toBe(520);
  });

  it("focuses the export preview workspace and restores the previous right panel state", () => {
    useNavigationStore.getState().setRightPanelOpen(true);

    useNavigationStore.getState().focusWorkspace("exportPreview");

    expect(useNavigationStore.getState().workspaceFocus).toBe("exportPreview");
    expect(useNavigationStore.getState().rightPanelOpen).toBe(false);

    useNavigationStore.getState().clearWorkspaceFocus();

    expect(useNavigationStore.getState().workspaceFocus).toBeNull();
    expect(useNavigationStore.getState().rightPanelOpen).toBe(true);
  });

  it("keeps the right panel closed after clearing focus when it was closed before focus", () => {
    useNavigationStore.getState().setRightPanelOpen(false);

    useNavigationStore.getState().focusWorkspace("exportPreview");
    useNavigationStore.getState().clearWorkspaceFocus();

    expect(useNavigationStore.getState().workspaceFocus).toBeNull();
    expect(useNavigationStore.getState().rightPanelOpen).toBe(false);
  });
});
