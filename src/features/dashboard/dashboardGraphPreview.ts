import type { GraphData, GraphStatus } from "../../types/graph";
import type { ProjectSummary } from "../../types/project";
import type { BackendTask } from "../../types/task";
import type { WikiPageType, WikiTree } from "../../types/wiki";

export interface DashboardGraphPreviewModel {
  nodeCount: number;
  edgeCount: number;
  pageCount: number;
  graphState: ProjectSummary["graphState"];
  status: GraphStatus;
  activeTaskLabel: string | null;
  topTypes: Array<{ type: string; count: number }>;
  previewNodes: Array<{ id: string; label: string; type: string; x: number; y: number }>;
  previewEdges: Array<{ source: string; target: string }>;
}

const MAX_PREVIEW_NODES = 18;
const MAX_PREVIEW_EDGES = 24;
const PREVIEW_WIDTH = 120;
const PREVIEW_HEIGHT = 72;

export function buildDashboardGraphPreview(
  project: ProjectSummary,
  graphData: GraphData | null,
  graphStatus: GraphStatus,
  tasks: BackendTask[],
  tree: WikiTree | null,
): DashboardGraphPreviewModel {
  const liveNodes = graphData?.nodes ?? [];
  const liveEdges = graphData?.edges ?? [];
  const hasGraphData = Boolean(graphData);
  const previewNodes = liveNodes.slice(0, MAX_PREVIEW_NODES).map((node, index, arr) => {
    const [x, y] = ovalPoint(index, arr.length);
    return { id: node.id, label: node.label, type: node.type, x, y };
  });
  const visibleNodeIds = new Set(previewNodes.map((node) => node.id));
  const previewEdges = liveEdges
    .filter((edge) => visibleNodeIds.has(edge.source) && visibleNodeIds.has(edge.target))
    .slice(0, MAX_PREVIEW_EDGES)
    .map((edge) => ({ source: edge.source, target: edge.target }));

  return {
    nodeCount: hasGraphData ? liveNodes.length : project.wikiPageCount,
    edgeCount: hasGraphData ? liveEdges.length : 0,
    pageCount: tree?.totalPages ?? project.wikiPageCount,
    graphState: hasGraphData ? "cached" : project.graphState,
    status: graphStatus,
    activeTaskLabel: activeGraphTaskLabel(tasks),
    topTypes: topTypes(tree),
    previewNodes,
    previewEdges,
  };
}

export function latestCompileTask(tasks: BackendTask[]): BackendTask | null {
  return (
    tasks
      .filter((task) => task.taskType === "wiki_compile")
      .sort((a, b) => (b.updatedAt ?? "").localeCompare(a.updatedAt ?? ""))[0] ?? null
  );
}

function activeGraphTaskLabel(tasks: BackendTask[]): string | null {
  const task = tasks
    .filter((candidate) => candidate.taskType === "graph_build" && (candidate.status === "running" || candidate.status === "queued"))
    .sort((a, b) => (b.updatedAt ?? "").localeCompare(a.updatedAt ?? ""))[0];
  return task?.title ?? null;
}

function topTypes(tree: WikiTree | null): Array<{ type: string; count: number }> {
  const counts = new Map<WikiPageType, number>();
  for (const page of tree?.pages ?? []) {
    counts.set(page.pageType, (counts.get(page.pageType) ?? 0) + 1);
  }
  return [...counts.entries()]
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .slice(0, 5)
    .map(([type, count]) => ({ type, count }));
}

function ovalPoint(index: number, total: number): [number, number] {
  if (total <= 1) return [PREVIEW_WIDTH / 2, PREVIEW_HEIGHT / 2];
  const angle = (index / total) * Math.PI * 2 - Math.PI / 2;
  const rx = PREVIEW_WIDTH * 0.38;
  const ry = PREVIEW_HEIGHT * 0.34;
  const cx = PREVIEW_WIDTH / 2;
  const cy = PREVIEW_HEIGHT / 2;
  return [
    Number((cx + Math.cos(angle) * rx).toFixed(2)),
    Number((cy + Math.sin(angle) * ry).toFixed(2)),
  ];
}
