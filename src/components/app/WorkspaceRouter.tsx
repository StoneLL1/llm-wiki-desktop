import { lazy, Suspense } from "react";

import { DashboardView } from "../../features/dashboard/DashboardView";
import type { ImportWorkflow } from "../../features/import/useImportWorkflow";
import { NoProjectWorkspace } from "../../features/project/NoProjectWorkspace";
import type { WorkflowsController } from "../../features/workflows/useWorkflowsController";
import type { AiCapabilitiesWorkflow } from "../../hooks/useAiCapabilities";
import type { AppView } from "../../stores/navigationStore";
import { ViewErrorBoundary } from "./ViewErrorBoundary";
import { ViewFallback } from "./ViewFallback";
import { loadWorkspaceView } from "./workspaceViewLoaders";

const ChatView = lazy(() =>
  loadWorkspaceView("chat").then((module) => ({
    default: module.ChatView,
  })),
);
const ExportsView = lazy(() =>
  loadWorkspaceView("exports").then((module) => ({
    default: module.ExportsView,
  })),
);
const GraphView = lazy(() =>
  loadWorkspaceView("graph").then((module) => ({
    default: module.GraphView,
  })),
);
const ImportView = lazy(() =>
  loadWorkspaceView("import").then((module) => ({
    default: module.ImportView,
  })),
);
const LintView = lazy(() =>
  loadWorkspaceView("lint").then((module) => ({
    default: module.LintView,
  })),
);
const WikiView = lazy(() =>
  loadWorkspaceView("wiki").then((module) => ({
    default: module.WikiView,
  })),
);
const WorkflowsView = lazy(() =>
  loadWorkspaceView("workflows").then((module) => ({
    default: module.WorkflowsView,
  })),
);

interface ProjectWorkspaceRouterProps {
  activeView: AppView;
  capabilities: AiCapabilitiesWorkflow;
  importWorkflow: ImportWorkflow;
  workflowsController: WorkflowsController;
  onOpenTask: (taskId: string) => void;
  noProject?: false;
}

interface NoProjectWorkspaceRouterProps {
  activeView: AppView;
  noProject: true;
}

type WorkspaceRouterProps = ProjectWorkspaceRouterProps | NoProjectWorkspaceRouterProps;

export function WorkspaceRouter(props: WorkspaceRouterProps) {
  if (props.noProject) {
    return <NoProjectWorkspace activeView={props.activeView} />;
  }

  const {
    activeView,
    capabilities,
    importWorkflow,
    workflowsController,
    onOpenTask,
  } = props;
  const renderActiveView = () => {
    switch (activeView) {
      case "dashboard":
        return <DashboardView />;
      case "wiki":
        return <WikiView capabilities={capabilities} />;
      case "chat":
        return <ChatView />;
      case "graph":
        return <GraphView />;
      case "lint":
        return <LintView />;
      case "exports":
        return <ExportsView />;
      case "import":
        return <ImportView workflow={importWorkflow} capabilities={capabilities} />;
      case "workflows":
        return <WorkflowsView controller={workflowsController} onOpenTask={onOpenTask} />;
    }
  };

  return (
    <ViewErrorBoundary key={activeView}>
      <Suspense fallback={<ViewFallback />}>{renderActiveView()}</Suspense>
    </ViewErrorBoundary>
  );
}
