import type { GraphData, GraphNode } from "../../types/graph";

/** A neighbor entry for the inspector list: the node plus a type badge color. */
export interface NeighborEntry {
  id: string;
  label: string;
  type: GraphNode["type"];
}

/**
 * Compute the distinct neighbors of `nodeId` from the cached topology edges.
 * Pure over GraphData so it is unit-testable without sigma. Mirrors the
 * neighbor-count logic in RightContextPanel but returns full node metadata so
 * the inspector can render label + type badge and navigate on click.
 */
export function neighborsOf(data: GraphData, nodeId: string, limit?: number): NeighborEntry[] {
  const neighborIds = new Set<string>();
  for (const edge of data.edges) {
    if (edge.source === nodeId) neighborIds.add(edge.target);
    else if (edge.target === nodeId) neighborIds.add(edge.source);
  }
  const byId = new Map<string, GraphNode>();
  for (const node of data.nodes) byId.set(node.id, node);
  const entries: NeighborEntry[] = [];
  for (const id of neighborIds) {
    const node = byId.get(id);
    if (!node) continue;
    entries.push({ id: node.id, label: node.label, type: node.type });
  }
  // Stable, deterministic order: by descending degree (most-connected first),
  // then label — so the list is meaningful rather than edge-iteration order.
  const degreeOf = new Map<string, number>();
  for (const node of data.nodes) degreeOf.set(node.id, node.degree);
  entries.sort((a, b) => {
    const da = degreeOf.get(a.id) ?? 0;
    const db = degreeOf.get(b.id) ?? 0;
    if (db !== da) return db - da;
    return a.label.localeCompare(b.label);
  });
  return typeof limit === "number" ? entries.slice(0, limit) : entries;
}

/** Distinct neighbor count (undirected), matching the inspector's old value. */
export function neighborCount(data: GraphData, nodeId: string): number {
  const ids = new Set<string>();
  for (const edge of data.edges) {
    if (edge.source === nodeId) ids.add(edge.target);
    else if (edge.target === nodeId) ids.add(edge.source);
  }
  return ids.size;
}
