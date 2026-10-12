import { useSyncExternalStore } from "react";

// Only dirty flags leave the form. Provider credentials remain in component memory.
const owners = new Set<string>();
const listeners = new Set<() => void>();

export function setSettingsDraftDirty(owner: string, dirty: boolean): void {
  const previous = owners.has(owner);
  if (dirty) owners.add(owner);
  else owners.delete(owner);
  if (previous !== dirty) listeners.forEach((listener) => listener());
}

export const getSettingsDraftDirty = (): boolean => owners.size > 0;

export function useSettingsDraftDirty(): boolean {
  return useSyncExternalStore((listener) => {
    listeners.add(listener);
    return () => { listeners.delete(listener); };
  }, getSettingsDraftDirty, () => false);
}
