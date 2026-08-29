import { invoke } from "@tauri-apps/api/core";
import type { BackendTask, SetActiveProjectResult } from "../types/task";
import { useProjectStore } from "../stores/projectStore";
import { useTaskStore } from "../stores/taskStore";
import { listAppTasks } from "./appTaskClient";

let recoveryEpoch = 0;

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export async function recoverTasksForProject(projectId: string, rootPath: string): Promise<void> {
  if (!hasTauri()) return;
  const recoveryId = ++recoveryEpoch;
  useTaskStore.setState({
    activeProjectId: projectId,
    activeProjectRootPath: rootPath,
    tasks: [],
    logs: {},
    activities: {},
    taskOutputs: {},
    selectedTaskId: null,
    drawerOpen: false,
    runningCount: 0,
    tasksHydrated: false,
    projectPersistence: null,
    projectPersistenceReason: null,
  });
  useProjectStore.getState().setTaskPersistence(projectId, rootPath, null, null);
  try {
    const result = await invoke<SetActiveProjectResult>("set_active_project", {
      request: { projectId, rootPath },
    });
    const state = useTaskStore.getState();
    if (
      recoveryId !== recoveryEpoch ||
      state.activeProjectId !== projectId ||
      state.activeProjectRootPath !== rootPath
    ) return;
    useTaskStore.setState({
      projectPersistence: result.persistence,
      projectPersistenceReason: result.persistenceReason ?? null,
    });
    useProjectStore.getState().setTaskPersistence(
      projectId,
      rootPath,
      result.persistence,
      result.persistenceReason ?? null,
    );
    let globalTasks: BackendTask[] = [];
    try {
      const listed = await listAppTasks();
      globalTasks = Array.isArray(listed) ? listed : [];
    } catch {
      globalTasks = [];
    }
    const latest = useTaskStore.getState();
    if (
      recoveryId !== recoveryEpoch ||
      latest.activeProjectId !== projectId ||
      latest.activeProjectRootPath !== rootPath
    ) return;
    latest.setTasks([...globalTasks, ...result.tasks]);
  } finally {
    // Unknown task cards are only dismissible after the project task registry
    // has had a chance to hydrate; otherwise a restart race can hide a live
    // batch before its task snapshot arrives.
    const state = useTaskStore.getState();
    if (
      recoveryId === recoveryEpoch &&
      state.activeProjectId === projectId &&
      state.activeProjectRootPath === rootPath
    ) {
      useTaskStore.setState({ tasksHydrated: true });
    }
  }
}

export async function recoverAppTasks(): Promise<void> {
  if (!hasTauri()) return;
  const recoveryId = ++recoveryEpoch;
  const before = useTaskStore.getState();
  const activeProjectId = before.activeProjectId;
  const activeProjectRootPath = before.activeProjectRootPath;
  const globalTasks = await listAppTasks();
  const state = useTaskStore.getState();
  if (
    recoveryId !== recoveryEpoch ||
    state.activeProjectId !== activeProjectId ||
    state.activeProjectRootPath !== activeProjectRootPath
  ) return;
  const projectTasks = state.tasks.filter((task) => task.projectId === activeProjectId);
  state.setTasks([...(Array.isArray(globalTasks) ? globalTasks : []), ...projectTasks]);
}
