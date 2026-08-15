import { useEffect } from "react";

import { registerTaskEventListener } from "../services/taskEventDispatcher";
import type { BackendEvent, StreamDelta } from "../types/task";
import { useChatStore } from "../stores/chatStore";
import { useProjectStore } from "../stores/projectStore";

/**
 * Bridges the app-owned task dispatcher into the chat presentation store. The
 * dispatcher is fed by useTaskEvents, the only Tauri event subscriber. The store filters by the
 * in-flight `sendTaskId`, so only the current chat generation accumulates; the
 * persisted answer lands via the normal terminal-status reload, making these
 * deltas idempotent UI hints.
 */
export function useChatStream(): void {
  const appendStreamDelta = useChatStore((state) => state.appendStreamDelta);

  useEffect(() => {
    return registerTaskEventListener((rawEvent) => {
      if (rawEvent.eventType !== "task_stream_output") return;
      const event = rawEvent as BackendEvent<StreamDelta>;
      if (!event.taskId) return;
      if (event.projectId !== useProjectStore.getState().currentProject.projectId) return;
      const payload = event.payload;
      if (!payload) return;
      const route = payload.route === "chat-agent" || payload.route === "agent"
        ? "agent"
        : payload.route === "chat-byok" || payload.route === "byok"
          ? "byok"
          : null;
      if (route) appendStreamDelta(event.taskId, payload.delta, route);
    });
  }, [appendStreamDelta]);
}
