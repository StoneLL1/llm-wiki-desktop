import Graph from "graphology";
import louvain from "graphology-communities-louvain";
import forceAtlas2 from "graphology-layout-forceatlas2";
import FA2LayoutSupervisor from "graphology-layout-forceatlas2/worker";
import Sigma from "sigma";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { useGraphStore } from "../../stores/graphStore";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { useWikiStore } from "../wiki/wikiStore";
import {
  COMMUNITY_PALETTE,
  PAGE_TYPE_COLORS,
  type GraphColorMode,
  type GraphData,
} from "../../types/graph";
import { GraphControls } from "./GraphControls";
import { GraphCanvasControls } from "./GraphCanvasControls";
import { exportGraphSvg } from "./graphExport";

const EDGE_COLOR = "#d4d4d4";
const PLAIN_COLOR = "#9b9b9b";
const SELECTED_COLOR = "#0d9488";
const DIM_COLOR = "#ececec";
const FA2_ITERATIONS = 80;

interface NodeShape {
  x?: number;
  y?: number;
  community?: number;
  label?: string;
  type?: string;
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
  const colorMode = useGraphStore((state) => state.colorMode);
  const search = useGraphStore((state) => state.search);
  const selectedNodeId = useGraphStore((state) => state.selectedNodeId);
  const load = useGraphStore((state) => state.load);
  const rebuild = useGraphStore((state) => state.rebuild);
  const setColorMode = useGraphStore((state) => state.setColorMode);
  const setSelectedNode = useGraphStore((state) => state.setSelectedNode);
  const setSearch = useGraphStore((state) => state.setSearch);
  const saveLayout = useGraphStore((state) => state.saveLayout);

  const currentProject = useProjectStore((state) => state.currentProject);
  const setActiveView = useNavigationStore((state) => state.setActiveView);
  const openWikiPage = useWikiStore((state) => state.openPage);

  const containerRef = useRef<HTMLDivElement | null>(null);
  const refs = useRef<RenderRefs>({ graph: null, renderer: null, layout: null, layoutTimer: null, refreshTimer: null });
  const stateRef = useRef({ hovered: null as string | null, selected: selectedNodeId, search });
  const [canvasAvailable, setCanvasAvailable] = useState(true);

  useEffect(() => {
    stateRef.current.selected = selectedNodeId;
    refresh(refs.current.renderer);
  }, [selectedNodeId]);

  useEffect(() => {
    stateRef.current.search = search;
    refresh(refs.current.renderer);
  }, [search]);

  const projectId = currentProject.projectId;
  const rootPath = currentProject.rootPath;

  useEffect(() => {
    if (projectId && rootPath) {
      void load(projectId, rootPath);
    }
  }, [projectId, rootPath, load]);

