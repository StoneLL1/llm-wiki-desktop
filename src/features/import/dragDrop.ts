import type { DragDropEvent } from "@tauri-apps/api/webview";

export interface DragDropUpdate {
  active: boolean;
  paths: string[] | null;
}

export function reduceDragDrop(payload: DragDropEvent): DragDropUpdate {
  switch (payload.type) {
    case "enter":
    case "over":
      return { active: true, paths: null };
    case "drop":
      return {
        active: false,
        paths: payload.paths.length > 0 ? payload.paths : null,
      };
    case "leave":
      return { active: false, paths: null };
  }
}
