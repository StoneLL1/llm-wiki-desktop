import type { ProfilerOnRenderCallback } from "react";

export interface PublicationObserver {
  readonly publications: number;
  stop: () => void;
}

export function observeStorePublications(subscribe: (listener: () => void) => () => void): PublicationObserver {
  let publications = 0;
  const unsubscribe = subscribe(() => { publications += 1; });
  return {
    get publications() { return publications; },
    stop: unsubscribe,
  };
}

export interface ReactCommitObserver {
  readonly commits: number;
  readonly phases: readonly ("mount" | "update" | "nested-update")[];
  onRender: ProfilerOnRenderCallback;
}

export function createReactCommitObserver(): ReactCommitObserver {
  let commits = 0;
  const phases: ("mount" | "update" | "nested-update")[] = [];
  return {
    get commits() { return commits; },
    get phases() { return phases; },
    onRender: (_id, phase) => {
      commits += 1;
      phases.push(phase);
    },
  };
}
