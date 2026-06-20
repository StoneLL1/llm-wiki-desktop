import type { WikiPageType } from "./wiki";

export interface GraphNode {
  id: string;
  path: string;
  label: string;
  type: WikiPageType;
  tags: string[];
  starred: boolean;
  degree: number;
}

export interface GraphEdge {
  source: string;
  target: string;
  relation: string;
  weight: number;
}

export interface GraphLayout {
  positions: Record<string, [number, number]>;
  communities: Record<string, number>;
}

export interface GraphData {
  nodes: GraphNode[];
  edges: GraphEdge[];
  contentHash: string;
  builtAt: string;
  layout?: GraphLayout | null;
}

export interface GraphBuildResult {
  data: GraphData;
  cached: boolean;
  layoutStale: boolean;
}

export interface GraphRequest {
  projectId: string;
  projectRootPath: string;
}

export interface SaveGraphLayoutRequest {
  projectId: string;
  projectRootPath: string;
  contentHash: string;
  positions: Record<string, [number, number]>;
  communities: Record<string, number>;
}

export type GraphColorMode = "type" | "community" | "plain";

export type GraphStatus = "idle" | "loading" | "ready" | "error";

/** Stable palette for type coloring, aligned with the graph.html design swatches. */
export const PAGE_TYPE_COLORS: Record<WikiPageType, string> = {
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

/** Moderate-saturation community palette (FRONTEND_GUIDELINES §8.4). */
export const COMMUNITY_PALETTE = [
  "#0d9488",
  "#7c3aed",
  "#db2777",
  "#ea580c",
  "#0ea5e9",
  "#ca8a04",
  "#16a34a",
  "#dc2626",
  "#475569",
  "#0891b2",
];
