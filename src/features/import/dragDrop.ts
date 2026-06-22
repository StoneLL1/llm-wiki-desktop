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

interface DragDropEnvelope {
  payload: DragDropEvent;
}

interface DragDropSubscription {
  listen: (handler: (event: DragDropEnvelope) => void) => Promise<() => void>;
  isCancelled: () => boolean;
  onActive: (active: boolean) => void;
  onPaths: (paths: string[]) => void;
}

export async function subscribeToDragDrop({
  listen,
  isCancelled,
  onActive,
  onPaths,
}: DragDropSubscription): Promise<() => void> {
  const unlisten = await listen((event) => {
    const update = reduceDragDrop(event.payload);
    onActive(update.active);
    if (update.paths) onPaths(update.paths);
  });
  if (isCancelled()) unlisten();
  return unlisten;
}
