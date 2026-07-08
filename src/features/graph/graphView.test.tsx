import { render, screen } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import "../../i18n";
import { useGraphStore } from "../../stores/graphStore";
import { GraphView } from "./GraphView";

describe("GraphView", () => {
  const graphData = {
    nodes: [
      { id: "a", path: "wiki/a.md", label: "Alpha", type: "concept" as const, tags: [], starred: false, degree: 1 },
    ],
    edges: [],
    contentHash: "hash-1",
    builtAt: "2026-07-04T00:00:00Z",
    layout: { positions: { a: [0, 0] as [number, number] }, communities: { a: 0 } },
  };

  it("mounts and renders the empty state when no graph data is loaded", () => {
    useGraphStore.getState().reset();
    // Neutralize the auto-load mount effect so the idle + no-data branch is
    // observable. The store's real load path is exercised in graphStore.test.ts.
    useGraphStore.setState({ load: async () => {} });
    render(<GraphView />);
    // No data + idle status → empty-state copy. Asserting the localized empty
    // text also confirms the component tree (and the sigma import) mounts
    // without throwing in a non-Tauri/jsdom environment.
    expect(screen.getByText(/No graph yet/i)).toBeInTheDocument();
  });

  it("renders ready-empty graph state without constructing sigma", () => {
    useGraphStore.getState().reset();
    useGraphStore.setState({
      status: "ready-empty",
      data: { nodes: [], edges: [], contentHash: "empty", builtAt: "2026-07-04T00:00:00Z", layout: null },
      load: async () => {},
    });

    render(<GraphView />);

    expect(screen.getByText(/No pages yet/i)).toBeInTheDocument();
    expect(screen.queryByText(/canvas unavailable/i)).not.toBeInTheDocument();
  });

  it("keeps canvas surface and shows rebuilding banner when data exists", () => {
    useGraphStore.getState().reset();
    useGraphStore.setState({
      status: "rebuilding",
      data: graphData,
      load: async () => {},
    });

    render(<GraphView />);

    expect(screen.getAllByText(/Rebuilding graph/i).length).toBeGreaterThan(0);
    expect(document.querySelector(".graph-canvas")).toBeInTheDocument();
  });

  it("shows rebuild progress in an overlay while keeping the graph surface mounted", () => {
    useGraphStore.getState().reset();
    useGraphStore.setState({
      status: "rebuilding",
      data: graphData,
      buildUi: {
        phase: "rebuilding",
        taskId: "task-graph-1",
        progress: 0.5,
        label: "Building graph",
        error: null,
      },
      load: async () => {},
    } as Partial<ReturnType<typeof useGraphStore.getState>>);

    render(<GraphView />);

    expect(screen.getByRole("status", { name: /rebuilding graph/i })).toBeInTheDocument();
    expect(screen.getByTestId("graph-canvas-surface")).toBeInTheDocument();
    expect(screen.getByText("50%")).toBeInTheDocument();
  });

  it("keeps graph edges visible while the user pans or zooms", () => {
    const source = readFileSync(join(process.cwd(), "src", "features", "graph", "GraphView.tsx"), "utf8");

    expect(source).toMatch(/hideEdgesOnMove:\s*false/);
  });
});
