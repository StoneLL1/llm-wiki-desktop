import type { GraphData } from "../../types/graph";
import { COMMUNITY_PALETTE, type GraphColorMode } from "../../types/graph";
import type { WikiPageType } from "../../types/wiki";

/** A single legend row: color swatch, label, and visible-node count. */
export interface LegendEntry {
  key: string;
  label: string;
  color: string;
  count: number;
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
): LegendEntry[] {
  const present = new Set<WikiPageType>();
  const visibleCounts = new Map<WikiPageType, number>();
  for (const node of data.nodes) {
    present.add(node.type);
    if (degreeThreshold > 0 && node.degree <= degreeThreshold) continue;
    visibleCounts.set(node.type, (visibleCounts.get(node.type) ?? 0) + 1);
  }
  const order: WikiPageType[] = ["entity", "concept", "source", "synthesis", "comparison", "query"];
  return order
    .filter((type) => present.has(type))
    .map((type) => {
      const hidden = hiddenTypes.has(type);
      return {
        key: type,
        label: pageTypeLabels[type],
        color: pageTypeSwatch(type),
        count: hidden ? 0 : (visibleCounts.get(type) ?? 0),
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
    key: `community-${community}`,
    label: `#${community}`,
    color: COMMUNITY_PALETTE[community % COMMUNITY_PALETTE.length],
    count,
  }));
  if (tailTotal > 0) {
    entries.push({ key: "community-other", label: "other", color: "#c4c4c4", count: tailTotal });
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
): LegendEntry[] {
  if (mode === "community") return communityLegendEntries(data);
  if (mode === "plain") return [];
  return typeLegendEntries(data, pageTypeLabels, hiddenTypes, degreeThreshold);
}
