export interface LatestLayoutSaveQueue {
  request: (save: () => Promise<void>) => void;
  whenIdle: () => Promise<void>;
}

/**
 * Serialize layout writes and keep only the newest pending snapshot.
 *
 * A save that is already in flight is allowed to finish. If more drag-end
 * events arrive meanwhile, only the latest callback runs next, guaranteeing
 * that an older IPC response can never become the final persisted layout.
 */
export function createLatestLayoutSaveQueue(): LatestLayoutSaveQueue {
  let pending: (() => Promise<void>) | null = null;
  let running: Promise<void> | null = null;

  const start = () => {
    if (running || !pending) return;
    running = drain().finally(() => {
      running = null;
      if (pending) start();
    });
  };

  const drain = async () => {
    while (pending) {
      const save = pending;
      pending = null;
      try {
        await save();
      } catch {
        // Layout persistence is best-effort; keep draining the newest request.
      }
    }
  };

  return {
    request: (save) => {
      pending = save;
      start();
    },
    whenIdle: async () => {
      while (running) await running;
    },
  };
}
