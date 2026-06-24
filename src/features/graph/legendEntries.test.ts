import { describe, expect, it } from "vitest";

import type { GraphData } from "../../types/graph";
import type { WikiPageType } from "../../types/wiki";
import { communityLegendEntries, legendEntries, typeLegendEntries } from "./legendEntries";

const labels: Record<WikiPageType, string> = {
  entity: "Entity",
  concept: "Concept",
  source: "Source",
  synthesis: "Synthesis",
  comparison: "Comparison",
  query: "Query",
  index: "Index",
  overview: "Overview",
  log: "Log",
  other: "Other",
};

function makeData(): GraphData {
  return {
    nodes: [
      { id: "e1", path: "wiki/e1.md", label: "E1", type: "entity", tags: [], starred: false, degree: 5 },
      { id: "e2", path: "wiki/e2.md", label: "E2", type: "entity", tags: [], starred: false, degree: 1 },
      { id: "c1", path: "wiki/c1.md", label: "C1", type: "concept", tags: [], starred: false, degree: 3 },
      { id: "c2", path: "wiki/c2.md", label: "C2", type: "concept", tags: [], starred: false, degree: 3 },
      { id: "s1", path: "wiki/s1.md", label: "S1", type: "source", tags: [], starred: false, degree: 2 },
    ],
    edges: [{ source: "e1", target: "c1", relation: "related", weight: 1 }],
    contentHash: "h",
    builtAt: "2026-06-24T00:00:00Z",
    layout: {
      positions: {},
      communities: { e1: 0, e2: 0, c1: 1, c2: 1, s1: 2 },
    },
  };
}

describe("legendEntries", () => {
  it("lists present page types with counts in canonical order", () => {
    const entries = typeLegendEntries(makeData(), labels, new Set());
    expect(entries.map((e) => e.key)).toEqual(["entity", "concept", "source"]);
    expect(entries.find((e) => e.key === "entity")?.count).toBe(2);
    expect(entries.find((e) => e.key === "concept")?.count).toBe(2);
  });

  it("zeros and dims types that are currently filtered out", () => {
    const entries = typeLegendEntries(makeData(), labels, new Set<WikiPageType>(["concept"]));
    const concept = entries.find((e) => e.key === "concept");
    expect(concept?.count).toBe(0);
    expect(entries.find((e) => e.key === "entity")?.count).toBe(2);
  });

  it("groups communities by size, top-N then Other", () => {
    const entries = communityLegendEntries(makeData(), 2);
    // community 0 and 1 have 2 each, community 2 has 1 → top 2 then Other(1).
    expect(entries).toHaveLength(3);
    expect(entries[0].count).toBe(2);
    expect(entries.at(-1)?.label).toBe("other");
    expect(entries.at(-1)?.count).toBe(1);
  });

  it("returns no entries in plain mode (component renders a static label)", () => {
    expect(legendEntries("plain", makeData(), labels, new Set())).toEqual([]);
  });

  it("routes community mode to the community grouping", () => {
    const entries = legendEntries("community", makeData(), labels, new Set());
    expect(entries.length).toBeGreaterThan(0);
    expect(entries.every((e) => e.color.startsWith("#"))).toBe(true);
  });
});
