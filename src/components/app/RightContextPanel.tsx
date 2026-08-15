import { lazy, Suspense, type ComponentType } from "react";
import { useTranslation } from "react-i18next";

import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import type { RightPanelHostProps } from "./right-panels/types";
import { RightPanelHeader } from "./RightPanelHeader";
import { ViewErrorBoundary } from "./ViewErrorBoundary";
import { ViewFallback } from "./ViewFallback";

const ProjectSummaryRightPanel = lazy(() =>
  import("./right-panels/ProjectSummaryRightPanel").then((module) => ({
    default: module.ProjectSummaryRightPanel,
  })),
);
const WikiRightPanelHost = lazy(() =>
  import("./right-panels/WikiRightPanelHost").then((module) => ({
    default: module.WikiRightPanelHost,
  })),
);
const ChatRightPanelHost = lazy(() =>
  import("./right-panels/ChatRightPanelHost").then((module) => ({
    default: module.ChatRightPanelHost,
  })),
);
const GraphRightPanelHost = lazy(() =>
  import("./right-panels/GraphRightPanelHost").then((module) => ({
    default: module.GraphRightPanelHost,
  })),
);
const ImportRightPanelHost = lazy(() =>
  import("./right-panels/ImportRightPanelHost").then((module) => ({
    default: module.ImportRightPanelHost,
  })),
);
const WorkflowsRightPanelHost = lazy(() =>
  import("./right-panels/WorkflowsRightPanelHost").then((module) => ({
    default: module.WorkflowsRightPanelHost,
  })),
);

export function RightContextPanel() {
  const { t } = useTranslation();
  const activeView = useNavigationStore((state) => state.activeView);
  const rightPanelMode = useNavigationStore((state) => state.rightPanelMode);
  const currentProject = useProjectStore((state) => state.currentProject);

  if (!currentProject.projectId || !currentProject.rootPath) {
    return (
      <aside
        id="right-context-panel"
        aria-label={t("noProject.context.title")}
        className="right-panel"
      >
        <RightPanelHeader title={t("noProject.context.title")} />
        <div className="app-pane-scrollbar min-h-0 flex-1 overflow-y-auto px-4 py-3">
          <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-3 text-[12px]">
            <dt className="font-medium text-[var(--text-muted)]">{t("noProject.context.state")}</dt>
            <dd className="m-0 text-[var(--text-primary)]">{t("noProject.switcher")}</dd>
            <dt className="font-medium text-[var(--text-muted)]">{t("noProject.context.storage")}</dt>
            <dd className="m-0 text-[var(--text-secondary)]">{t("noProject.context.storageValue")}</dd>
            <dt className="font-medium text-[var(--text-muted)]">{t("noProject.context.policy")}</dt>
            <dd className="m-0 leading-5 text-[var(--text-secondary)]">{t("noProject.context.policyValue")}</dd>
          </dl>
        </div>
      </aside>
    );
  }

  let Panel: ComponentType<RightPanelHostProps> = ProjectSummaryRightPanel;
  if (activeView === "wiki") Panel = WikiRightPanelHost;
  if (activeView === "chat") Panel = ChatRightPanelHost;
  if (activeView === "graph") Panel = GraphRightPanelHost;
  if (activeView === "import") Panel = ImportRightPanelHost;
  if (activeView === "workflows") Panel = WorkflowsRightPanelHost;

  return (
    <ViewErrorBoundary key={`${activeView}:${rightPanelMode}`}>
      <Suspense fallback={<ViewFallback />}>
        <Panel currentProject={currentProject} rightPanelMode={rightPanelMode} />
      </Suspense>
    </ViewErrorBoundary>
  );
}