  // (Re)build the graphology graph + sigma renderer whenever the topology
  // changes. Recomputes layout only when no cached layout is present.
  useEffect(() => {
    const container = containerRef.current;
    if (!container || !data || data.nodes.length === 0) {
      disposeRenderer(refs.current);
      return;
    }

    const { graph, computed } = buildGraph(data);
    refs.current.graph = graph;

    let renderer: Sigma | null = null;
    try {
      renderer = createRenderer(graph, container, stateRef);
    } catch {
      setCanvasAvailable(false);
      disposeRenderer(refs.current);
      return;
    }
    setCanvasAvailable(true);
    refs.current.renderer = renderer;

    applyColors(graph, colorMode);
    refresh(renderer);

    if (computed) {
      startBackgroundLayout(refs.current, graph, () => {
        refresh(renderer);
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
      stateRef.current.hovered = node;
      refresh(renderer);
    };
    const onLeave = () => {
      stateRef.current.hovered = null;
      refresh(renderer);
    };
    renderer.on("clickNode", onClick);
    renderer.on("doubleClickNode", onDoubleClick);
    renderer.on("enterNode", onEnter);
    renderer.on("leaveNode", onLeave);

    return () => {
      renderer?.off("clickNode", onClick);
      renderer?.off("doubleClickNode", onDoubleClick);
      renderer?.off("enterNode", onEnter);
      renderer?.off("leaveNode", onLeave);
      disposeRenderer(refs.current);
    };
  }, [data?.contentHash]);

  // Recolor when the color mode changes without rebuilding topology.
  useEffect(() => {
    const graph = refs.current.graph;
    if (graph) {
      applyColors(graph, colorMode);
      refresh(refs.current.renderer);
    }
  }, [colorMode]);

  const handleZoomIn = () => refs.current.renderer?.getCamera().animatedZoom({ duration: 200 });
  const handleZoomOut = () => refs.current.renderer?.getCamera().animatedUnzoom({ duration: 200 });
  const handleFit = () => refs.current.renderer?.getCamera().animatedReset({ duration: 300 });
  const handleResetLayout = () => {
    const graph = refs.current.graph;
    if (!graph) return;
    seedRandomPositions(graph);
    startBackgroundLayout(refs.current, graph, () => {
      refresh(refs.current.renderer);
      void persistLayout(graph, data, projectId, rootPath, saveLayout);
    });
  };
  const handleRebuild = () => {
    if (projectId && rootPath) void rebuild(projectId, rootPath);
  };
  const handleExportSvg = () => {
    const graph = refs.current.graph;
    if (!graph) return;
    exportGraphSvg(graph, currentProject.name, selectedNodeId);
  };

  if (status === "loading") {
    return (
      <div className="flex h-full items-center justify-center text-[13px] text-[var(--text-muted)]">
        {t("graph.loading")}
      </div>
    );
  }
  if (status === "error") {
    return (
      <div className="flex h-full items-center justify-center px-6 text-center text-[13px] text-[var(--text-danger)]">
        {error ?? t("graph.error")}
      </div>
    );
  }
  if (!data || data.nodes.length === 0) {
    return (
      <div className="flex h-full items-center justify-center px-6 text-center text-[13px] text-[var(--text-muted)]">
        {t("graph.empty")}
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
      />
      <div className="relative min-h-0 flex-1 p-[var(--sp-4)]">
        <div ref={containerRef} className="graph-canvas h-full w-full">
          {canvasAvailable ? (
            <GraphCanvasControls
              onZoomIn={handleZoomIn}
              onZoomOut={handleZoomOut}
              onFit={handleFit}
              onResetLayout={handleResetLayout}
            />
          ) : (
            <div className="absolute inset-0 flex items-center justify-center bg-[var(--background)] text-[12px] text-[var(--text-muted)]">
              {t("graph.canvasUnavailable")}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function buildGraph(data: GraphData): { graph: Graph; computed: boolean } {
  const graph = new Graph();
  for (const node of data.nodes) {
    graph.addNode(node.id, {
      label: node.label,
      size: nodeSize(node.degree),
      type: node.type,
      starred: node.starred,
      degree: node.degree,
    });
  }
  for (const edge of data.edges) {
    if (graph.hasNode(edge.source) && graph.hasNode(edge.target) && !graph.hasEdge(edge.source, edge.target)) {
      graph.addEdge(edge.source, edge.target, {
        color: EDGE_COLOR,
        size: Math.max(0.4, Math.min(1.4, 0.4 + edge.weight * 0.2)),
      });
    }
  }

  let computed = false;
  const cached = data.layout;
  if (cached && Object.keys(cached.positions).length > 0) {
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
  return 4 + Math.min(8, Math.sqrt(degree) * 1.6);
}

function createRenderer(
  graph: Graph,
  container: HTMLElement,
  stateRef: React.RefObject<{ hovered: string | null; selected: string | null; search: string }>,
): Sigma {
  const renderer = new Sigma(graph, container, {
    renderEdgeLabels: false,
    labelDensity: 0.07,
    labelGridCellSize: 80,
    labelRenderedSizeThreshold: 6,
    defaultEdgeColor: EDGE_COLOR,
    labelColor: { color: "#6b7280" },
  });

  renderer.setSetting("nodeReducer", (node, data) => {
    const state = stateRef.current;
    const next = { ...data };
    const matchesSearch =
      !state.search ||
      String(data.label ?? "")
        .toLowerCase()
        .includes(state.search.toLowerCase());
    const hovered = state.hovered;
    const isNeighbor =
      hovered && (node === hovered || graph.areNeighbors(node, hovered));
    if (state.search && !matchesSearch) {
      next.color = DIM_COLOR;
      next.hidden = true;
    }
    if (hovered && !isNeighbor) {
      next.color = DIM_COLOR;
    }
    if (state.selected === node) {
      next.highlighted = true;
      next.color = SELECTED_COLOR;
    }
    return next;
  });

  renderer.setSetting("edgeReducer", (edge, data) => {
    const state = stateRef.current;
    if (!state.hovered) return data;
    const [src, tgt] = graph.extremities(edge);
    const touched = src === state.hovered || tgt === state.hovered;
    return touched ? { ...data, color: SELECTED_COLOR, size: 1.4 } : { ...data, color: "#ececec" };
  });

  return renderer;
}

function applyColors(graph: Graph, mode: GraphColorMode): void {
  graph.forEachNode((node, attrs) => {
    graph.setNodeAttribute(node, "color", baseColorFor(attrs as NodeShape, mode));
  });
}

function baseColorFor(
  attrs: { type?: string; community?: number },
  mode: GraphColorMode,
): string {
  if (mode === "plain") return PLAIN_COLOR;
  if (mode === "community") {
    const community = typeof attrs.community === "number" ? attrs.community : 0;
    return COMMUNITY_PALETTE[community % COMMUNITY_PALETTE.length];
  }
  return PAGE_TYPE_COLORS[attrs.type as keyof typeof PAGE_TYPE_COLORS] ?? PLAIN_COLOR;
}

function refresh(renderer: Sigma | null): void {
  renderer?.refresh({ skipIndexation: false });
}

function disposeRenderer(refs: RenderRefs): void {
  refs.layout?.kill();
  refs.layout = null;
  if (refs.layoutTimer) clearTimeout(refs.layoutTimer);
  if (refs.refreshTimer) clearInterval(refs.refreshTimer);
  refs.layoutTimer = null;
  refs.refreshTimer = null;
  refs.renderer?.kill();
  refs.renderer = null;
  refs.graph = null;
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
  refs.refreshTimer = setInterval(() => refresh(refs.renderer), 50);
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
