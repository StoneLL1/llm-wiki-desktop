import {
  COMMUNITY_PALETTE,
  PAGE_TYPE_COLORS,
  type GraphColorMode,
  type GraphEdge,
  type GraphNode,
} from "../../types/graph";
import type { WikiPageType } from "../../types/wiki";

export type GraphHiddenReason = "type" | "degree" | "search" | null;

export interface GraphRenderOptions {
  colorMode: GraphColorMode;
  selectedNodeId: string | null;
  hoveredNodeId: string | null;
  focusedNodeId: string | null;
  search: string;
  /** Page types allowed to render. Empty means all types are allowed. */
  typeFilter: Set<WikiPageType>;
  degreeThreshold: number;
  neighborIds: Set<string>;
  hoveredType: WikiPageType | null;
}

export interface NodeVisual {
  hidden: boolean;
  hiddenReason: GraphHiddenReason;
  color: string;
  sizeDelta: number;
  opacity: number;
  highlighted: boolean;
  borderColor?: string;
  forceLabel: boolean;
}

export interface EdgeVisual {
  hidden: boolean;
  color: string;
  size: number;
  opacity: number;
}

export const GRAPH_SELECTED_COLOR = "#0d9488";

const PLAIN_COLOR = "#9b9b9b";
const DEFAULT_EDGE_COLOR = "#d4d4d4";
const DIMMED_NODE_OPACITY = 0.16;
const HOVERED_TYPE_OPACITY = 0.28;
const DIMMED_EDGE_OPACITY = 0.12;

export function graphSearchMatches(node: GraphNode, search: string): boolean {
  const needle = search.trim().toLocaleLowerCase();
  if (!needle) return true;
  const haystack = [node.label, node.path, ...node.tags]
    .join("\n")
    .toLocaleLowerCase();
  return haystack.includes(needle);
}

export function hiddenReasonForNode(
  node: GraphNode,
  options: Pick<GraphRenderOptions, "typeFilter" | "degreeThreshold" | "search">,
): GraphHiddenReason {
  if (options.typeFilter.size > 0 && !options.typeFilter.has(node.type)) {
    return "type";
  }
  if (options.degreeThreshold > 0 && node.degree < options.degreeThreshold) {
    return "degree";
  }
  if (!graphSearchMatches(node, options.search)) {
    return "search";
  }
  return null;
}

export function visualForNode(node: GraphNode, options: GraphRenderOptions): NodeVisual {
  const hiddenReason = hiddenReasonForNode(node, options);
  const color = nodeColor(node, options.colorMode);
  if (hiddenReason) {
    return {
      hidden: true,
      hiddenReason,
      color,
      sizeDelta: 0,
      opacity: 0,
      highlighted: false,
      forceLabel: false,
    };
  }

  const isSelected = node.id === options.selectedNodeId;
  const isHovered = node.id === options.hoveredNodeId;
  const isFocused = node.id === options.focusedNodeId;
  const hasFocusRoot = Boolean(options.selectedNodeId ?? options.focusedNodeId ?? options.hoveredNodeId);
  const isNeighbor = options.neighborIds.has(node.id);
  let opacity = hasFocusRoot && !(isSelected || isHovered || isFocused || isNeighbor) ? DIMMED_NODE_OPACITY : 1;

  if (options.hoveredType && node.type !== options.hoveredType) {
    opacity = Math.min(opacity, HOVERED_TYPE_OPACITY);
  }

  const highlighted = isSelected || isHovered || isFocused || (hasFocusRoot && isNeighbor);
  const searchHit = options.search.trim().length > 0 && graphSearchMatches(node, options.search);

  return {
    hidden: false,
    hiddenReason: null,
    color,
    sizeDelta: isSelected ? 2 : isHovered || isFocused ? 1 : 0,
    opacity,
    highlighted,
    borderColor: isSelected ? GRAPH_SELECTED_COLOR : undefined,
    forceLabel: searchHit || isSelected || isHovered || isFocused,
  };
}

export function visualForEdge(
  edge: GraphEdge,
  options: GraphRenderOptions,
  hiddenNodeIds: Set<string> = new Set(),
): EdgeVisual {
  if (hiddenNodeIds.has(edge.source) || hiddenNodeIds.has(edge.target)) {
    return { hidden: true, color: DEFAULT_EDGE_COLOR, size: 0, opacity: 0 };
  }

  const activeIds = new Set(
    [options.selectedNodeId, options.focusedNodeId, options.hoveredNodeId].filter((id): id is string => Boolean(id)),
  );
  const hasFocusRoot = activeIds.size > 0;
  const touchesActive = activeIds.has(edge.source) || activeIds.has(edge.target);
  const touchesNeighbor = options.neighborIds.has(edge.source) || options.neighborIds.has(edge.target);
  const highlighted = hasFocusRoot && touchesActive && touchesNeighbor;

  if (highlighted) {
    return {
      hidden: false,
      color: GRAPH_SELECTED_COLOR,
      size: 1.4,
      opacity: 1,
    };
  }

  return {
    hidden: false,
    color: DEFAULT_EDGE_COLOR,
    size: Math.max(0.4, Math.min(1.4, 0.4 + edge.weight * 0.2)),
    opacity: hasFocusRoot ? DIMMED_EDGE_OPACITY : 1,
  };
}

export function visibleNodeIdsForExport(
  nodes: GraphNode[],
  options: Pick<GraphRenderOptions, "typeFilter" | "degreeThreshold" | "search">,
): Set<string> {
  return new Set(nodes.filter((node) => hiddenReasonForNode(node, options) === null).map((node) => node.id));
}

function nodeColor(node: GraphNode, mode: GraphColorMode): string {
  if (mode === "plain") return PLAIN_COLOR;
  if (mode === "community") {
    return COMMUNITY_PALETTE[stableIndex(node.id, COMMUNITY_PALETTE.length)];
  }
  return PAGE_TYPE_COLORS[node.type] ?? PLAIN_COLOR;
}

function stableIndex(value: string, size: number): number {
  let hash = 0;
  for (let i = 0; i < value.length; i += 1) {
    hash = (hash * 31 + value.charCodeAt(i)) >>> 0;
  }
  return size > 0 ? hash % size : 0;
}
