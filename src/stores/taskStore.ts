import { create } from "zustand";

export type TaskStatus = "running" | "queued" | "succeeded" | "failed" | "idle";

export interface TaskSummary {
  id: string;
  title: string;
  status: TaskStatus;
}

interface TaskState {
  tasks: TaskSummary[];
  runningCount: number;
  setTasks: (tasks: TaskSummary[]) => void;
}

export const defaultTasks: TaskSummary[] = [
  {
    id: "task-graph-refresh",
    title: "Refreshing graph cache",
    status: "running",
  },
];

const countRunning = (tasks: TaskSummary[]) => tasks.filter((task) => task.status === "running").length;

export const useTaskStore = create<TaskState>((set) => ({
  tasks: defaultTasks,
  runningCount: countRunning(defaultTasks),
  setTasks: (tasks) => set({ tasks, runningCount: countRunning(tasks) }),
}));
