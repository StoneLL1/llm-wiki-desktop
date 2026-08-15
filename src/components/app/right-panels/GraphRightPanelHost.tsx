import { useTranslation } from "react-i18next";

import { GraphInspector } from "../../../features/graph/GraphInspector";
import { useWikiStore } from "../../../features/wiki/wikiStore";
import { useGraphStore } from "../../../stores/graphStore";
import { useNavigationStore } from "../../../stores/navigationStore";
import { RightPanelHeader } from "../RightPanelHeader";
import type { RightPanelHostProps } from "./types";

export function GraphRightPanelHost({ currentProject }: RightPanelHostProps) {
  const { t } = useTranslation();
  const setActiveView = useNavigationStore((state) => state.setActiveView);
  const openWikiPage = useWikiStore((state) => state.openPage);
  const graphData = useGraphStore((state) => state.data);
  const graphStatus = useGraphStore((state) => state.status);
  const graphCached = useGraphStore((state) => state.cached);
  const graphLayoutStale = useGraphStore((state) => state.layoutStale);
  const graphSelectedId = useGraphStore((state) => state.selectedNodeId);
  const graphFocusedId = useGraphStore((state) => state.focusedNodeId);
  const graphSearch = useGraphStore((state) => state.search);
  const setGraphSelectedNode = useGraphStore((state) => state.setSelectedNode);
  const setGraphFocusedNode = useGraphStore((state) => state.setFocusedNodeId);
  const graphTypeFilter = useGraphStore((state) => state.typeFilter);
  const graphDegreeThreshold = useGraphStore((state) => state.degreeThreshold);
  const toggleGraphType = useGraphStore((state) => state.toggleTypeFilter);
  const setGraphDegreeThreshold = useGraphStore((state) => state.setDegreeThreshold);
  const graphExportPng = useGraphStore((state) => state.exportPng);
  const graphRecomputeLayout = useGraphStore((state) => state.recomputeLayout);
  const selectedNode = graphData?.nodes.find((node) => node.id === graphSelectedId) ?? null;

  return (
    <aside id="right-context-panel" aria-label={t("graph.inspector.title")} className="right-panel">
      <RightPanelHeader title={t("graph.inspector.title")} />
      <div className="app-pane-scrollbar min-h-0 flex-1 overflow-y-auto">
        {graphData ? (
          <GraphInspector
            node={selectedNode}
            data={graphData}
            typeFilter={graphTypeFilter}
            degreeThreshold={graphDegreeThreshold}
            search={graphSearch}
            focusedNodeId={graphFocusedId}
            layoutStale={graphLayoutStale}
            cached={graphCached}
            status={graphStatus}
            onOpenPage={() => {
              if (!selectedNode) return;
              setActiveView("wiki");
              void openWikiPage(currentProject.projectId, currentProject.rootPath, selectedNode.path);
            }}
            onFocusNode={setGraphFocusedNode}
            onOpenNeighbor={(nodeId) => setGraphSelectedNode(nodeId)}
            onToggleType={toggleGraphType}
            onDegreeThresholdChange={setGraphDegreeThreshold}
            onExportPng={() => graphExportPng?.()}
            onRecomputeLayout={() => graphRecomputeLayout?.()}
          />
        ) : (
          <p className="px-4 py-3 text-[12px] leading-5 text-[var(--text-muted)]">
            {t("graph.inspector.empty")}
          </p>
        )}
      </div>
    </aside>
  );
}
