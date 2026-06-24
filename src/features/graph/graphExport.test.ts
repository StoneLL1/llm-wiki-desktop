import Graph from "graphology";
import { describe, expect, it } from "vitest";

import { __test__, buildGraphSvg, graphExportFilename } from "./graphExport";

describe("graphExport", () => {
  function seededGraph(): Graph {
    const g = new Graph();
    g.addNode("a", { label: "Agent", x: 0, y: 0, size: 8, color: "#10a37f", type: "concept" });
    g.addNode("b", { label: "Claude", x: 2, y: 1, size: 5, color: "#0d0d0d", type: "entity" });
    g.addNode("c", { label: "<Tagged>", x: 4, y: 0, size: 4, color: "#2563eb", type: "source" });
    g.addEdge("a", "b", { color: "#d4d4d4", size: 1 });
    g.addEdge("a", "c", { color: "#d4d4d4", size: 1 });
    return g;
  }

  it("builds a valid SVG with all nodes and edges projected into the viewBox", () => {
    const svg = buildGraphSvg(seededGraph(), null);
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
    const svg = buildGraphSvg(seededGraph(), null);
    expect(svg).toContain("&lt;Tagged&gt;");
    expect(svg).not.toContain("<Tagged>");
  });

  it("marks the selected node with a teal stroke ring", () => {
    const svg = buildGraphSvg(seededGraph(), "a");
    // The selected circle is the only one carrying a stroke attribute.
    const circles = svg.match(/<circle[^>]*>/g) ?? [];
    const stroked = circles.filter((c) => c.includes('stroke="#0d9488"'));
    expect(stroked).toHaveLength(1);
  });

  it("normalizes coordinates so output stays within the 1200 viewBox bounds", () => {
    const g = new Graph();
    g.addNode("far", { label: "Far", x: 1_000_000, y: -999_000, size: 4, color: "#9b9b9b" });
    g.addNode("near", { label: "Near", x: 0, y: 0, size: 4, color: "#9b9b9b" });
    g.addEdge("far", "near", { color: "#d4d4d4", size: 1 });
    const svg = buildGraphSvg(g, null);
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
    const svg = buildGraphSvg(new Graph(), null);
    expect(svg.match(/<circle/g)?.length ?? 0).toBe(0);
    expect(svg).toContain('fill="#ffffff"');
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
