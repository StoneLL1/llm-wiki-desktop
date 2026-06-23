import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";

import type { BackendEvent } from "../types/task";
import { useChatStore } from "../stores/chatStore";

interface StreamDelta {
  delta: string;
  route?: string;
}

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/**
 * Subscribes to the backend `task://stream-output` channel for the lifetime of
 * the app and forwards each delta into the chat store. The store filters by the
 * in-flight `sendTaskId`, so only the current chat generation accumulates; the
 * persisted answer lands via the normal terminal-status reload, making these
 * deltas idempotent UI hints.
 */
export function useChatStream(): void {
  const appendStreamDelta = useChatStore((state) => state.appendStreamDelta);

  useEffect(() => {
    if (!hasTauri()) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    listen<BackendEvent<StreamDelta>>("task://stream-output", (evt) => {
      const event = evt.payload as BackendEvent<StreamDelta>;
      if (!event.taskId) return;
      const payload = event.payload as StreamDelta | undefined;
      if (!payload) return;
      const route = payload.route === "agent" || payload.route === "byok" ? payload.route : null;
      appendStreamDelta(event.taskId, payload.delta, route);
    })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {
        // Tauri event system unavailable (browser-only dev).
      });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [appendStreamDelta]);
}
