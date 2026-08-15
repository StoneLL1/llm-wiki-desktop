import { lazy, Suspense } from "react";
import { useTranslation } from "react-i18next";

import { RelatedPagesPanel } from "../../../features/wiki/RelatedPagesPanel";
import { SourceRightPanel } from "../../../features/wiki/SourceRightPanel";
import { useWikiStore } from "../../../features/wiki/wikiStore";
import { useNavigationStore } from "../../../stores/navigationStore";
import { useGraphStore } from "../../../stores/graphStore";
import { RightPanelHeader } from "../RightPanelHeader";
import { ViewErrorBoundary } from "../ViewErrorBoundary";
import { ViewFallback } from "../ViewFallback";
import type { RightPanelHostProps } from "./types";

const PageChatPanel = lazy(() =>
  import("../../../features/chat/PageChatPanel").then((module) => ({
    default: module.PageChatPanel,
  })),
);

export function WikiRightPanelHost({ currentProject, rightPanelMode }: RightPanelHostProps) {
  const { t } = useTranslation();
  const setActiveView = useNavigationStore((state) => state.setActiveView);
  const closeWikiAssistant = useNavigationStore((state) => state.closeWikiAssistant);
  const wikiContent = useWikiStore((state) => state.page);
  const wikiPage = wikiContent?.meta ?? null;
  const wikiTree = useWikiStore((state) => state.tree);
  const openWikiPage = useWikiStore((state) => state.openPage);
  const requestWikiExport = useWikiStore((state) => state.requestExport);
  const graphData = useGraphStore((state) => state.data);
  const setGraphSelectedNode = useGraphStore((state) => state.setSelectedNode);

  const openPage = (path: string) => {
    void openWikiPage(currentProject.projectId, currentProject.rootPath, path);
  };
  const viewWikiPageInGraph = () => {
    if (!wikiPage) return;
    const node = graphData?.nodes.find((candidate) => candidate.path === wikiPage.path);
    setGraphSelectedNode(node?.id ?? wikiPage.path);
    setActiveView("graph");
  };

  if (
    wikiPage?.pageType === "source"
    && wikiPage.sourceBinding?.sourceId
    && wikiPage.sourceBinding.sourceId === wikiPage.sourceId
  ) {
    return (
      <aside id="right-context-panel" aria-label={t("source.panel.title")} className="right-panel">
        <RightPanelHeader title={t("source.panel.title")} />
        <div className="app-pane-scrollbar min-h-0 flex-1 overflow-y-auto">
          <SourceRightPanel
            projectId={currentProject.projectId}
            rootPath={currentProject.rootPath}
            sourceId={wikiPage.sourceBinding.sourceId}
            onOpenPage={openPage}
            onMutation={(path) => {
              void useWikiStore.getState().reload(currentProject.projectId, currentProject.rootPath).then(() => {
                if (path) openPage(path);
              });
            }}
          />
        </div>
      </aside>
    );
  }

  if (rightPanelMode === "wikiAssistant") {
    return (
      <aside id="right-context-panel" aria-label={t("wiki.askAi.panelTitle")} className="right-panel">
        <RightPanelHeader title={t("wiki.askAi.panelTitle")} />
        <div className="min-h-0 flex-1 overflow-hidden">
          <ViewErrorBoundary>
            <Suspense fallback={<ViewFallback />}>
              <PageChatPanel
                page={wikiContent}
                projectId={currentProject.projectId}
                rootPath={currentProject.rootPath}
                onShowRelatedPages={closeWikiAssistant}
                onOpenCitation={openPage}
              />
            </Suspense>
          </ViewErrorBoundary>
        </div>
      </aside>
    );
  }

  return (
    <aside id="right-context-panel" aria-label={t("wiki.related.title")} className="right-panel">
      <RightPanelHeader title={t("wiki.related.title")} />
      <div className="app-pane-scrollbar min-h-0 flex-1 overflow-y-auto">
        <RelatedPagesPanel
          page={wikiPage}
          pages={wikiTree?.pages ?? []}
          onOpenPage={openPage}
          onViewAllBacklinks={viewWikiPageInGraph}
          onGenerateHtml={() => requestWikiExport("beautiful_read")}
          onGenerateCard={() => requestWikiExport("knowledge_card")}
          onViewInGraph={viewWikiPageInGraph}
          onCopyWikilink={() => {
            if (wikiPage) void navigator.clipboard.writeText(`[[${wikiPage.title}]]`);
          }}
        />
      </div>
    </aside>
  );
}
