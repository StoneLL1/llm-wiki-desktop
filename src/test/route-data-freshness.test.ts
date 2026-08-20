import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createElement, useEffect } from "react";
import { render, waitFor } from "@testing-library/react";
import "../i18n";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import { useWikiStore } from "../features/wiki/wikiStore";
import { useChatStore } from "../stores/chatStore";
import { useExportStore } from "../stores/exportStore";
import { useGraphStore } from "../stores/graphStore";
import { useLintStore } from "../stores/lintStore";
import { useSettingsStore } from "../stores/settingsStore";
import { defaultProject, useProjectStore } from "../stores/projectStore";
import { WorkspaceRouter } from "../components/app/WorkspaceRouter";
import {
  observeProjectResources,
  type ProjectResourceScope,
} from "../stores/projectScope";

const PROJECT = { projectId: "project-a", rootPath: "D:/知识库" };
const CURRENT_PROJECT = { ...defaultProject, ...PROJECT, name: "Freshness" };
type ProbeRoute = "wiki" | "exports" | "chat" | "graph" | "lint" | "settings";

function RouteProbe({ route, scope }: { route: ProbeRoute; scope: ProjectResourceScope }) {
  useEffect(() => {
    const request = { projectId: scope.projectId, projectRootPath: scope.rootPath };
    switch (route) {
      case "wiki":
        void useWikiStore.getState().ensureScanned(scope.projectId, scope.rootPath);
        return observeProjectResources(scope, ["wiki"]);
      case "exports":
        void useExportStore.getState().ensureExports(scope.projectId, scope.rootPath);
        return observeProjectResources(scope, ["exports"]);
      case "chat":
        void useChatStore.getState().ensureSessions(scope.projectId, scope.rootPath);
        return observeProjectResources(scope, ["chat-sessions"]);
      case "graph":
        void useGraphStore.getState().ensureGraph(scope.projectId, scope.rootPath);
        return observeProjectResources(scope, ["graph"]);
      case "lint":
        void useLintStore.getState().ensureIgnores(request);
        void useLintStore.getState().ensureHistory(request);
        return observeProjectResources(scope, ["lint-ignores", "lint-history"]);
      case "settings":
        void useSettingsStore.getState().ensureChatConvenienceAuthorization(
          scope.projectId,
          scope.rootPath,
        );
        return observeProjectResources(scope, ["settings-chat-authorization"]);
    }
  }, [route, scope]);
  return null;
}

describe("freshness-aware route data reuse", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
    useWikiStore.getState().reset();
    useChatStore.getState().reset();
    useExportStore.getState().reset();
    useGraphStore.getState().reset();
    useLintStore.getState().reset();
    useSettingsStore.getState().reset();
    useProjectStore.setState({ currentProject: CURRENT_PROJECT });
    invokeMock.mockImplementation(async (command: string) => {
      switch (command) {
        case "scan_wiki":
          return {
            root: { name: "wiki", kind: "folder", path: "wiki", starred: false, bookmarked: false, fileCount: 0, children: [] },
            pages: [],
            totalPages: 0,
          };
        case "list_exports":
        case "list_chat_sessions":
          return [];
        case "get_graph":
          return {
            data: { nodes: [], edges: [], contentHash: "graph-a", builtAt: "2026-08-16T00:00:00Z" },
            cached: true,
            layoutStale: false,
          };
        case "list_lint_ignores":
          return { ignored: [] };
        case "list_lint_history":
          return { version: 1, entries: [] };
        case "get_chat_convenience_authorization":
          return {
            enabled: false,
            confirmedAt: "",
            projectId: PROJECT.projectId,
            rootPathFingerprint: "",
          };
        default:
          throw new Error(`Unexpected command: ${command}`);
      }
    });
  });

  afterEach(() => {
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  });

  it("serves twenty rapid route remounts with one IPC read per resource", async () => {
    const routes: ProbeRoute[] = ["wiki", "exports", "chat", "graph", "lint", "settings"];
    const mounted = render(createElement(RouteProbe, { route: "wiki", scope: PROJECT }));
    for (let remount = 0; remount < 20; remount += 1) {
      for (const route of routes) {
        mounted.rerender(createElement(RouteProbe, {
          key: `${remount}:${route}`,
          route,
          scope: PROJECT,
        }));
      }
    }

    await waitFor(() => {
      expect(new Set(invokeMock.mock.calls.map(([command]) => command))).toEqual(new Set([
        "scan_wiki",
        "list_exports",
        "list_chat_sessions",
        "get_graph",
        "list_lint_ignores",
        "list_lint_history",
        "get_chat_convenience_authorization",
      ]));
    });
    for (const command of [
      "scan_wiki",
      "list_exports",
      "list_chat_sessions",
      "get_graph",
      "list_lint_ignores",
      "list_lint_history",
      "get_chat_convenience_authorization",
    ]) {
      expect(invokeMock.mock.calls.filter(([called]) => called === command), command).toHaveLength(1);
    }
    mounted.unmount();
  });

  it("keeps the real Wiki and Exports routes warm across rapid router remounts", async () => {
    const props = {
      capabilities: { agents: [], providers: [], refreshing: false, refresh: vi.fn() },
      importWorkflow: {} as never,
      workflowsController: {} as never,
      onOpenTask: vi.fn(),
    };
    const mounted = render(createElement(WorkspaceRouter, { activeView: "wiki", ...props }));
    await waitFor(
      () => expect(invokeMock).toHaveBeenCalledWith("scan_wiki", expect.anything()),
      // The full release gate intentionally runs the Rust and frontend lanes
      // together. Give the real lazy route enough time to load under that
      // contention while keeping the test's existing 20-second hard ceiling.
      { timeout: 15_000 },
    );

    for (let remount = 0; remount < 20; remount += 1) {
      mounted.rerender(createElement(WorkspaceRouter, { activeView: "exports", ...props }));
      mounted.rerender(createElement(WorkspaceRouter, { activeView: "wiki", ...props }));
    }

    await waitFor(() => expect(invokeMock.mock.calls.filter(([command]) => command === "list_exports")).toHaveLength(1));
    expect(invokeMock.mock.calls.filter(([command]) => command === "scan_wiki")).toHaveLength(1);
    mounted.unmount();
  }, 20_000);
});
