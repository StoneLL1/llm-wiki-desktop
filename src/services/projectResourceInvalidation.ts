import type { BackendEvent, BackendTask } from "../types/task";
import type { ProjectResourceKind } from "../lib/projectResourceFreshness";

export function projectResourcesForBackendEvent(event: BackendEvent): ProjectResourceKind[] {
  if (event.eventType === "wiki_changed") return ["wiki", "graph"];
  if (event.eventType === "graph_updated") return ["graph"];
  if (![
    "task_completed",
    "task_failed",
    "task_cancelled",
  ].includes(event.eventType)) return [];

  const task = asBackendTask(event.payload);
  switch (task?.taskType) {
    case "export":
      return ["exports"];
    case "deep_lint":
    case "auto_fix":
      return ["lint-history", "lint-ignores"];
    case "llm_request":
    case "agent_run":
      return ["chat-sessions"];
    case "import":
    case "wiki_compile":
    case "source_ai_organize":
    case "workflow":
      return ["wiki", "graph"];
    default:
      return [];
  }
}

export function gitFactsChangedForBackendEvent(event: BackendEvent): boolean {
  if (![
    "task_completed",
    "task_failed",
    "task_cancelled",
  ].includes(event.eventType)) return false;
  const task = asBackendTask(event.payload);
  return (task?.result?.affectedPaths.length ?? 0) > 0;
}

function asBackendTask(value: unknown): BackendTask | null {
  if (!value || typeof value !== "object" || !("taskType" in value)) return null;
  return value as BackendTask;
}
