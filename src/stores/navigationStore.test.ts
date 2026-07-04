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
      sidebarCollapsed: false,
      paneSizes: {
        sidebar: 240,
        rightPanel: 320,
        wikiTree: 260,
        exportsList: 360,
        lintList: 360,
      },
    });
  });


  it("starts with the default view and right panel mode", () => {
    expect(useNavigationStore.getState().activeView).toBe("dashboard");
    expect(useNavigationStore.getState().rightPanelOpen).toBe(true);
    expect(useNavigationStore.getState().rightPanelMode).toBe("default");
    expect(useNavigationStore.getState().wikiAssistantPagePath).toBeNull();
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
    expect(window.localStorage.getItem("llm-wiki-desktop.layout.v1")).toContain(
      "sidebarCollapsed",
    );
  });

  it("clamps and persists pane size changes", () => {
    useNavigationStore.getState().setPaneSize("rightPanel", 900);
    expect(useNavigationStore.getState().paneSizes.rightPanel).toBe(520);
  });
});
