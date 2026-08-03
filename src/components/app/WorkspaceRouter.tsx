import { lazy, Suspense } from "react";

import { DashboardView } from "../../features/dashboard/DashboardView";
import type { ImportWorkflow } from "../../features/import/useImportWorkflow";
import type { WorkflowsController } from "../../features/workflows/useWorkflowsController";
import type { AiCapabilitiesWorkflow } from "../../hooks/useAiCapabilities";
import type { AppView } from "../../stores/navigationStore";
import { ViewErrorBoundary } from "./ViewErrorBoundary";
import { ViewFallback } from "./ViewFallback";

const ChatView = lazy(() =>
  import("../../features/chat/ChatView").then((module) => ({
    default: module.ChatView,
  })),
);
const ExportsView = lazy(() =>
  import("../../features/exports/ExportsView").then((module) => ({
    default: module.ExportsView,
  })),
);
const GraphView = lazy(() =>
  import("../../features/graph/GraphView").then((module) => ({
    default: module.GraphView,
  })),
);
const ImportView = lazy(() =>
  import("../../features/import/ImportView").then((module) => ({
    default: module.ImportView,
  })),
);
const LintView = lazy(() =>
  import("../../features/lint/LintView").then((module) => ({
    default: module.LintView,
  })),
);
const WikiView = lazy(() =>
  import("../../features/wiki/WikiView").then((module) => ({
    default: module.WikiView,
  })),
);
const WorkflowsView = lazy(() =>
  import("../../features/workflows/WorkflowsView").then((module) => ({
    default: module.WorkflowsView,
  })),
);

interface WorkspaceRouterProps {
  activeView: AppView;
  capabilities: AiCapabilitiesWorkflow;
  importWorkflow: ImportWorkflow;
  workflowsController: WorkflowsController;
  onOpenTask: (taskId: string) => void;
}

export function WorkspaceRouter({
  activeView,
  capabilities,
  importWorkflow,
  workflowsController,
  onOpenTask,
}: WorkspaceRouterProps) {
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
