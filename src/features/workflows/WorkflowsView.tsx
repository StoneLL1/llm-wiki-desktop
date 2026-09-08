import { RefreshCw, X } from "lucide-react";
import { useTranslation } from "react-i18next";

import {
  useWorkflowStore,
  workflowOperationPending,
  type WorkflowOperationError,
  type WorkflowOperationState,
} from "../../stores/workflowStore";
import { useTaskStore } from "../../stores/taskStore";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import type { WorkflowsController } from "./useWorkflowsController";
import { WorkflowHistoryView } from "./WorkflowHistoryView";
import { historyPageErrorRequiresRefresh } from "./workflowHistoryRecovery";
import { WorkflowsOverviewView } from "./WorkflowsOverview";
import { WorkflowDraftForm } from "./WorkflowDraftForm";
import { UpdateWikiForm } from "./UpdateWikiForm";
import { WorkflowPreparationView } from "./WorkflowPreparationView";
import { WorkflowTaskDetail } from "./WorkflowTaskDetail";

export function WorkflowsView({ controller, onOpenTask }: { controller: WorkflowsController; onOpenTask: (taskId: string) => void }) {
  const { t } = useTranslation();
  const project = useProjectStore((state) => state.currentProject);
  const requestWorkflowLaunch = useNavigationStore((state) => state.requestWorkflowLaunch);
  const overview = useWorkflowStore((state) => state.overview);
  const overviewStatus = useWorkflowStore((state) => state.overviewStatus);
  const selectedRun = useWorkflowStore((state) => state.runs.find((run) => run.taskId === state.selectedTaskId) ?? null);
  const summaryById = useTaskStore((state) => state.workflowById);
  const historyRuns = useWorkflowStore((state) => state.historyRuns);
  const historyKind = useWorkflowStore((state) => state.historyKind);
  const historyStatus = useWorkflowStore((state) => state.historyStatus);
  const historyCursor = useWorkflowStore((state) => state.historyCursor);
  const preparation = useWorkflowStore((state) => state.preparation);
  const preparingKind = useWorkflowStore((state) => state.preparingKind);
  const preparationKind = preparingKind ?? preparation?.kind;
  const selectedTaskId = useWorkflowStore((state) => state.selectedTaskId);
  const surface = useWorkflowStore((state) => state.surface);
  const operations = useWorkflowStore((state) => state.operations);
  const queuedRuns = Object.values(summaryById).filter((run) => run.projectId === project.projectId
    && run.canonicalIdentityKey === overview?.projectAccess?.canonicalIdentityKey
    && run.identityRevision === overview?.projectAccess?.identityRevision
    && run.displayStatus === "queued").sort((a, b) => (a.queuePosition ?? 999) - (b.queuePosition ?? 999));
  const overviewError = latestOperationError(operations, ["overview:init", "overview:reconcile"]);
  const surfaceError = latestOperationError(
    operations,
    surface === "preparation"
      ? ["update:start", `draft:start:${preparationKind ?? ""}`, `prepare:${preparationKind ?? ""}`, `start:${preparation?.preparationId ?? ""}`, "prerequisite:"]
      : surface === "detail"
        ? [`task:${selectedTaskId ?? ""}:`]
        : surface === "history"
          ? ["history:", "task-open:", "task-retry:"]
          : ["overview:", "queue:", "prepare:", "task-open:"],
  );
  const surfacePending = surface === "preparation"
    ? workflowOperationPending(operations, `draft:start:${preparationKind ?? ""}`)
      || workflowOperationPending(operations, `prepare:${preparationKind ?? ""}`)
      || workflowOperationPending(operations, `start:${preparation?.preparationId ?? ""}`)
    : surface === "detail"
      ? workflowOperationPending(operations, `task:${selectedTaskId ?? ""}:`)
      : surface === "history"
      ? workflowOperationPending(operations, "history:")
        : !overview && workflowOperationPending(operations, "overview:init");
  const preparationStale = surfaceError?.key.startsWith("start:")
    && surfaceError.error.technicalDetails?.includes("WORKFLOW_PREPARATION_STALE") === true;
  const historyPageRequiresRefresh = surface === "history"
    && surfaceError?.key === "history:page"
    && historyPageErrorRequiresRefresh(surfaceError.error.technicalDetails);
  const historyTaskRetryError = surface === "history" && surfaceError?.key.endsWith(":retry");
  const recoverSurfaceError = () => {
    if (!surfaceError) return;
    useWorkflowStore.getState().clearOperationError(surfaceError.key);
    if (surfaceError.key.startsWith("overview:")) {
      void controller.refresh();
      return;
    }
    if ((surfaceError.key.startsWith("prepare:") || preparationStale) && preparationKind) {
      void controller.prepare(preparationKind);
      return;
    }
    if (surfaceError.key.startsWith("history:")) {
      if (historyPageRequiresRefresh) {
        void controller.filterHistory(historyKind, historyStatus);
      } else if (surfaceError.key === "history:page" && historyCursor) {
        void controller.loadHistoryMore();
      } else {
        void controller.filterHistory(historyKind, historyStatus);
      }
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
      return;
    }
    if (historyTaskRetryError) {
      const taskId = surfaceError.key.slice("task:".length, -":retry".length);
      if (taskId) void controller.retry(taskId);
    }
  };
  const surfaceErrorAction = preparationStale ? "workflows.action.prepareAgain"
    : surfaceError?.key.startsWith("overview:") || historyPageRequiresRefresh
    ? "workflows.action.refresh"
    : surfaceError?.key.startsWith("prepare:") || surfaceError?.key.startsWith("history:") || surfaceError?.key.includes(":hydrate:") || surfaceError?.key.endsWith(":open") || historyTaskRetryError
      ? "workflows.action.retry"
      : "workflows.action.dismiss";
  const surfaceErrorCanRecover = preparationStale || surfaceError?.key.startsWith("overview:")
    || surfaceError?.key.startsWith("prepare:")
    || surfaceError?.key.startsWith("history:")
    || surfaceError?.key.includes(":hydrate:")
    || surfaceError?.key.endsWith(":open")
    || historyTaskRetryError;
  const panel = surface === "preparation" && preparationKind === "update_wiki"
    ? <UpdateWikiForm key={`${project.projectId}\0${project.rootPath}`} project={project} onBack={controller.backToOverview} onStart={controller.startUpdate} />
    : surface === "preparation" && (preparingKind === "health_check" || preparingKind === "generate_content")
    ? <WorkflowDraftForm key={`${project.projectId}\0${project.rootPath}\0${preparingKind}`} kind={preparingKind} project={project}
        lastHealth={overview?.contextSummary?.lastHealth} onOpenLastHealth={(taskId) => void controller.openRun(taskId)}
        onBack={controller.backToOverview} onStart={(draft) => controller.startDraft(preparingKind, draft)} />
    : surface === "preparation" && preparation
    ? <WorkflowPreparationView key={preparation.kind} lastHealth={overview?.contextSummary?.lastHealth} onOpenLastHealth={(taskId) => void controller.openRun(taskId)} preparation={preparation} onBack={controller.backToOverview} onPrerequisite={controller.handlePrerequisite} onStart={(restricted, remote, draft) => void controller.startPrepared(restricted, remote, draft)} />
    : surface === "detail" && selectedRun
      ? <WorkflowTaskDetail run={selectedRun} queuedRuns={queuedRuns} controller={controller} onOpenLogs={onOpenTask} />
      : null;
  const overviewView = (
    <WorkflowsOverviewView
      selectedKind={surface === "preparation" ? preparationKind : surface === "detail" ? selectedRun?.kind : null}
      overview={overview}
      overviewStatus={overviewStatus}
      error={overviewError?.error ?? null}
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
    >
      {panel}
    </WorkflowsOverviewView>
  );
  const content = overview && surface === "history"
    ? <WorkflowHistoryView runs={historyRuns} onBack={controller.backToOverview} onFilter={(kind, status) => void controller.filterHistory(kind, status)} onOpen={(taskId) => void controller.openRun(taskId)} onRetry={(taskId) => void controller.retry(taskId)} onLoadMore={() => void controller.loadHistoryMore()} />
    : overviewView;

  return (
    <div className="workflows-view app-pane-scrollbar" aria-busy={surfacePending}>
      {surfaceError && overview ? (
        <div className="workflow-error-banner" role="alert">
          <span>{surfaceError.error.summary}</span>
          {surfaceError.error.technicalDetails ? <details><summary>{t("workflows.error.technicalDetails")}</summary><pre>{surfaceError.error.technicalDetails}</pre></details> : null}
          <button className="btn btn--secondary btn--sm" type="button" onClick={recoverSurfaceError}>
            {surfaceErrorCanRecover
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
        : prefix === "task-retry:"
          ? key.startsWith("task:") && key.endsWith(":retry")
          : key.startsWith(prefix),
    ))
    .sort(([, left], [, right]) => right.requestId - left.requestId)[0];
  return entry?.[1].error ? { key: entry[0], error: entry[1].error } : null;
}
