import { beforeEach, describe, expect, it } from "vitest";
import { useNavigationStore } from "./navigationStore";

describe("navigationStore layout preferences", () => {
  beforeEach(() => {
    window.localStorage.clear();
    useNavigationStore.setState({
      activeView: "dashboard",
      rightPanelOpen: true,
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
