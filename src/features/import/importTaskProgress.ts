import type { ImportItem, ImportItemStatus } from "../../types/importV2";
import type { BackendTask } from "../../types/task";

const ACTIVE_ITEM_STATUSES = new Set<ImportItemStatus>([
  "queued",
  "inspecting",
  "extracting",
  "validating",
]);

function activeStatusForTask(item: ImportItem, task: BackendTask): ImportItemStatus {
  if (!["running", "cancelling"].includes(task.status)) return item.status;
  const label = task.progress?.label;
  if (label === "Inspecting input") return "inspecting";
  if (label === "Validating preview" || label === "Preview ready") return "validating";
  if (item.status === "validating") return item.status;
  return label?.startsWith("asr.")
    || label === "media.downloading"
    || label === "Extracting source"
    || item.status === "extracting"
    ? "extracting"
    : "inspecting";
}

export function mergeImportItemTask(
  item: ImportItem,
  task: BackendTask,
  allowBinding = false,
): ImportItem {
  if (item.taskId !== task.id && !allowBinding) return item;
  const status = ACTIVE_ITEM_STATUSES.has(item.status) || allowBinding
    ? activeStatusForTask(
      allowBinding && !ACTIVE_ITEM_STATUSES.has(item.status) ? { ...item, status: "queued" } : item,
      task,
    )
    : item.status;
  const progress = allowBinding && item.taskId !== task.id
    ? task.progress
    : task.progress ?? item.progress;
  if (item.taskId === task.id && item.status === status && item.progress === progress) return item;
  return {
    ...item,
    taskId: task.id,
    status,
    progress,
  };
}
