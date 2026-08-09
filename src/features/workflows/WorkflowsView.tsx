import { RefreshCw, X } from "lucide-react";
import { useTranslation } from "react-i18next";

import {
  useWorkflowStore,
  workflowOperationPending,
  type WorkflowOperationError,
  type WorkflowOperationState,
} from "../../stores/workflowStore";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import type { WorkflowsController } from "./useWorkflowsController";
import { WorkflowHistoryView } from "./WorkflowHistoryView";
import { WorkflowsOverviewView } from "./WorkflowsOverview";
import { WorkflowPreparationView } from "./WorkflowPreparationView";
import { WorkflowTaskDetail } from "./WorkflowTaskDetail";

export function WorkflowsView({ controller, onOpenTask }: { controller: WorkflowsController; onOpenTask: (taskId: string) => void }) {
  const { t } = useTranslation();
  const project = useProjectStore((state) => state.currentProject);
  const requestWorkflowLaunch = useNavigationStore((state) => state.requestWorkflowLaunch);
  const overview = useWorkflowStore((state) => state.overview);
  const overviewStatus = useWorkflowStore((state) => state.overviewStatus);
  const runs = useWorkflowStore((state) => state.runs);
  const preparation = useWorkflowStore((state) => state.preparation);
  const selectedTaskId = useWorkflowStore((state) => state.selectedTaskId);
  const surface = useWorkflowStore((state) => state.surface);
  const operations = useWorkflowStore((state) => state.operations);
  const selectedRun = runs.find((run) => run.taskId === selectedTaskId) ?? null;
  const queuedRuns = runs.filter((run) => run.displayStatus === "queued").sort((a, b) => (a.queuePosition ?? 999) - (b.queuePosition ?? 999));
  const overviewError = latestOperationError(operations, ["overview:init", "overview:reconcile"]);
  const surfaceError = latestOperationError(
    operations,
    surface === "preparation"
      ? [`prepare:${preparation?.kind ?? ""}`, `start:${preparation?.preparationId ?? ""}`, "prerequisite:"]
      : surface === "detail"
        ? [`task:${selectedTaskId ?? ""}:`]
        : surface === "history"
          ? ["history:", "task-open:"]
          : ["overview:", "queue:", "prepare:", "task-open:"],
  );
  const surfacePending = surface === "preparation"
    ? workflowOperationPending(operations, `prepare:${preparation?.kind ?? ""}`)
      || workflowOperationPending(operations, `start:${preparation?.preparationId ?? ""}`)
    : surface === "detail"
      ? workflowOperationPending(operations, `task:${selectedTaskId ?? ""}:`)
      : surface === "history"
        ? workflowOperationPending(operations, "history:")
        : !overview && workflowOperationPending(operations, "overview:init");
  const recoverSurfaceError = () => {
    if (!surfaceError) return;
    useWorkflowStore.getState().clearOperationError(surfaceError.key);
    if (surfaceError.key.startsWith("overview:")) {
      void controller.refresh();
      return;
    }
    if (
      surfaceError.key.includes(":hydrate:")
      && selectedTaskId
    ) {
      void controller.openRun(selectedTaskId);
      return;
    }
    if (surfaceError.key.endsWith(":open")) {
      const taskId = surfaceError.key.slice("task:".length, -":open".length);
      if (taskId) void controller.openRun(taskId);
    }
  };
  const surfaceErrorAction = surfaceError?.key.startsWith("overview:")
    ? "workflows.action.refresh"
    : surfaceError?.key.includes(":hydrate:") || surfaceError?.key.endsWith(":open")
      ? "workflows.action.retry"
      : "workflows.action.dismiss";
  const overviewView = (
    <WorkflowsOverviewView
      overview={overview}
      overviewStatus={overviewStatus}
      error={overviewError?.error ?? null}
      runs={runs}
      onRetry={() => void controller.refresh()}
      onPrepare={(kind) => requestWorkflowLaunch({
        projectId: project.projectId,
        projectRootPath: project.rootPath,
        kind,
        origin: "workflows",
        scopePreset: null,
      })}
      onPrerequisite={controller.handlePrerequisite}
      onOpenRun={(taskId) => void controller.openRun(taskId)}
      onContinueQueue={() => void controller.continueQueue()}
    />
  );
  const content = !overview
    ? overviewView
    : surface === "history"
      ? <WorkflowHistoryView runs={runs} onBack={controller.backToOverview} onOpen={(taskId) => void controller.openRun(taskId)} onLoadMore={() => void controller.loadHistoryMore()} />
      : surface === "preparation" && preparation
        ? <WorkflowPreparationView preparation={preparation} onBack={controller.backToOverview} onPrerequisite={controller.handlePrerequisite} onReprepare={(scope, route) => void controller.prepare(preparation.kind, scope, route)} onStart={(restricted, remote) => void controller.startPrepared(restricted, remote)} />
        : surface === "detail" && selectedRun
          ? <WorkflowTaskDetail run={selectedRun} queuedRuns={queuedRuns} controller={controller} onOpenLogs={onOpenTask} />
          : overviewView;

  return (
    <div className="workflows-view app-pane-scrollbar" aria-busy={surfacePending}>
      {surfaceError && overview ? (
        <div className="workflow-error-banner" role="alert">
          <span>{surfaceError.error.summary}</span>
          {surfaceError.error.technicalDetails ? <details><summary>{t("workflows.error.technicalDetails")}</summary><pre>{surfaceError.error.technicalDetails}</pre></details> : null}
          <button className="btn btn--secondary btn--sm" type="button" onClick={recoverSurfaceError}>
            {surfaceError.key.startsWith("overview:") || surfaceError.key.includes(":hydrate:") || surfaceError.key.endsWith(":open")
              ? <RefreshCw size={13} aria-hidden="true" />
              : <X size={13} aria-hidden="true" />}
            {t(surfaceErrorAction)}
          </button>
        </div>
      ) : null}
      {content}
    </div>
  );
}

function latestOperationError(
  operations: Record<string, WorkflowOperationState>,
  prefixes: string[],
): { key: string; error: WorkflowOperationError } | null {
  const entry = Object.entries(operations)
    .filter(([key, operation]) => operation.error && prefixes.some((prefix) =>
      prefix === "task-open:"
        ? key.startsWith("task:") && key.endsWith(":open")
        : key.startsWith(prefix),
    ))
    .sort(([, left], [, right]) => right.requestId - left.requestId)[0];
  return entry?.[1].error ? { key: entry[0], error: entry[1].error } : null;
}
