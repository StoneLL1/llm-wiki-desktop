import Graph from "graphology";
import louvain from "graphology-communities-louvain";
import forceAtlas2 from "graphology-layout-forceatlas2";
import FA2LayoutSupervisor from "graphology-layout-forceatlas2/worker";
import Sigma from "sigma";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { RefreshCw } from "lucide-react";

import { useGraphStore } from "../../stores/graphStore";
import { observeProjectResources } from "../../stores/projectScope";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import { useWikiStore } from "../wiki/wikiStore";
import {
  COMMUNITY_PALETTE,
  PAGE_TYPE_COLORS,
  type GraphColorMode,
  type GraphData,
} from "../../types/graph";
import type { WikiPageType } from "../../types/wiki";
import { GraphControls } from "./GraphControls";
import { GraphCanvasControls } from "./GraphCanvasControls";
import { GraphInfo } from "./GraphInfo";
import { GraphLegend } from "./GraphLegend";
import { bindGraphCanvasInteractions, fitGraphToViewport } from "./graphCanvasInteractions";
import { exportGraphPng, exportGraphSvg } from "./graphExport";
import { createLatestLayoutSaveQueue } from "./graphLayoutSaveQueue";
import { GRAPH_DEFAULT_EDGE_COLOR, renderedNodeColor, visualForEdge, visualForNode } from "./graphRenderStyle";
import { buildRenderSnapshot, type RenderSnapshot } from "./graphRenderModel";
import { edgeSizeForWeight, GRAPH_VISUAL_SCALE, nodeSizeForDegree } from "./graphVisualScale";

const EDGE_COLOR = GRAPH_DEFAULT_EDGE_COLOR;
const PLAIN_COLOR = "#9b9b9b";
const DIM_COLOR = "#e8edf2";
const DIM_EDGE_COLOR = "#eef2f6";
const FA2_ITERATIONS = 80;

interface NodeShape {
  x?: number;
  y?: number;
  community?: number;
  label?: string;
  // Our wiki page type (entity/concept/source/...). Deliberately NOT named
  // `type` — sigma v3 reserves the graphology `type` attribute as its node
  // *rendering program* key ("circle"/"point"/...). Setting `type: "comparison"`
  // makes sigma look up a program that doesn't exist and throw
  // "could not find a suitable program for node type" at init. See gotchas.
  pageType?: string;
  color?: string;
  degree?: number;
  size?: number;
  starred?: boolean;
}

interface RenderRefs {
  graph: Graph | null;
  renderer: Sigma | null;
  layout: FA2LayoutSupervisor | null;
  layoutTimer: ReturnType<typeof setTimeout> | null;
  refreshTimer: ReturnType<typeof setInterval> | null;
  /**
   * Precomputed-once-per-refresh render model (options + hidden node set).
   * Updated by `updateRenderSnapshot` before every `refresh()` so the sigma
   * node/edge reducers read a fresh snapshot instead of recomputing options
   * and scanning all nodes on every edge (the O(E*N) hot path from
   * PERF-005). `null` only before the first renderer is constructed.
   */
  snapshot: RenderSnapshot | null;
  /** Live graph data ref so module-level helpers can read current topology. */
  dataRef: { current: GraphData | null };
  /** Live render state ref so module-level helpers can read current state. */
  stateRef: { current: RenderState };
}

interface RenderState {
  hoveredNodeId: string | null;
  hoveredType: WikiPageType | null;
  selectedNodeId: string | null;
  focusedNodeId: string | null;
  search: string;
  typeFilter: Set<WikiPageType>;
  degreeThreshold: number;
  neighborIds: Set<string>;
}

/**
 * Graph view: full-bleed sigma.js canvas over a graphology topology built from
 * the backend cache. ForceAtlas2 + Louvain run on the frontend and persist back
 * through `save_graph_layout` so repeated opens skip recomputation. Falls back
 * to a placeholder when canvas rendering is unavailable (tests / headless).
 */
