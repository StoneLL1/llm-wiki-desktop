import type { GraphData, GraphNode } from "../../types/graph";
import { COMMUNITY_PALETTE, PAGE_TYPE_COLORS, type GraphColorMode } from "../../types/graph";
import type { WikiPageType } from "../../types/wiki";
import { hiddenReasonForNode } from "./graphRenderStyle";

/** A single legend row: color swatch, label, and visible-node count. */
export interface LegendEntry {
  id: string;
  key: string;
  label: string;
  color: string;
  count: number;
  visibleCount: number;
  hiddenCount: number;
}

/**
 * Type counts shown in `type` mode, respecting the active type filter and
 * degree threshold. A type present in the data always renders a row (so the
 * user sees it exists); hidden types are dimmed to zero, and non-hidden types
 * show the count of nodes still visible after the degree filter — matching the
 * on-canvas state rather than the raw topology.
 */
export function typeLegendEntries(
  data: GraphData,
  pageTypeLabels: Record<WikiPageType, string>,
  hiddenTypes: Set<WikiPageType>,
  degreeThreshold = 0,
  search = "",
): LegendEntry[] {
  const present = new Set<WikiPageType>();
  const totalCounts = new Map<WikiPageType, number>();
  const visibleCounts = new Map<WikiPageType, number>();
  const typeFilter = visibleTypeFilter(data.nodes, hiddenTypes);
  for (const node of data.nodes) {
    present.add(node.type);
    totalCounts.set(node.type, (totalCounts.get(node.type) ?? 0) + 1);
    if (hiddenReasonForNode(node, { typeFilter, degreeThreshold, search }) === null) {
      visibleCounts.set(node.type, (visibleCounts.get(node.type) ?? 0) + 1);
    }
  }
  const order: WikiPageType[] = ["entity", "concept", "source", "synthesis", "comparison", "query"];
  return order
    .filter((type) => present.has(type))
    .map((type) => {
      const total = totalCounts.get(type) ?? 0;
      const visible = visibleCounts.get(type) ?? 0;
      return {
        id: type,
        key: type,
        label: pageTypeLabels[type],
        color: pageTypeSwatch(type),
        count: total,
        visibleCount: visible,
        hiddenCount: total - visible,
      };
    });
}

/** Community counts shown in `community` mode: top N by size + "Other". */
export function communityLegendEntries(data: GraphData, topN = 8): LegendEntry[] {
  const communities = data.layout?.communities ?? {};
  const counts = new Map<number, number>();
  for (const node of data.nodes) {
    const community = communities[node.id] ?? 0;
    counts.set(community, (counts.get(community) ?? 0) + 1);
  }
  const sorted = [...counts.entries()].sort((a, b) => b[1] - a[1]);
  const head = sorted.slice(0, topN);
  const tailTotal = sorted.slice(topN).reduce((sum, [, n]) => sum + n, 0);
  // Index the palette by the RAW community id — this is how GraphView's
  // `baseColorFor` paints nodes (`COMMUNITY_PALETTE[community % len]`), so the
  // legend swatch must read the same way or colors won't line up.
  const entries: LegendEntry[] = head.map(([community, count]) => ({
    id: `community-${community}`,
    key: `community-${community}`,
    label: `#${community}`,
    color: COMMUNITY_PALETTE[community % COMMUNITY_PALETTE.length],
    count,
    visibleCount: count,
    hiddenCount: 0,
  }));
  if (tailTotal > 0) {
    entries.push({ id: "community-other", key: "community-other", label: "other", color: "#c4c4c4", count: tailTotal, visibleCount: tailTotal, hiddenCount: 0 });
  }
  return entries;
}

function pageTypeSwatch(type: WikiPageType): string {
  const palette: Record<WikiPageType, string> = {
    entity: "#0d0d0d",
    concept: "#10a37f",
    source: "#2563eb",
    synthesis: "#f5a623",
    comparison: "#9b9b9b",
    query: "#c4c4c4",
    index: "#6b7280",
    overview: "#6b7280",
    log: "#6b7280",
    other: "#9b9b9b",
  };
  return palette[type] ?? "#9b9b9b";
}

/** Resolve legend entries for the active color mode. */
export function legendEntries(
  mode: GraphColorMode,
  data: GraphData,
  pageTypeLabels: Record<WikiPageType, string>,
  hiddenTypes: Set<WikiPageType>,
  degreeThreshold = 0,
  search = "",
): LegendEntry[] {
  if (mode === "community") return communityLegendEntries(data);
  if (mode === "plain") return [];
  return typeLegendEntries(data, pageTypeLabels, hiddenTypes, degreeThreshold, search);
}

function visibleTypeFilter(nodes: GraphNode[], hiddenTypes: Set<WikiPageType>): Set<WikiPageType> {
  if (hiddenTypes.size === 0) return new Set();
  void nodes;
  return new Set((Object.keys(PAGE_TYPE_COLORS) as WikiPageType[]).filter((type) => !hiddenTypes.has(type)));
}
