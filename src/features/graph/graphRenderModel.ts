import {
  PAGE_TYPE_COLORS,
  type GraphColorMode,
  type GraphData,
  type GraphNode,
} from "../../types/graph";
import type { WikiPageType } from "../../types/wiki";
import { hiddenReasonForNode, type GraphRenderOptions } from "./graphRenderStyle";

/**
 * Inputs to {@link buildRenderSnapshot}. Mirrors the slice of `RenderState` +
 * `useGraphStore` that drives rendering. Pure data — no sigma/React deps.
 */
export interface RenderSnapshotInput {
  colorMode: GraphColorMode;
  selectedNodeId: string | null;
  hoveredNodeId: string | null;
  focusedNodeId: string | null;
  search: string;
  /**
   * Page types the user has UNCHECKED in the legend (hidden from canvas).
   * The snapshot converts this to the visible-types set that
   * {@link GraphRenderOptions.typeFilter} expects (visible types, not hidden).
   */
  typeFilter: Set<WikiPageType>;
  degreeThreshold: number;
  neighborIds: Set<string>;
  hoveredType: WikiPageType | null;
}

/**
 * Precomputed-once-per-refresh render model. Reducers read {@link options} and
 * {@link hiddenNodeIds} directly instead of rebuilding them on every node/edge.
 *
 * Why this exists: the prior `createRenderer` closures called
 * `currentRenderOptions()` (which rebuilt `communityByNodeId` as a `new Map`
 * from `Object.entries(layout.communities)` on every call) and
 * `hiddenNodeIds(options)` (which scanned every node) **inside the edge
 * reducer**, i.e. once per edge — O(E*N) per refresh. This snapshot is built
 * once per refresh and reused by every reducer invocation.
 */
export interface RenderSnapshot {
  options: GraphRenderOptions;
  hiddenNodeIds: Set<string>;
}

/**
 * Convert the store's "hidden types" set (legend unchecked types) into the
 * "visible types" set that {@link GraphRenderOptions.typeFilter} expects.
 *
 * `GraphRenderOptions.typeFilter` is documented as "page types allowed to
 * render; empty means all types are allowed" (see `graphRenderStyle.ts`).
 * The store holds the inverse (unchecked types). When nothing is unchecked,
 * the visible set is empty, which the render-style code treats as "all
 * visible" — matching the pre-snapshot `visibleTypeFilter` helper.
 *
 * This empty-set-means-all equivalence is load-bearing on the
 * `options.typeFilter.size > 0` guard inside `hiddenReasonForNode`: both an
 * empty visible set and a "all types" visible set mean "never hide by type."
 * Any future code path that iterates `typeFilter` directly (rather than
 * `.has(node.type)`) must handle the empty-means-all case explicitly.
 *
 * Exported for tests; not part of the render snapshot contract.
 */
export function visibleTypeFilterFromHidden(
  hiddenTypes: Set<WikiPageType>,
): Set<WikiPageType> {
  if (hiddenTypes.size === 0) return new Set();
  return new Set(
    (Object.keys(PAGE_TYPE_COLORS) as WikiPageType[]).filter(
      (type) => !hiddenTypes.has(type),
    ),
  );
}

/**
 * Build the per-refresh render snapshot: one `GraphRenderOptions` (with a
 * `communityByNodeId` Map constructed once from `layout.communities`) and one
 * `hiddenNodeIds` Set (one scan of `graphData.nodes` against the type/degree/
 * search filters). Reducers should read these instead of recomputing.
 *
 * Pure: no side effects, no sigma/React imports. Safe to call from tests.
 */
export function buildRenderSnapshot(
  graphData: GraphData,
  input: RenderSnapshotInput,
): RenderSnapshot {
  const options: GraphRenderOptions = {
    colorMode: input.colorMode,
    selectedNodeId: input.selectedNodeId,
    hoveredNodeId: input.hoveredNodeId,
    focusedNodeId: input.focusedNodeId,
    search: input.search,
    typeFilter: visibleTypeFilterFromHidden(input.typeFilter),
    degreeThreshold: input.degreeThreshold,
    neighborIds: input.neighborIds,
    hoveredType: input.hoveredType,
    communityByNodeId: new Map(
      Object.entries(graphData.layout?.communities ?? {}),
    ),
  };

  const hiddenNodeIds = computeHiddenNodeIds(graphData.nodes, options);

  return { options, hiddenNodeIds };
}

function computeHiddenNodeIds(
  nodes: GraphNode[],
  options: Pick<GraphRenderOptions, "typeFilter" | "degreeThreshold" | "search">,
): Set<string> {
  const hidden = new Set<string>();
  for (const node of nodes) {
    if (hiddenReasonForNode(node, options) !== null) {
      hidden.add(node.id);
    }
  }
  return hidden;
}
