import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import "../../i18n";
import type { GraphData, GraphStatus } from "../../types/graph";
import { GraphInspector } from "./GraphInspector";

describe("GraphInspector", () => {
  it("omits search-hidden neighbors from the neighbor list", () => {
    render(
      <GraphInspector
        node={data.nodes[0]}
        data={data}
        typeFilter={new Set()}
        degreeThreshold={0}
        search="visible"
        focusedNodeId={null}
        layoutStale={false}
        cached
        status={"ready" as GraphStatus}
        onOpenPage={vi.fn()}
        onFocusNode={vi.fn()}
        onOpenNeighbor={vi.fn()}
        onToggleType={vi.fn()}
        onDegreeThresholdChange={vi.fn()}
        onExportPng={vi.fn()}
        onRecomputeLayout={vi.fn()}
      />,
    );

    expect(screen.getByRole("button", { name: /Visible Neighbor/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Hidden Neighbor/ })).not.toBeInTheDocument();
  });
});

const data: GraphData = {
  nodes: [
    { id: "root", path: "wiki/root.md", label: "Root", type: "concept", tags: [], starred: false, degree: 2 },
    { id: "visible", path: "wiki/visible.md", label: "Visible Neighbor", type: "entity", tags: ["visible"], starred: false, degree: 1 },
    { id: "hidden", path: "wiki/hidden.md", label: "Hidden Neighbor", type: "source", tags: [], starred: false, degree: 1 },
  ],
  edges: [
    { source: "root", target: "visible", relation: "related", weight: 1 },
    { source: "root", target: "hidden", relation: "related", weight: 1 },
  ],
  contentHash: "hash",
  builtAt: "2026-07-04T00:00:00Z",
  layout: null,
};
