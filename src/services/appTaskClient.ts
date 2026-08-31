import { invoke } from "@tauri-apps/api/core";
import type { BackendTask, LogLine, TaskActivity } from "../types/task";

interface AppTaskRequest {
  taskId: string;
  taskRevision: string;
  scope: "app_global";
}

function requestFor(task: BackendTask): AppTaskRequest {
  return {
    taskId: task.id,
    taskRevision: task.updatedAt,
    scope: "app_global",
  };
}

export function listAppTasks(): Promise<BackendTask[]> {
  return invoke<BackendTask[]>("list_app_tasks_v1");
}

export function cancelAppCapabilityTask(task: BackendTask): Promise<BackendTask> {
  return invoke<BackendTask>("cancel_app_capability_install_v1", {
    request: requestFor(task),
  });
}

export function getAppCapabilityTaskLogs(task: BackendTask): Promise<LogLine[]> {
  return invoke<LogLine[]>("get_app_capability_task_logs_v1", {
    request: requestFor(task),
  });
}

export function getAppCapabilityTaskActivities(task: BackendTask): Promise<TaskActivity[]> {
  return invoke<TaskActivity[]>("get_app_capability_task_activities_v1", {
    request: requestFor(task),
  });
}
