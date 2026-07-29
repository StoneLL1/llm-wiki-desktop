import { lazy, Suspense } from "react";

import type { AgentWorkflow } from "../../features/agent/useAgentWorkflow";
import { DashboardView } from "../../features/dashboard/DashboardView";
import type { ImportWorkflow } from "../../features/import/useImportWorkflow";
import type { AiCapabilitiesWorkflow } from "../../hooks/useAiCapabilities";
import type { TaskLauncher } from "../../hooks/useTaskLauncher";
import type { AppView } from "../../stores/navigationStore";
import type { BackendTask, TaskActivity } from "../../types/task";
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
const AgentView = lazy(() =>
  import("../../features/agent/AgentView").then((module) => ({
    default: module.AgentView,
  })),
);

interface WorkspaceRouterProps {
  activeView: AppView;
  capabilities: AiCapabilitiesWorkflow;
  taskLauncher: TaskLauncher;
  importWorkflow: ImportWorkflow;
  agentWorkflow: AgentWorkflow;
  tasks: BackendTask[];
  activities?: Record<string, TaskActivity[]>;
  taskOutputs?: Record<string, string>;
  onOpenTask: (taskId: string) => void;
  onNavigate: (view: AppView) => void;
}

export function WorkspaceRouter({
  activeView,
  capabilities,
  taskLauncher,
  importWorkflow,
  agentWorkflow,
  tasks,
  activities = {},
  taskOutputs = {},
  onOpenTask,
  onNavigate,
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
      case "agent":
        return (
          <AgentView
            agents={agentWorkflow.agents}
            providers={capabilities.providers}
            tasks={tasks.filter(
              (task) =>
                task.taskType === "wiki_compile" ||
                task.taskType === "agent_run" ||
                task.taskType === "llm_request" ||
                task.taskType === "deep_lint" ||
                task.taskType === "export",
            )}
            activities={activities}
            taskOutputs={taskOutputs}
            onOpenTask={onOpenTask}
            onDetect={capabilities.refresh}
            onRunAgent={agentWorkflow.openRunDialog}
            onSetDefault={agentWorkflow.setDefaultAgent}
            onCancelTask={taskLauncher.cancel}
            onNavigate={onNavigate}
          />
        );
    }
  };

  return (
    <ViewErrorBoundary key={activeView}>
      <Suspense fallback={<ViewFallback />}>{renderActiveView()}</Suspense>
    </ViewErrorBoundary>
  );
}
