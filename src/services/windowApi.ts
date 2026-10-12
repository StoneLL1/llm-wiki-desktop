import { invoke } from "@tauri-apps/api/core";
import type { BackendTask } from "../types/task";

export interface WindowCloseSnapshot {
  intent: "ask" | "quit" | null;
  activeTasks: BackendTask[];
}

export interface ResolveWindowCloseRequest {
  action: "cancel" | "background" | "quit";
  unsavedChanges: boolean;
  discardUnsavedChanges: boolean;
  editorSaving: boolean;
}

export const getWindowCloseState = (): Promise<WindowCloseSnapshot> =>
  invoke<WindowCloseSnapshot>("get_window_close_state");

export const resolveWindowClose = (request: ResolveWindowCloseRequest): Promise<void> =>
  invoke<void>("resolve_window_close", { request });