export function GraphView() {
  const { t } = useTranslation();
  const data = useGraphStore((state) => state.data);
  const status = useGraphStore((state) => state.status);
  const error = useGraphStore((state) => state.error);
  const buildUi = useGraphStore((state) => state.buildUi);
  const layoutStale = useGraphStore((state) => state.layoutStale);
  const colorMode = useGraphStore((state) => state.colorMode);
  const search = useGraphStore((state) => state.search);
  const selectedNodeId = useGraphStore((state) => state.selectedNodeId);
  const focusedNodeId = useGraphStore((state) => state.focusedNodeId);
  const typeFilter = useGraphStore((state) => state.typeFilter);
  const degreeThreshold = useGraphStore((state) => state.degreeThreshold);
  const ensureGraph = useGraphStore((state) => state.ensureGraph);
  const rebuild = useGraphStore((state) => state.rebuild);
  const setColorMode = useGraphStore((state) => state.setColorMode);
  const setSelectedNode = useGraphStore((state) => state.setSelectedNode);
  const setSearch = useGraphStore((state) => state.setSearch);
  const saveLayout = useGraphStore((state) => state.saveLayout);
  const registerActions = useGraphStore((state) => state.registerActions);
  const activeBuildTask = useTaskStore((state) =>
    buildUi.taskId ? state.tasks.find((task) => task.id === buildUi.taskId) ?? null : null,
  );

  const currentProject = useProjectStore((state) => state.currentProject);
  const setActiveView = useNavigationStore((state) => state.setActiveView);
  const openWikiPage = useWikiStore((state) => state.openPage);

  const containerRef = useRef<HTMLDivElement | null>(null);
  const stateRef = useRef<RenderState>({
    hoveredNodeId: null,
    hoveredType: null,
    selectedNodeId,
    focusedNodeId,
    search,
    typeFilter,
    degreeThreshold,
    neighborIds: new Set(),
  });
  // Live pointer to the latest graph data so the snapshot builder (a
  // module-level helper) can read the current topology without being an
  // effect dependency that would re-create the renderer on every data change.
  const dataRef = useRef<GraphData | null>(data);
  dataRef.current = data;
  const refs = useRef<RenderRefs>({
    graph: null,
    renderer: null,
    layout: null,
    layoutTimer: null,
    refreshTimer: null,
    snapshot: null,
    dataRef,
    stateRef,
  });
  const [hoveredType, setHoveredType] = useState<WikiPageType | null>(null);
  const [canvasAvailable, setCanvasAvailable] = useState(true);
  const [zoom, setZoom] = useState<number | null>(null);
  const [layoutSaveQueue] = useState(createLatestLayoutSaveQueue);

  useEffect(() => {
    stateRef.current.selectedNodeId = selectedNodeId;
    syncNeighborIds(refs.current.graph, stateRef.current);
    // Selection changes node size/z-index for the focus root and neighbors, so
    // keep sigma's spatial/program ordering in sync.
    refresh(refs.current, refs.current.renderer);
  }, [selectedNodeId]);

  useEffect(() => {
    stateRef.current.focusedNodeId = focusedNodeId;
    syncNeighborIds(refs.current.graph, stateRef.current);
    // Focus changes node size/z-index for the focus root and neighbors.
    refresh(refs.current, refs.current.renderer);
  }, [focusedNodeId]);

  useEffect(() => {
    stateRef.current.hoveredType = hoveredType;
    // Legend hover only dims non-matching visible nodes — hidden set and
    // positions unchanged, so skip the spatial-index reindex. (PERF-005)
    refreshVisuals(refs.current, refs.current.renderer);
  }, [hoveredType]);

  useEffect(() => {
    stateRef.current.search = search;
    // Search hides/unhides nodes — spatial index must be rebuilt. (PERF-005)
    refresh(refs.current, refs.current.renderer);
  }, [search]);

  // Filter changes (type checkboxes / degree slider) must re-run the node
  // reducer so hidden nodes drop out without rebuilding topology.
  useEffect(() => {
    stateRef.current.typeFilter = typeFilter;
    // Type filter hides/unhides nodes — spatial index must be rebuilt.
    refresh(refs.current, refs.current.renderer);
  }, [typeFilter]);

  useEffect(() => {
    stateRef.current.degreeThreshold = degreeThreshold;
    // Degree filter hides/unhides nodes — spatial index must be rebuilt.
    refresh(refs.current, refs.current.renderer);
  }, [degreeThreshold]);

  const projectId = currentProject.projectId;
  const rootPath = currentProject.rootPath;

  useEffect(() => {
    if (projectId && rootPath) {
      const unobserve = observeProjectResources({ projectId, rootPath }, ["graph"]);
      void ensureGraph(projectId, rootPath);
      return unobserve;
    }
  }, [projectId, rootPath, ensureGraph]);

  // (Re)build the graphology graph + sigma renderer whenever the topology
  // changes. Recomputes layout only when no cached layout is present.
  useEffect(() => {
    const container = containerRef.current;
    if (!container || !data || data.nodes.length === 0) {
      disposeRenderer(refs.current);
      return;
    }

    const { graph, computed } = buildGraph(data, layoutStale);
    refs.current.graph = graph;

    let renderer: Sigma | null = null;
    try {
      renderer = createRenderer(graph, data, container, refs.current);
    } catch (err) {
      // sigma v3 needs a WebGL context; when `canvas.getContext("webgl2" /
      // "webgl" / "experimental-webgl")` all return null (headless, GPU
      // disabled, blacklisted driver, remote desktop w/o hw accel), sigma
      // dereferences the null `gl` and throws. The original `catch {}` swallowed
      // that error silently, leaving only the "canvas unavailable" message with
      // no clue why — so surface it. See SPEC/gotchas.txt.
      console.warn("[graph] sigma renderer init failed:", err);
      setCanvasAvailable(false);
      disposeRenderer(refs.current);
      return;
    }
    setCanvasAvailable(true);
    refs.current.renderer = renderer;

    applyColors(graph, colorMode);
    refresh(refs.current, renderer);

    if (computed) {
      startBackgroundLayout(refs.current, graph, () => {
        refresh(refs.current, renderer);
        void persistLayout(graph, data, projectId, rootPath, saveLayout);
      });
    }

    const onClick = ({ node }: { node: string }) => setSelectedNode(node);
    const onDoubleClick = ({ node }: { node: string }) => {
      openWikiNode(node, graph, setActiveView, () =>
        openWikiPage(projectId, rootPath, node),
      );
    };
    const onEnter = ({ node }: { node: string }) => {
      stateRef.current.hoveredNodeId = node;
      syncNeighborIds(graph, stateRef.current);
      // Hover changes node size/z-index; use a full refresh so picked/rendered
      // item ordering stays aligned with the reducer output.
      refresh(refs.current, renderer);
    };
    const onLeave = () => {
      stateRef.current.hoveredNodeId = null;
      syncNeighborIds(graph, stateRef.current);
      // Hover-end restores node size/z-index.
      refresh(refs.current, renderer);
    };
    const unbindCanvasInteractions = bindGraphCanvasInteractions(renderer, graph, {
      onClearSelection: () => setSelectedNode(null),
      onDragStart: (nodeId) => {
        stopBackgroundLayout(refs.current);
        setSelectedNode(nodeId);
      },
      onDragEnd: () => {
        refresh(refs.current, renderer);
        layoutSaveQueue.request(() => {
          const live = useGraphStore.getState();
          return persistLayout(graph, live.data, projectId, rootPath, saveLayout);
        });
      },
      onDragStateChange: (dragging) => container.classList.toggle("is-dragging", dragging),
    });
    renderer.on("clickNode", onClick);
    renderer.on("doubleClickNode", onDoubleClick);
    renderer.on("enterNode", onEnter);
    renderer.on("leaveNode", onLeave);

    // Report the live zoom ratio to the floating info card. Sigma's camera
    // ratio is inverse to zoom (ratio 1 ≈ fit), so display 1/ratio. Guard
    // against degenerate ratios (dispose, NaN) that would render Infinity.
    const camera = renderer.getCamera();
    const syncZoom = () => {
      const ratio = camera.getState().ratio;
      const z = ratio > 0 && Number.isFinite(ratio) ? 1 / ratio : 1;
      if (Number.isFinite(z)) setZoom(z);
    };
    syncZoom();
    camera.on("updated", syncZoom);

    return () => {
      renderer?.off("clickNode", onClick);
      renderer?.off("doubleClickNode", onDoubleClick);
      renderer?.off("enterNode", onEnter);
      renderer?.off("leaveNode", onLeave);
      unbindCanvasInteractions();
      camera.off("updated", syncZoom);
      disposeRenderer(refs.current);
    };
  }, [data?.contentHash, data?.nodes.length, data?.edges.length, layoutStale]);

  // Recolor when the color mode changes without rebuilding topology.
  useEffect(() => {
    const graph = refs.current.graph;
    if (graph) {
      applyColors(graph, colorMode);
      // Color mode only changes node base colors via applyColors — hidden set
      // and positions unchanged, so skip the spatial-index reindex. (PERF-005)
      refreshVisuals(refs.current, refs.current.renderer);
    }
  }, [colorMode]);

  const handleZoomIn = () => refs.current.renderer?.getCamera().animatedZoom({ duration: 200 });
  const handleZoomOut = () => refs.current.renderer?.getCamera().animatedUnzoom({ duration: 200 });
  const handleFit = () => {
    const renderer = refs.current.renderer;
    if (!renderer) return;
    fitGraphToViewport(renderer, () => refresh(refs.current, renderer));
  };
  const handleResetLayout = () => {
    const graph = refs.current.graph;
    if (!graph) return;
    refs.current.renderer?.setCustomBBox(null);
    seedRandomPositions(graph);
    startBackgroundLayout(refs.current, graph, () => {
      refresh(refs.current, refs.current.renderer);
      void persistLayout(graph, data, projectId, rootPath, saveLayout);
    });
  };
  const handleRebuild = () => {
    if (projectId && rootPath) void rebuild(projectId, rootPath);
  };
  const handleExportSvg = () => {
    const graph = refs.current.graph;
    if (!graph) return;
    const { typeFilter, degreeThreshold, search, selectedNodeId: selected } = useGraphStore.getState();
    exportGraphSvg(graph, currentProject.name, selected, {
      hiddenTypes: typeFilter,
      degreeThreshold,
      search,
    });
  };
  const isGraphBuildActive = buildUi.phase === "loading" || buildUi.phase === "rebuilding";
  const activeTaskProgress = progressRatio(activeBuildTask);
  const activeProgress = activeTaskProgress ?? buildUi.progress;
  const activeLabel = activeBuildTask?.progress?.label ?? buildUi.label;
  const buildLabel = activeLabel?.startsWith("graph.") ? t(activeLabel) : activeLabel;

  // Publish live action hooks so the inspector (rendered in RightContextPanel,
  // which has no access to this component's refs) can export PNG and recompute
  // the layout. The closures read fresh state via getState() so they stay
  // correct after project switches or data reloads without re-registering;
  // cleared on unmount so a stale graph never responds.
  useEffect(() => {
    const exportPng = () => {
      const graph = refs.current.graph;
      if (!graph) return;
      const { typeFilter, degreeThreshold, search, selectedNodeId } = useGraphStore.getState();
      void exportGraphPng(graph, currentProject.name, selectedNodeId, {
        hiddenTypes: typeFilter,
        degreeThreshold,
        search,
      });
    };
    const recomputeLayout = () => {
      const graph = refs.current.graph;
      if (!graph) return;
      refs.current.renderer?.setCustomBBox(null);
      seedRandomPositions(graph);
      startBackgroundLayout(refs.current, graph, () => {
        refresh(refs.current, refs.current.renderer);
        const live = useGraphStore.getState();
        void persistLayout(graph, live.data, projectId, rootPath, saveLayout);
      });
    };
    registerActions({ exportPng, recomputeLayout });
    return () => registerActions({ exportPng: null, recomputeLayout: null });
  }, [registerActions, currentProject.name, projectId, rootPath, saveLayout]);

  if (status === "loading" && !data) {
    return (
      <div className="flex h-full items-center justify-center text-[13px] text-[var(--text-muted)]">
        {t("graph.loading")}
      </div>
    );
  }
  if (status === "error" && !data) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center text-[13px] text-[var(--danger)]">
        <p className="m-0">{error ?? t("graph.error")}</p>
        <button
          type="button"
          onClick={handleRebuild}
          className="h-[28px] rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] text-[var(--text-primary)] hover:bg-[var(--surface-muted)]"
        >
          {t("graph.rebuild")}
        </button>
      </div>
    );
  }
  if (status === "ready-empty") {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center text-[13px] text-[var(--text-muted)]">
        <p className="m-0">{t("graph.empty.noPages")}</p>
        <button
          type="button"
          onClick={() => setActiveView("import")}
          className="h-[28px] rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] text-[var(--text-primary)] hover:bg-[var(--surface-muted)]"
        >
          {t("dashboard.quickActions.import")}
        </button>
      </div>
    );
  }
  if (!data || data.nodes.length === 0) {
    return (
      <div className="flex h-full items-center justify-center px-6 text-center text-[13px] text-[var(--text-muted)]">
        {status === "error" && error ? error : t("graph.empty")}
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <GraphControls
        colorMode={colorMode}
        onColorModeChange={setColorMode}
        search={search}
        onSearchChange={setSearch}
        onRebuild={handleRebuild}
        onExportSvg={handleExportSvg}
        nodeCount={data.nodes.length}
        edgeCount={data.edges.length}
        status={status}
        buildActive={isGraphBuildActive}
      />
      <div className="relative min-h-0 flex-1 p-[var(--sp-4)]">
        <div
          ref={containerRef}
          className={`graph-canvas h-full w-full${isGraphBuildActive ? " is-rebuilding" : ""}`}
          data-testid="graph-canvas-surface"
        >
          {status === "error" && error ? (
            <div className="graph-state-banner graph-state-banner--error" role="status">{error}</div>
          ) : null}
          {canvasAvailable ? (
            <>
              <GraphCanvasControls
                onZoomIn={handleZoomIn}
                onZoomOut={handleZoomOut}
                onFit={handleFit}
                onResetLayout={handleResetLayout}
              />
              <GraphInfo
                zoom={zoom}
                selectedNode={data.nodes.find((n) => n.id === selectedNodeId) ?? null}
              />
              <GraphLegend
                data={data}
                colorMode={colorMode}
                hiddenTypes={typeFilter}
                degreeThreshold={degreeThreshold}
                search={search}
                hoveredType={hoveredType}
                onTypeHover={setHoveredType}
              />
            </>
          ) : (
            <div className="absolute inset-0 flex items-center justify-center bg-[var(--background)] text-[12px] text-[var(--text-muted)]">
              {t("graph.canvasUnavailable")}
            </div>
          )}
          {isGraphBuildActive ? (
            <div className="graph-rebuild-overlay" role="status" aria-label={t("graph.rebuildOverlay.title")}>
              <RefreshCw className="graph-rebuild-overlay__spinner" aria-hidden="true" />
              <div className="graph-rebuild-overlay__title">{t("graph.rebuildOverlay.title")}</div>
              {buildLabel ? <div className="graph-rebuild-overlay__label">{buildLabel}</div> : null}
              {typeof activeProgress === "number" ? (
                <div className="graph-rebuild-overlay__progress">{Math.round(activeProgress * 100)}%</div>
              ) : null}
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}

function progressRatio(task: { progress: { current: number; total: number | null } | null } | null): number | null {
  const progress = task?.progress;
  if (!progress || progress.total === null || progress.total <= 0) return null;
  return Math.max(0, Math.min(1, progress.current / progress.total));
}

function buildGraph(data: GraphData, layoutStale = false): { graph: Graph; computed: boolean } {
  const graph = new Graph();
  for (const node of data.nodes) {
    graph.addNode(node.id, {
      label: node.label,
      size: nodeSize(node.degree),
      pageType: node.type,
      path: node.path,
      tags: node.tags,
      starred: node.starred,
      degree: node.degree,
    });
  }
  for (const edge of data.edges) {
    if (graph.hasNode(edge.source) && graph.hasNode(edge.target) && !graph.hasEdge(edge.source, edge.target)) {
      graph.addEdge(edge.source, edge.target, {
        color: EDGE_COLOR,
        size: edgeSizeForWeight(edge.weight),
      });
    }
  }

  let computed = false;
  const cached = data.layout;
  if (!layoutStale && cached && Object.keys(cached.positions).length > 0) {
    graph.forEachNode((id) => {
      const pos = cached.positions[id];
      graph.setNodeAttribute(id, "x", pos ? pos[0] : Math.random());
      graph.setNodeAttribute(id, "y", pos ? pos[1] : Math.random());
      const community = cached.communities[id] ?? 0;
      graph.setNodeAttribute(id, "community", community);
    });
  } else {
    seedRandomPositions(graph);
    try {
      louvain.assign(graph);
    } catch {
      graph.forEachNode((id) => graph.setNodeAttribute(id, "community", 0));
    }
    computed = true;
  }
  return { graph, computed };
}

function seedRandomPositions(graph: Graph): void {
  graph.forEachNode((id, attrs) => {
    const typed = attrs as NodeShape;
    if (typeof typed.x !== "number") graph.setNodeAttribute(id, "x", Math.random());
    if (typeof typed.y !== "number") graph.setNodeAttribute(id, "y", Math.random());
  });
}

function nodeSize(degree: number): number {
  return nodeSizeForDegree(degree);
}

function createRenderer(
  graph: Graph,
  graphData: GraphData,
  container: HTMLElement,
  refs: RenderRefs,
): Sigma {
  const nodeById = new Map(graphData.nodes.map((node) => [node.id, node]));
  const edgeByKey = new Map(graphData.edges.map((edge) => [edgeKey(edge.source, edge.target), edge]));
  // Empty fallback used if a reducer fires before `refresh()` has built a
  // snapshot — sigma invokes reducers during `new Sigma()` construction, and
  // `disposeRenderer` resets `refs.snapshot` to null so a recreated renderer
  // does not read the previous graph's stale snapshot during that pass. Once
  // the first `refresh()` runs, `refs.snapshot` is populated and every
  // subsequent reducer call reads the precomputed snapshot.
  const emptySnapshot: RenderSnapshot = {
    options: {
      colorMode: useGraphStore.getState().colorMode,
      selectedNodeId: null,
      hoveredNodeId: null,
      focusedNodeId: null,
      search: "",
      typeFilter: new Set(),
      degreeThreshold: 0,
      neighborIds: new Set(),
      hoveredType: null,
      communityByNodeId: new Map(),
    },
    hiddenNodeIds: new Set(),
  };
  const snapshot = (): RenderSnapshot => refs.snapshot ?? emptySnapshot;

  const renderer = new Sigma(graph, container, {
    renderEdgeLabels: false,
    hideEdgesOnMove: false,
    hideLabelsOnMove: true,
    labelDensity: 0.045,
    labelGridCellSize: 96,
    labelRenderedSizeThreshold: 6.4,
    labelSize: 11,
    labelWeight: "500",
    minEdgeThickness: GRAPH_VISUAL_SCALE.minRenderedEdgeThickness,
    zIndex: true,
    defaultEdgeColor: EDGE_COLOR,
    labelColor: { color: "#6b7280" },
  });

  renderer.setSetting("nodeReducer", (node, data) => {
    const source = nodeById.get(node);
    if (!source) return data;
    // Read the precomputed snapshot — no per-node options rebuild, no
    // community Map construction, no hidden-set scan. (PERF-005)
    const visual = visualForNode(source, snapshot().options);
    const baseSize = typeof data.size === "number" ? data.size : 1;
    return {
      ...data,
      hidden: visual.hidden,
      color: renderedNodeColor(visual, DIM_COLOR),
      size: Math.max(1, baseSize + visual.sizeDelta),
      highlighted: visual.highlighted,
      zIndex: visual.highlighted ? 2 : 0,
      forceLabel: visual.forceLabel,
      label: visual.hidden ? "" : data.label,
    };
  });

  renderer.setSetting("edgeReducer", (edge, data) => {
    const [src, tgt] = graph.extremities(edge);
    const snap = snapshot();
    const source = edgeByKey.get(edgeKey(src, tgt)) ?? { source: src, target: tgt, relation: "related", weight: 1 };
    // Read the precomputed hidden set — no per-edge scan of all nodes.
    // (PERF-005: this was the O(E*N) hot path.)
    const visual = visualForEdge(source, snap.options, snap.hiddenNodeIds);
    return {
      ...data,
      hidden: visual.hidden,
      color: visual.opacity < 1 ? DIM_EDGE_COLOR : visual.color,
      size: visual.size,
      zIndex: visual.opacity === 1 ? 1 : 0,
    };
  });

  return renderer;
}

function syncNeighborIds(graph: Graph | null, state: RenderState): void {
  const root = state.hoveredNodeId ?? state.focusedNodeId ?? state.selectedNodeId;
  if (!graph || !root || !graph.hasNode(root)) {
    state.neighborIds = new Set();
    return;
  }
  state.neighborIds = new Set(graph.neighbors(root));
}

function edgeKey(source: string, target: string): string {
  return `${source}\u0000${target}`;
}

function applyColors(graph: Graph, mode: GraphColorMode): void {
  graph.forEachNode((node, attrs) => {
    graph.setNodeAttribute(node, "color", baseColorFor(attrs as NodeShape, mode));
  });
}

function baseColorFor(
  attrs: { pageType?: string; community?: number },
  mode: GraphColorMode,
): string {
  if (mode === "plain") return PLAIN_COLOR;
  if (mode === "community") {
    const community = typeof attrs.community === "number" ? attrs.community : 0;
    return COMMUNITY_PALETTE[community % COMMUNITY_PALETTE.length];
  }
  return PAGE_TYPE_COLORS[attrs.pageType as keyof typeof PAGE_TYPE_COLORS] ?? PLAIN_COLOR;
}

function refresh(refs: RenderRefs, renderer: Sigma | null): void {
  // Reducers read the snapshot, so it must be rebuilt before each refresh.
  // `skipIndexation: false` keeps sigma's spatial index in sync with the
  // current hidden set — required for search/filter/degree changes where
  // hidden nodes drop out of the renderable set. See PERF-005.
  updateRenderSnapshot(refs);
  renderer?.refresh({ skipIndexation: false });
}

/**
 * Visual-only refresh for changes that alter color, opacity, or labels without
 * changing hidden nodes, positions, node size, or z-index.
 *
 * Verified-safe scopes: hoveredType and colorMode. Search/type filters change
 * hidden sets, and selected/focused/hovered nodes change size/z-index.
 */
function refreshVisuals(refs: RenderRefs, renderer: Sigma | null): void {
  updateRenderSnapshot(refs);
  renderer?.refresh({ skipIndexation: true });
}

/**
 * Rebuild the per-refresh render snapshot (options + hidden node set) from the
 * live graph data + render state + current color mode. Pure: assigns to
 * `refs.snapshot` and reads `useGraphStore.getState().colorMode` so it stays
 * correct after color-mode changes without being a render-effect dependency.
 */
function updateRenderSnapshot(refs: RenderRefs): void {
  const data = refs.dataRef.current;
  if (!data) {
    refs.snapshot = null;
    return;
  }
  const state = refs.stateRef.current;
  refs.snapshot = buildRenderSnapshot(data, {
    colorMode: useGraphStore.getState().colorMode,
    selectedNodeId: state.selectedNodeId,
    hoveredNodeId: state.hoveredNodeId,
    focusedNodeId: state.focusedNodeId,
    search: state.search,
    typeFilter: state.typeFilter,
    degreeThreshold: state.degreeThreshold,
    neighborIds: state.neighborIds,
    hoveredType: state.hoveredType,
  });
}

function disposeRenderer(refs: RenderRefs): void {
  stopBackgroundLayout(refs);
  refs.renderer?.kill();
  refs.renderer = null;
  refs.graph = null;
  // Drop the render snapshot so the next renderer's pre-first-refresh
  // construction pass uses the emptySnapshot fallback rather than a stale
  // snapshot referencing the previous graph's node ids.
  refs.snapshot = null;
}

function stopBackgroundLayout(refs: RenderRefs): void {
  refs.layout?.stop();
  refs.layout?.kill();
  refs.layout = null;
  if (refs.layoutTimer) clearTimeout(refs.layoutTimer);
  if (refs.refreshTimer) clearInterval(refs.refreshTimer);
  refs.layoutTimer = null;
  refs.refreshTimer = null;
}

function startBackgroundLayout(
  refs: RenderRefs,
  graph: Graph,
  onComplete: () => void,
): void {
  refs.layout?.kill();
  if (refs.layoutTimer) clearTimeout(refs.layoutTimer);
  if (refs.refreshTimer) clearInterval(refs.refreshTimer);

  if (typeof Worker === "undefined") {
    forceAtlas2.assign(graph, { iterations: FA2_ITERATIONS });
    onComplete();
    return;
  }
  const layout = new FA2LayoutSupervisor(graph, {
    settings: forceAtlas2.inferSettings(graph),
  });
  refs.layout = layout;
  layout.start();
  refs.refreshTimer = setInterval(() => refresh(refs, refs.renderer), 50);
  refs.layoutTimer = setTimeout(() => {
    layout.stop();
    layout.kill();
    refs.layout = null;
    if (refs.refreshTimer) clearInterval(refs.refreshTimer);
    refs.refreshTimer = null;
    refs.layoutTimer = null;
    onComplete();
  }, 1000);
}

async function persistLayout(
  graph: Graph,
  data: GraphData | null,
  projectId: string,
  rootPath: string,
  saveLayout: (projectId: string, rootPath: string, positions: Record<string, [number, number]>, communities: Record<string, number>) => Promise<void>,
): Promise<void> {
  if (!data) return;
  const positions: Record<string, [number, number]> = {};
  const communities: Record<string, number> = {};
  graph.forEachNode((id, attrs) => {
    const typed = attrs as NodeShape;
    // ForceAtlas2 can emit NaN/Infinity for degenerate graphs (single isolated
    // node, coincident points); sanitize before persisting — JSON.stringify(NaN)
    // becomes null and corrupts the cache on reload.
    const x = typeof typed.x === "number" && Number.isFinite(typed.x) ? typed.x : 0;
    const y = typeof typed.y === "number" && Number.isFinite(typed.y) ? typed.y : 0;
    positions[id] = [x, y];
    if (typeof typed.community === "number") communities[id] = typed.community;
  });
  await saveLayout(projectId, rootPath, positions, communities);
}

function openWikiNode(
  nodeId: string,
  graph: Graph | null,
  setActiveView: (view: "wiki") => void,
  open: () => void,
): void {
  if (!graph || !graph.hasNode(nodeId)) return;
  setActiveView("wiki");
  void open();
}
