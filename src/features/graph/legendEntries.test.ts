import { describe, expect, it } from "vitest";

import { COMMUNITY_PALETTE, type GraphData } from "../../types/graph";
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
    expect(concept?.count).toBe(2);
    expect(concept?.visibleCount).toBe(0);
    expect(concept?.hiddenCount).toBe(2);
    expect(entries.find((e) => e.key === "entity")?.count).toBe(2);
  });

  it("subtracts degree-filtered nodes from type counts but keeps the row", () => {
    // entity: e1 degree 5, e2 degree 1. With threshold 1, e2 (degree<=1) is
    // hidden → entity visible count drops to 1, but the row stays present.
    const entries = typeLegendEntries(makeData(), labels, new Set(), 2);
    const entity = entries.find((e) => e.key === "entity");
    expect(entity?.count).toBe(2);
    expect(entity?.visibleCount).toBe(1);
    expect(entity?.hiddenCount).toBe(1);
    // concept: c1/c2 both degree 3 (>1) → still 2.
    expect(entries.find((e) => e.key === "concept")?.visibleCount).toBe(2);
    // source s1 degree 2 (>1) → still 1.
    expect(entries.find((e) => e.key === "source")?.visibleCount).toBe(1);
  });

  it("reports visible and hidden counts from type, degree, and search filters", () => {
    const entries = typeLegendEntries(makeData(), labels, new Set<WikiPageType>(["source"]), 2, "wiki/e");
    const entity = entries.find((e) => e.key === "entity");
    const concept = entries.find((e) => e.key === "concept");
    const source = entries.find((e) => e.key === "source");

    expect(entity).toMatchObject({ count: 2, visibleCount: 1, hiddenCount: 1 });
    expect(concept).toMatchObject({ count: 2, visibleCount: 0, hiddenCount: 2 });
    expect(source).toMatchObject({ count: 1, visibleCount: 0, hiddenCount: 1 });
  });

  it("keeps every present type hidden when all present types are filtered out", () => {
    const entries = typeLegendEntries(
      makeData(),
      labels,
      new Set<WikiPageType>(["entity", "concept", "source"]),
    );

    expect(entries.every((entry) => entry.visibleCount === 0)).toBe(true);
    expect(entries.map((entry) => entry.hiddenCount)).toEqual([2, 2, 1]);
  });

  it("groups communities by size, top-N then Other", () => {
    const entries = communityLegendEntries(makeData(), 2);
    // community 0 and 1 have 2 each, community 2 has 1 → top 2 then Other(1).
    expect(entries).toHaveLength(3);
    expect(entries[0].count).toBe(2);
    expect(entries.at(-1)?.label).toBe("other");
    expect(entries.at(-1)?.count).toBe(1);
  });

  it("colors community swatches by raw id (matching the canvas), not sort rank", () => {
    // Communities {3:5, 1:3, 7:2} → sorted head [3,1]. The swatch for community
    // 3 must use palette[3 % len], NOT palette[0] (sort rank). This is the same
    // indexing GraphView's baseColorFor uses, so legend matches the canvas even
    // when community ids don't align with size rank.
    const data: GraphData = {
      nodes: [
        { id: "n1", path: "n1", label: "N1", type: "entity", tags: [], starred: false, degree: 1 },
        { id: "n2", path: "n2", label: "N2", type: "entity", tags: [], starred: false, degree: 1 },
      ],
      edges: [],
      contentHash: "h",
      builtAt: "2026-06-24T00:00:00Z",
      layout: { positions: {}, communities: { n1: 3, n2: 1 } },
    };
    const entries = communityLegendEntries(data, 8);
    const palette = COMMUNITY_PALETTE;
    expect(entries.find((e) => e.label === "#3")?.color).toBe(palette[3 % palette.length]);
    expect(entries.find((e) => e.label === "#1")?.color).toBe(palette[1 % palette.length]);
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
