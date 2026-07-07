import { Download, ExternalLink, FileText, RotateCcw } from "lucide-react";
import { useTranslation } from "react-i18next";

import { PAGE_TYPE_COLORS, type GraphData, type GraphStatus } from "../../types/graph";
import { PAGE_TYPE_LABEL_KEYS, WIKI_PAGE_TYPES, type WikiPageType } from "../../types/wiki";
import { neighborsOf } from "./graphNeighbors";
import { graphSearchMatches } from "./graphRenderStyle";

interface GraphInspectorProps {
  node: import("../../types/graph").GraphNode | null;
  data: GraphData;
  typeFilter: Set<WikiPageType>;
  degreeThreshold: number;
  search: string;
  focusedNodeId: string | null;
  layoutStale: boolean;
  cached: boolean;
  status: GraphStatus;
  onOpenPage: () => void;
  onFocusNode: (nodeId: string | null) => void;
  onOpenNeighbor: (nodeId: string) => void;
  onToggleType: (type: WikiPageType) => void;
  onDegreeThresholdChange: (value: number) => void;
  onExportPng: () => void;
  onRecomputeLayout: () => void;
}

const NEIGHBOR_PREVIEW = 6;
const GRAPH_STATUS_LABELS: Record<GraphStatus, string> = {
  idle: "graph.status.idle",
  loading: "graph.loading",
  rebuilding: "graph.status.rebuilding",
  ready: "graph.status.ready",
  "ready-empty": "graph.status.readyEmpty",
  error: "graph.error",
};

/**
 * Selected-node inspector (UI-Frontend-design/graph.html `.rightpanel` graph
 * variant): node header + meta, a neighbor list with type badges and a
 * "view all" affordance, the graph-status block, a filter section
 * (type checkboxes + degree threshold), and action buttons (open page,
 * export PNG, recompute layout).
 */
