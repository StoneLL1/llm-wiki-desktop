import { useWorkflowStore } from "../../stores/workflowStore";
import type { WorkflowsController } from "./useWorkflowsController";
import { WorkflowHistoryView } from "./WorkflowHistoryView";
import { WorkflowsOverviewView } from "./WorkflowsOverview";
import { WorkflowPreparationView } from "./WorkflowPreparationView";
import { WorkflowTaskDetail } from "./WorkflowTaskDetail";

export function WorkflowsView({ controller, onOpenTask }: { controller: WorkflowsController; onOpenTask: (taskId: string) => void }) {
  const overview = useWorkflowStore((state) => state.overview);
  const runs = useWorkflowStore((state) => state.runs);
  const preparation = useWorkflowStore((state) => state.preparation);
  const selectedTaskId = useWorkflowStore((state) => state.selectedTaskId);
  const surface = useWorkflowStore((state) => state.surface);
  const loading = useWorkflowStore((state) => state.loading);
  const error = useWorkflowStore((state) => state.error);
  const selectRun = useWorkflowStore((state) => state.selectRun);
  const selectedRun = runs.find((run) => run.taskId === selectedTaskId) ?? null;
  const queuedRuns = runs.filter((run) => run.displayStatus === "queued").sort((a, b) => (a.queuePosition ?? 999) - (b.queuePosition ?? 999));
  return <div className="workflows-view app-pane-scrollbar" aria-busy={loading}>
    {error ? <div className="workflow-error-banner" role="alert">{error}</div> : null}
    {surface === "history" ? <WorkflowHistoryView runs={runs} onBack={controller.backToOverview} onOpen={selectRun} onLoadMore={() => void controller.loadHistoryMore()} /> : surface === "preparation" && preparation ? <WorkflowPreparationView preparation={preparation} onBack={controller.backToOverview} onPrerequisite={controller.handlePrerequisite} onReprepare={(scope, route) => void controller.prepare(preparation.kind, scope, route)} onStart={(restricted, remote) => void controller.startPrepared(restricted, remote)} /> : surface === "detail" && selectedRun ? <WorkflowTaskDetail run={selectedRun} queuedRuns={queuedRuns} controller={controller} onOpenLogs={onOpenTask} /> : <WorkflowsOverviewView overview={overview} runs={runs} onPrepare={(kind) => void controller.prepare(kind)} onOpenRun={selectRun} />}
  </div>;
}
