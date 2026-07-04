import Graph from "graphology";
import { describe, expect, it } from "vitest";

import { __test__, buildGraphSvg, graphExportFilename, type ExportFilters } from "./graphExport";

describe("graphExport", () => {
  const noFilters: ExportFilters = { hiddenTypes: new Set(), degreeThreshold: 0, search: "" };

  function seededGraph(): Graph {
    const g = new Graph();
    // pageType mirrors what GraphView writes (sigma reserves `type`).
    g.addNode("a", { label: "Agent", x: 0, y: 0, size: 8, color: "#10a37f", pageType: "concept", degree: 4, path: "concepts/agent.md", tags: ["llm"] });
    g.addNode("b", { label: "Claude", x: 2, y: 1, size: 5, color: "#0d0d0d", pageType: "entity", degree: 2, path: "entities/claude.md", tags: [] });
    g.addNode("c", { label: "<Tagged>", x: 4, y: 0, size: 4, color: "#2563eb", pageType: "source", degree: 1, path: "sources/paper.md", tags: ["reading"] });
    g.addEdge("a", "b", { color: "#d4d4d4", size: 1 });
    g.addEdge("a", "c", { color: "#d4d4d4", size: 1 });
    return g;
  }

  it("builds a valid SVG with all nodes and edges projected into the viewBox", () => {
    const svg = buildGraphSvg(seededGraph(), null, noFilters);
    expect(svg.startsWith("<svg")).toBe(true);
    expect(svg).toContain('xmlns="http://www.w3.org/2000/svg"');
    // One circle + one text per node → 3 nodes.
    expect(svg.match(/<circle/g)?.length).toBe(3);
    expect(svg.match(/<text/g)?.length).toBe(3);
    // Two edges.
    expect(svg.match(/<line/g)?.length).toBe(2);
    // Node colors preserved from attributes.
    expect(svg).toContain('fill="#10a37f"');
    expect(svg).toContain('fill="#0d0d0d"');
  });

  it("escapes XML-special characters in labels", () => {
    const svg = buildGraphSvg(seededGraph(), null, noFilters);
    expect(svg).toContain("&lt;Tagged&gt;");
    expect(svg).not.toContain("<Tagged>");
  });

  it("marks the selected node with a teal stroke ring", () => {
    const svg = buildGraphSvg(seededGraph(), "a", noFilters);
    // The selected circle is the only one carrying a stroke attribute.
    const circles = svg.match(/<circle[^>]*>/g) ?? [];
    const stroked = circles.filter((c) => c.includes('stroke="#0d9488"'));
    expect(stroked).toHaveLength(1);
  });

  it("normalizes coordinates so output stays within the 1200 viewBox bounds", () => {
    const g = new Graph();
    g.addNode("far", { label: "Far", x: 1_000_000, y: -999_000, size: 4, color: "#9b9b9b", degree: 1 });
    g.addNode("near", { label: "Near", x: 0, y: 0, size: 4, color: "#9b9b9b", degree: 1 });
    g.addEdge("far", "near", { color: "#d4d4d4", size: 1 });
    const svg = buildGraphSvg(g, null, noFilters);
    const xs = [...(svg.matchAll(/cx="([\d.]+)"/g) ?? [])].map((m) => Number(m[1]));
    const ys = [...(svg.matchAll(/cy="([\d.]+)"/g) ?? [])].map((m) => Number(m[1]));
    for (const x of xs) {
      expect(x).toBeGreaterThanOrEqual(0);
      expect(x).toBeLessThanOrEqual(1200);
    }
    for (const y of ys) {
      expect(y).toBeGreaterThanOrEqual(0);
      expect(y).toBeLessThanOrEqual(1200);
    }
  });

  it("returns an empty (background-only) SVG for a graph with no nodes", () => {
    const svg = buildGraphSvg(new Graph(), null, noFilters);
    expect(svg.match(/<circle/g)?.length ?? 0).toBe(0);
    expect(svg).toContain('fill="#ffffff"');
  });

  it("excludes nodes and their edges when a type is filtered out", () => {
    const svg = buildGraphSvg(seededGraph(), null, {
      hiddenTypes: new Set(["entity"]),
      degreeThreshold: 0,
      search: "",
    });
    // entity node "b" dropped → 2 circles, and the a-b edge dropped → 1 line.
    expect(svg.match(/<circle/g)?.length).toBe(2);
    expect(svg.match(/<line/g)?.length).toBe(1);
    expect(svg).not.toContain('fill="#0d0d0d"');
  });

  it("excludes nodes below the degree threshold and prunes dangling edges", () => {
    // c has degree 1; threshold 2 hides degree < 2, so c and the a-c edge drop.
    const svg = buildGraphSvg(seededGraph(), null, {
      hiddenTypes: new Set(),
      degreeThreshold: 2,
      search: "",
    });
    expect(svg.match(/<circle/g)?.length).toBe(2);
    expect(svg.match(/<line/g)?.length).toBe(1);
  });

  it("excludes non-matching nodes when a search query is active", () => {
    const svg = buildGraphSvg(seededGraph(), null, {
      hiddenTypes: new Set(),
      degreeThreshold: 0,
      search: "agent",
    });
    // Only "Agent" matches (case-insensitive).
    expect(svg.match(/<circle/g)?.length).toBe(1);
    expect(svg.match(/<line/g)?.length ?? 0).toBe(0);
  });

  it("matches export search against path and tags as well as labels", () => {
    const svg = buildGraphSvg(seededGraph(), null, {
      hiddenTypes: new Set(),
      degreeThreshold: 0,
      search: "reading",
    });
    expect(svg.match(/<circle/g)?.length).toBe(1);
    expect(svg).toContain("&lt;Tagged&gt;");
  });

  it("builds a timestamped, sanitized filename", () => {
    expect(graphExportFilename("Agent Knowledge Base", "svg")).toMatch(
      /^Agent-Knowledge-Base-graph-\d{8}-\d{4}\.svg$/,
    );
    // Unsafe project names collapse to dashes, never empty.
    expect(graphExportFilename("??/\\bad", "png")).toMatch(/^bad-graph-\d{8}-\d{4}\.png$/);
  });

  it("formats timestamps as YYYYMMDD-HHMM", () => {
    expect(__test__.exportTimestamp(new Date(2026, 5, 24, 9, 7))).toBe("20260624-0907");
  });
});
