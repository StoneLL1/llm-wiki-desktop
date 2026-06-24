import { describe, expect, it } from "vitest";

import type { GraphData } from "../../types/graph";
import { neighborCount, neighborsOf } from "./graphNeighbors";

function makeData(): GraphData {
  return {
    nodes: [
      { id: "a", path: "wiki/a.md", label: "Alpha", type: "concept", tags: [], starred: false, degree: 4 },
      { id: "b", path: "wiki/b.md", label: "Beta", type: "concept", tags: [], starred: false, degree: 3 },
      { id: "c", path: "wiki/c.md", label: "Gamma", type: "entity", tags: [], starred: false, degree: 2 },
      { id: "d", path: "wiki/d.md", label: "Delta", type: "entity", tags: [], starred: false, degree: 1 },
    ],
    edges: [
      { source: "a", target: "b", relation: "related", weight: 1 },
      { source: "a", target: "c", relation: "related", weight: 1 },
      // Parallel edge to b should not double-count (undirected, distinct neighbors).
      { source: "b", target: "a", relation: "related", weight: 1 },
    ],
    contentHash: "h",
    builtAt: "2026-06-24T00:00:00Z",
    layout: null,
  };
}

describe("graphNeighbors", () => {
  it("counts distinct undirected neighbors", () => {
    expect(neighborCount(makeData(), "a")).toBe(2);
    expect(neighborCount(makeData(), "d")).toBe(0);
  });

  it("lists neighbors ordered by descending degree then label", () => {
    const neighbors = neighborsOf(makeData(), "a");
    expect(neighbors.map((n) => n.id)).toEqual(["b", "c"]);
    expect(neighbors[0].label).toBe("Beta");
    expect(neighbors[0].type).toBe("concept");
  });

  it("honors the limit argument", () => {
    expect(neighborsOf(makeData(), "a", 1)).toHaveLength(1);
  });

  it("returns an empty list for a node with no edges", () => {
    expect(neighborsOf(makeData(), "d")).toEqual([]);
  });
});