export function GraphInspector({
  node,
  data,
  typeFilter,
  degreeThreshold,
  search,
  focusedNodeId,
  layoutStale,
  cached,
  status,
  onOpenPage,
  onFocusNode,
  onOpenNeighbor,
  onToggleType,
  onDegreeThresholdChange,
  onExportPng,
  onRecomputeLayout,
}: GraphInspectorProps) {
  const { t } = useTranslation();

  const byId = new Map(data.nodes.map((n) => [n.id, n]));
  const isVisible = (id: string): boolean => {
    const n = byId.get(id);
    if (!n) return false;
    if (typeFilter.has(n.type)) return false;
    if (degreeThreshold > 0 && n.degree < degreeThreshold) return false;
    if (!graphSearchMatches(n, search)) return false;
    return true;
  };
  // Only list neighbors the user can actually see on the canvas — a hidden
  // neighbor would select a node that stays invisible (no visible feedback).
  const neighbors = node ? neighborsOf(data, node.id).filter((n) => isVisible(n.id)) : [];
  const preview = neighbors.slice(0, NEIGHBOR_PREVIEW);
  const totalNeighbors = neighbors.length;
  const communityCount = countCommunities(data);

  const typeCounts = new Map<WikiPageType, number>();
  let maxDegree = 0;
  for (const n of data.nodes) {
    typeCounts.set(n.type, (typeCounts.get(n.type) ?? 0) + 1);
    if (n.degree > maxDegree) maxDegree = n.degree;
  }
  // Clamp the degree slider to the data's real max so a hard 20 can't blank
  // a small wiki entirely; keep at least 0..1 when the graph is trivial.
  const degreeMax = Math.max(1, maxDegree);
  const focusActive = Boolean(node && focusedNodeId === node.id);

  return (
    <div className="px-4 py-3">
      {node ? (
        <>
          {/* Header */}
          <div className="border-b border-[var(--border-subtle)] py-3">
            <div className="mb-1 flex items-center gap-2">
              <span
                className="inline-block h-[10px] w-[10px] rounded-full"
                style={{ background: PAGE_TYPE_COLORS[node.type as keyof typeof PAGE_TYPE_COLORS] ?? "#9b9b9b" }}
                aria-hidden
              />
              <h4 className="m-0 text-[13px] font-semibold text-[var(--text-primary)]">{node.label}</h4>
            </div>
            <p className="m-0 break-all font-mono text-[11px] text-[var(--text-muted)]">{node.path}</p>
          </div>

          {/* Meta */}
          <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-2 border-b border-[var(--border-subtle)] py-3 text-[12px]">
            <dt className="font-medium text-[var(--text-muted)]">{t("graph.inspector.type")}</dt>
            <dd className="m-0 text-[var(--text-primary)]">{t(PAGE_TYPE_LABEL_KEYS[node.type])}</dd>
            <dt className="font-medium text-[var(--text-muted)]">{t("graph.inspector.degree")}</dt>
            <dd className="m-0 font-mono text-[11.5px] text-[var(--text-primary)]">{node.degree}</dd>
            <dt className="font-medium text-[var(--text-muted)]">{t("graph.inspector.neighbors")}</dt>
            <dd className="m-0 font-mono text-[11.5px] text-[var(--text-primary)]">{totalNeighbors}</dd>
          </dl>

          {/* Neighbor list */}
          <div className="border-b border-[var(--border-subtle)] py-3">
            <h5 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-[var(--text-muted)]">
              {t("graph.inspector.neighborList")}
              <span className="ml-1 font-mono font-normal normal-case text-[var(--text-muted)]">{totalNeighbors}</span>
            </h5>
            {preview.length === 0 ? (
              <p className="m-0 text-[11px] text-[var(--text-muted)]">{t("graph.info.none")}</p>
            ) : (
              <div className="flex flex-col gap-0.5">
                {preview.map((neighbor) => (
                  <button
                    key={neighbor.id}
                    type="button"
                    onClick={() => onOpenNeighbor(neighbor.id)}
                    className="flex items-center gap-2 rounded-[var(--radius-sm)] px-1.5 py-1 text-left hover:bg-[var(--surface-muted)]"
                  >
                    <FileText size={13} className="shrink-0 text-[var(--text-muted)]" />
                    <span className="min-w-0 flex-1 truncate text-[12px] text-[var(--text-primary)]">{neighbor.label}</span>
                    <span className="shrink-0 rounded-[var(--radius-pill)] bg-[var(--surface-muted)] px-1.5 py-px text-[9.5px] text-[var(--text-secondary)]">
                      {t(PAGE_TYPE_LABEL_KEYS[neighbor.type])}
                    </span>
                  </button>
                ))}
                {totalNeighbors > preview.length ? (
                  <button
                    type="button"
                    onClick={onOpenPage}
                    className="mt-1 text-center text-[11.5px] text-[var(--accent-hover)] hover:underline"
                  >
                    {t("graph.inspector.viewAll", { count: totalNeighbors })}
                  </button>
                ) : null}
              </div>
            )}
          </div>
        </>
      ) : (
        <p className="border-b border-[var(--border-subtle)] py-3 text-[12px] leading-5 text-[var(--text-muted)]">
          {t("graph.inspector.empty")}
        </p>
      )}

      {/* Graph status */}
      <div className="border-b border-[var(--border-subtle)] py-3">
        <h5 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-[var(--text-muted)]">
          {t("graph.inspector.graphStatus")}
        </h5>
        <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-2 text-[12px]">
          <dt className="font-medium text-[var(--text-muted)]">{t("graph.inspector.status.nodes")}</dt>
          <dd className="m-0 font-mono text-[11.5px] text-[var(--text-primary)]">{data.nodes.length}</dd>
          <dt className="font-medium text-[var(--text-muted)]">{t("graph.inspector.status.edges")}</dt>
          <dd className="m-0 font-mono text-[11.5px] text-[var(--text-primary)]">{data.edges.length}</dd>
          <dt className="font-medium text-[var(--text-muted)]">{t("graph.inspector.status.communities")}</dt>
          <dd className="m-0 font-mono text-[11.5px] text-[var(--text-primary)]">{communityCount} (Louvain)</dd>
          <dt className="font-medium text-[var(--text-muted)]">{t("graph.inspector.status.layout")}</dt>
          <dd className="m-0 font-mono text-[11.5px] text-[var(--text-primary)]">
            {layoutStale
              ? t("graph.status.layoutStale")
              : data.layout
                ? t("graph.inspector.status.layoutCached")
                : t("graph.inspector.status.layoutComputed")}
          </dd>
          <dt className="font-medium text-[var(--text-muted)]">{t("graph.inspector.status.cache")}</dt>
          <dd className="m-0 font-mono text-[11.5px] text-[var(--text-primary)]">
            {cached ? t("graph.status.cached") : t("graph.status.fresh")}
          </dd>
          <dt className="font-medium text-[var(--text-muted)]">{t("graph.inspector.graphStatus")}</dt>
          <dd className="m-0 font-mono text-[11.5px] text-[var(--text-primary)]">{t(GRAPH_STATUS_LABELS[status])}</dd>
        </dl>
      </div>

      {/* Filters */}
      <div className="border-b border-[var(--border-subtle)] py-3">
        <h5 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-[var(--text-muted)]">
          {t("graph.filter.title")}
        </h5>
        <div className="flex flex-col gap-1.5">
          {WIKI_PAGE_TYPES.filter((type) => (typeCounts.get(type) ?? 0) > 0).map((type) => (
            <label key={type} className="flex items-center gap-2 text-[12px] text-[var(--text-secondary)]">
              <input
                type="checkbox"
                checked={!typeFilter.has(type)}
                onChange={() => onToggleType(type)}
                className="accent-[var(--accent)]"
              />
              {t("graph.filter.showType", { type: t(PAGE_TYPE_LABEL_KEYS[type]) })}
              <span className="ml-auto font-mono text-[11px] text-[var(--text-muted)]">{typeCounts.get(type)}</span>
            </label>
          ))}
        </div>
        <div className="mt-2.5 flex items-center gap-2 font-mono text-[11px] text-[var(--text-muted)]">
          <span>{t("graph.filter.degreeThreshold")}</span>
          <input
            type="range"
            min={0}
            max={degreeMax}
            value={degreeThreshold}
            onChange={(event) => onDegreeThresholdChange(Number(event.target.value))}
            className="h-[4px] flex-1 accent-[var(--accent)]"
            aria-label={t("graph.filter.degreeThreshold")}
          />
          <span className="w-5 text-right">{degreeThreshold}</span>
        </div>
      </div>

      {/* Actions */}
      <div className="py-3">
        <h5 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-[var(--text-muted)]">
          {t("graph.inspector.actions")}
        </h5>
        <div className="graph-inspector__actions">
          <button
            type="button"
            onClick={onOpenPage}
            disabled={!node}
            className="flex h-[30px] items-center gap-2 rounded-[var(--radius-md)] bg-[var(--foreground)] px-3 text-[12px] text-[var(--text-inverse)] hover:bg-[var(--text-secondary)] disabled:opacity-40"
          >
            <ExternalLink size={13} />
            {t("graph.inspector.openInWiki")}
          </button>
          <button
            type="button"
            onClick={() => onFocusNode(focusActive ? null : (node?.id ?? null))}
            disabled={!node}
            className="flex h-[30px] items-center gap-2 rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] text-[var(--text-primary)] hover:bg-[var(--surface-muted)] disabled:opacity-40"
          >
            <FileText size={13} />
            {focusActive ? t("graph.inspector.clearFocus") : t("graph.inspector.focusNeighbors")}
          </button>
          <button
            type="button"
            onClick={onExportPng}
            className="flex h-[30px] items-center gap-2 rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] text-[var(--text-primary)] hover:bg-[var(--surface-muted)]"
          >
            <Download size={13} />
            {t("graph.exportPng")}
          </button>
          <button
            type="button"
            onClick={onRecomputeLayout}
            className="flex h-[30px] items-center gap-2 rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] text-[var(--text-primary)] hover:bg-[var(--surface-muted)]"
          >
            <RotateCcw size={13} />
            {t("graph.inspector.recomputeLayout")}
          </button>
        </div>
      </div>
    </div>
  );
}

function countCommunities(data: GraphData): number {
  const communities = data.layout?.communities ?? {};
  const ids = new Set<number>();
  for (const value of Object.values(communities)) ids.add(value);
  return ids.size || (data.nodes.length > 0 ? 1 : 0);
}
