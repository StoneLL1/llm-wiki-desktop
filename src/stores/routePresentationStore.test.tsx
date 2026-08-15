import { act, render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import {
  activateRoutePresentationProject,
  readChatRoutePreference,
  readGraphCameraSnapshot,
  readRouteScrollPosition,
  resetRoutePresentation,
  saveChatRoutePreference,
  saveGraphCameraSnapshot,
  saveRouteScrollPosition,
  useRouteScrollCallbackRestoration,
  useRouteScrollRestoration,
} from "../hooks/useRouteScrollRestoration";
import { WikiEditor } from "../features/wiki/WikiEditor";
import { projectResourceKey } from "../lib/projectResourceFreshness";
import { resetProjectScopedStores } from "./resetProjectScope";
import { defaultProject, useProjectStore } from "./projectStore";

function ScrollSurface({ projectId = "project-a", routeKey = "wiki:content" }) {
  const ref = useRouteScrollRestoration(projectId, "/vault", routeKey);
  return <div ref={ref} data-testid="scroll-surface" />;
}

function RetainedWikiEditor({ mounted }: { mounted: boolean }) {
  const scrollRef = useRouteScrollCallbackRestoration<HTMLDivElement>(
    "project-a",
    "/vault",
    "wiki:page:wiki/concept.md:edit",
  );
  return mounted ? (
    <WikiEditor
      draft="# Draft"
      saveState="idle"
      onDraftChange={vi.fn()}
      onSave={vi.fn()}
      onCancel={vi.fn()}
      onReload={vi.fn()}
      scrollRef={scrollRef}
    />
  ) : null;
}

describe("routePresentationStore", () => {
  it("restores numeric scroll state after a route remount", () => {
    resetRoutePresentation();
    const first = render(<ScrollSurface />);
    const surface = first.getByTestId("scroll-surface");
    act(() => {
      surface.scrollTop = 164;
    });
    first.unmount();

    const second = render(<ScrollSurface />);
    expect(second.getByTestId("scroll-surface").scrollTop).toBe(164);
  });

  it("clears snapshots on project activation and rejects a late old-project cleanup", () => {
    const oldKey = projectResourceKey("project-a", "/vault-a");
    const newKey = projectResourceKey("project-b", "/vault-b");
    resetRoutePresentation();
    activateRoutePresentationProject(oldKey);
    saveRouteScrollPosition(oldKey, "exports:list", 88);

    activateRoutePresentationProject(newKey);
    saveRouteScrollPosition(oldKey, "exports:list", 144);

    expect(readRouteScrollPosition(newKey, "exports:list")).toBeNull();
    expect(readRouteScrollPosition(oldKey, "exports:list")).toBeNull();
  });

  it("ignores invalid scroll values", () => {
    const key = projectResourceKey("project-a", "/vault");
    resetRoutePresentation();
    activateRoutePresentationProject(key);
    saveRouteScrollPosition(key, "lint:list", Number.NaN);
    saveRouteScrollPosition(key, "lint:list", -1);
    expect(readRouteScrollPosition(key, "lint:list")).toBeNull();
  });

  it("keeps the Chat route preference for ordinary route remounts", () => {
    const key = projectResourceKey("project-a", "/vault");
    resetRoutePresentation();
    activateRoutePresentationProject(key);
    saveChatRoutePreference("byok");
    activateRoutePresentationProject(key);
    expect(readChatRoutePreference()).toBe("byok");
  });

  it("restores the real retained Wiki editor scroller after a route remount", () => {
    resetRoutePresentation();
    const view = render(<RetainedWikiEditor mounted />);
    const editor = view.getByTestId("wiki-editor-scroll");
    act(() => {
      editor.scrollTop = 236;
    });

    view.rerender(<RetainedWikiEditor mounted={false} />);
    view.rerender(<RetainedWikiEditor mounted />);

    expect(view.getByTestId("wiki-editor-scroll").scrollTop).toBe(236);
  });

  it("project reset clears scroll, graph camera, and Chat route preference together", () => {
    const key = projectResourceKey("project-a", "/vault");
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "project-a",
        rootPath: "/vault",
      },
    });
    resetRoutePresentation();
    activateRoutePresentationProject(key);
    saveRouteScrollPosition(key, "wiki:tree", 91);
    saveGraphCameraSnapshot({
      contentHash: "hash-a",
      x: 0.2,
      y: 0.3,
      ratio: 1.4,
      angle: 0,
    });
    saveChatRoutePreference("byok");

    resetProjectScopedStores();
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "project-b",
        rootPath: "/other-vault",
      },
    });
    activateRoutePresentationProject(projectResourceKey("project-b", "/other-vault"));

    expect(readRouteScrollPosition(key, "wiki:tree")).toBeNull();
    expect(readGraphCameraSnapshot()).toBeNull();
    expect(readChatRoutePreference()).toBe("auto");
  });
});
