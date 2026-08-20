import { useEffect } from "react";

import { updateCheckDue, useUpdateStore } from "../../stores/updateStore";

const AUTOMATIC_CHECK_DELAY_MS = 750;

export function useUpdateController() {
  const initialize = useUpdateStore((state) => state.initialize);

  useEffect(() => {
    let active = true;
    let timer: number | null = null;

    void initialize().then(() => {
      if (!active) return;
      const state = useUpdateStore.getState();
      if (!updateCheckDue(state.preferences)) return;
      timer = window.setTimeout(() => {
        if (!active) return;
        void useUpdateStore.getState().checkNow().catch(() => undefined);
      }, AUTOMATIC_CHECK_DELAY_MS);
    });

    return () => {
      active = false;
      if (timer !== null) window.clearTimeout(timer);
    };
  }, [initialize]);
}
