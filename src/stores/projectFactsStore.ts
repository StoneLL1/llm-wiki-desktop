import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";

import type { AgentInfo } from "../types/agent";
import type { ProviderStatus } from "../types/llm";

export type ProjectFactKind = "git" | "agents" | "providers";
export type ProjectFactStatus = "idle" | "loading" | "ready" | "stale" | "error";

export interface ProjectFactsScope {
  projectId: string;
  rootPath: string;
}

export interface GitRepositoryStatus {
  isRepository: boolean;
  branch: string | null;
  head: string | null;
  hasChanges: boolean;
}

export interface FactResource<T> {
  value: T | null;
  status: ProjectFactStatus;
  updatedAt: number | null;
  error: unknown | null;
  requestEpoch: number;
}

export interface ProjectFactsEntry {
  key: string;
  authorityIdentityKey: string | null;
  git: FactResource<GitRepositoryStatus>;
  agents: FactResource<AgentInfo[]>;
  providers: FactResource<ProviderStatus[]>;
}

interface ProjectFactsState {
  entries: Record<string, ProjectFactsEntry>;
  accessOrder: string[];
}

interface EnsureProjectFactsOptions {
  forceRefresh?: boolean;
}

interface ResourceControl {
  scope: ProjectFactsScope;
  kind: ProjectFactKind;
  desiredGeneration: number;
  completedGeneration: number;
  activeGeneration: number | null;
  activePromise: Promise<unknown> | null;
  pendingRefresh: boolean;
  pendingForce: boolean;
  retryAt: number | null;
  failureCount: number;
  retireWhenIdle: boolean;
}

const MAX_PROJECT_FACTS_ENTRIES = 3;

export const PROJECT_FACT_TTL_MS: Record<ProjectFactKind, number> = {
  git: 5_000,
  agents: 30_000,
  providers: 30_000,
};

export const PROJECT_FACT_RETRY_DELAYS_MS = [5_000, 30_000, 120_000] as const;

const ALL_FACT_KINDS: readonly ProjectFactKind[] = ["git", "agents", "providers"];
const resourceControls = new Map<string, ResourceControl>();
let activeProjectFactsKey: string | null = null;

declare global {
  interface Window {
    __LLM_WIKI_PROJECT_FACTS_IPC_COUNTS__?: Record<string, number>;
  }
}

const packagedProjectFactsObserverEnabled =
  import.meta.env.VITE_PROJECT_FACTS_PERF_OBSERVER === "1";

if (typeof window !== "undefined" && packagedProjectFactsObserverEnabled) {
  window.__LLM_WIKI_PROJECT_FACTS_IPC_COUNTS__ ??= {};
}

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function emptyResource<T>(): FactResource<T> {
  return {
    value: null,
    status: "idle",
    updatedAt: null,
    error: null,
    requestEpoch: 0,
  };
}

function createEntry(key: string, authorityIdentityKey: string | null = null): ProjectFactsEntry {
  return {
    key,
    authorityIdentityKey,
    git: emptyResource<GitRepositoryStatus>(),
    agents: emptyResource<AgentInfo[]>(),
    providers: emptyResource<ProviderStatus[]>(),
  };
}

export function projectFactsKey(scope: ProjectFactsScope): string {
  return `${scope.projectId}\0${scope.rootPath}`;
}

export function projectFactsAuthorityKey(authority: {
  canonicalIdentityKey: string;
  identityRevision: string;
  authorityRevision: string;
}): string {
  return [
    authority.canonicalIdentityKey,
    authority.identityRevision,
    authority.authorityRevision,
  ].join("\0");
}

function scopeFromKey(entryKey: string): ProjectFactsScope {
  const separator = entryKey.indexOf("\0");
  return {
    projectId: entryKey.slice(0, separator),
    rootPath: entryKey.slice(separator + 1),
  };
}

function requestKey(entryKey: string, kind: ProjectFactKind): string {
  return `${entryKey}\0${kind}`;
}

export const useProjectFactsStore = create<ProjectFactsState>(() => ({
  entries: {},
  accessOrder: [],
}));

function controlsForEntry(entryKey: string): ResourceControl[] {
  return ALL_FACT_KINDS.flatMap((kind) => {
    const control = resourceControls.get(requestKey(entryKey, kind));
    return control ? [control] : [];
  });
}

function deleteIdleControls(entryKey: string): void {
  for (const kind of ALL_FACT_KINDS) {
    const key = requestKey(entryKey, kind);
    const control = resourceControls.get(key);
    if (control && control.activePromise === null) resourceControls.delete(key);
  }
}

function cleanupRetiredEntry(entryKey: string): void {
  if (entryKey === activeProjectFactsKey) return;
  const controls = controlsForEntry(entryKey);
  if (controls.some((control) => control.activePromise !== null)) return;
  if (useProjectFactsStore.getState().accessOrder.includes(entryKey)) return;
  deleteIdleControls(entryKey);
  useProjectFactsStore.setState((state) => {
    if (!state.entries[entryKey]) return state;
    const entries = { ...state.entries };
    delete entries[entryKey];
    return { entries };
  });
}

function retireEntry(entryKey: string): boolean {
  const controls = controlsForEntry(entryKey);
  const active = controls.filter((control) => control.activePromise !== null);
  if (active.length === 0) {
    deleteIdleControls(entryKey);
    return false;
  }
  for (const control of active) {
    control.retireWhenIdle = true;
    control.pendingRefresh = false;
    control.pendingForce = false;
    control.desiredGeneration += 1;
  }
  return true;
}

function touchEntry(scope: ProjectFactsScope): ProjectFactsEntry {
  const key = projectFactsKey(scope);
  const state = useProjectFactsStore.getState();
  const existing = state.entries[key];
  const entry = existing ?? createEntry(key);
  for (const control of controlsForEntry(key)) control.retireWhenIdle = false;
  const accessOrder = [
    ...state.accessOrder.filter((candidate) => candidate !== key),
    key,
  ].slice(-MAX_PROJECT_FACTS_ENTRIES);
  const keep = new Set(accessOrder);
  const sourceEntries = existing ? state.entries : { ...state.entries, [key]: entry };
  const entries = Object.fromEntries(
    Object.entries(sourceEntries).filter(([entryKey]) =>
      keep.has(entryKey) || retireEntry(entryKey)
    ),
  );
  useProjectFactsStore.setState({ entries, accessOrder });
  return existing ?? entry;
}

function updateResource(
  entryKey: string,
  kind: ProjectFactKind,
  update: (resource: FactResource<unknown>) => FactResource<unknown>,
): void {
  useProjectFactsStore.setState((state) => {
    const entry = state.entries[entryKey];
    if (!entry) return state;
    const current = entry[kind] as FactResource<unknown>;
    const resource = update(current);
    if (resource === current) return state;
    return {
      entries: {
        ...state.entries,
        [entryKey]: {
          ...entry,
          [kind]: resource,
        } as ProjectFactsEntry,
      },
    };
  });
}

function isFresh(resource: FactResource<unknown>, kind: ProjectFactKind, now: number): boolean {
  return resource.status === "ready"
    && resource.updatedAt !== null
    && now - resource.updatedAt < PROJECT_FACT_TTL_MS[kind];
}

function invokeFact(
  scope: ProjectFactsScope,
  kind: ProjectFactKind,
  forceRefresh: boolean,
): Promise<unknown> {
  const request = {
    projectId: scope.projectId,
    projectRootPath: scope.rootPath,
    forceRefresh,
  };
  const recordPackagedInvoke = (command: string) => {
    if (!packagedProjectFactsObserverEnabled || typeof window === "undefined") return;
    const counts = window.__LLM_WIKI_PROJECT_FACTS_IPC_COUNTS__ ??= {};
    counts[command] = (counts[command] ?? 0) + 1;
  };
  switch (kind) {
    case "git":
      recordPackagedInvoke("git_status");
      return invoke<GitRepositoryStatus>("git_status", { request });
    case "agents":
      recordPackagedInvoke("detect_agents");
      return invoke<AgentInfo[]>("detect_agents", { request });
    case "providers":
      recordPackagedInvoke("list_llm_providers");
      return invoke<ProviderStatus[]>("list_llm_providers", { request });
  }
}

function getControl(
  scope: ProjectFactsScope,
  kind: ProjectFactKind,
  resource: FactResource<unknown>,
): ResourceControl {
  const key = requestKey(projectFactsKey(scope), kind);
  const existing = resourceControls.get(key);
  if (existing) {
    existing.scope = scope;
    return existing;
  }
  const control: ResourceControl = {
    scope,
    kind,
    desiredGeneration: resource.requestEpoch,
    completedGeneration: resource.requestEpoch,
    activeGeneration: null,
    activePromise: null,
    pendingRefresh: false,
    pendingForce: false,
    retryAt: null,
    failureCount: 0,
    retireWhenIdle: false,
  };
  resourceControls.set(key, control);
  return control;
}

function nextRetryAt(control: ResourceControl): number {
  const delay = PROJECT_FACT_RETRY_DELAYS_MS[
    Math.min(control.failureCount, PROJECT_FACT_RETRY_DELAYS_MS.length - 1)
  ];
  control.failureCount += 1;
  return Date.now() + delay;
}

function startControl(control: ResourceControl): Promise<unknown> {
  const entryKey = projectFactsKey(control.scope);
  const controlKey = requestKey(entryKey, control.kind);
  const run = async (): Promise<unknown> => {
    let lastValue: unknown = null;
    while (!control.retireWhenIdle) {
      const generation = control.desiredGeneration;
      const forceRefresh = control.pendingForce;
      control.pendingForce = false;
      control.pendingRefresh = false;
      control.activeGeneration = generation;
      updateResource(entryKey, control.kind, (current) => ({
        ...current,
        status: current.value === null ? "loading" : "stale",
        error: null,
        requestEpoch: generation,
      }));

      try {
        lastValue = await invokeFact(control.scope, control.kind, forceRefresh);
        const canCommit = !control.retireWhenIdle
          && control.desiredGeneration === generation;
        if (canCommit) {
          updateResource(entryKey, control.kind, (current) => {
            if (current.requestEpoch !== generation) return current;
            return {
              value: lastValue,
              status: "ready",
              updatedAt: Date.now(),
              error: null,
              requestEpoch: generation,
            };
          });
          control.failureCount = 0;
          control.retryAt = null;
        }
      } catch (error: unknown) {
        const canCommit = !control.retireWhenIdle
          && control.desiredGeneration === generation;
        if (canCommit) {
          control.retryAt = nextRetryAt(control);
          updateResource(entryKey, control.kind, (current) => {
            if (current.requestEpoch !== generation) return current;
            return {
              ...current,
              status: "error",
              error,
              requestEpoch: generation,
            };
          });
        }
      } finally {
        control.completedGeneration = Math.max(control.completedGeneration, generation);
        control.activeGeneration = null;
      }

      if (control.retireWhenIdle) break;
      if (!control.pendingForce && !control.pendingRefresh) break;
    }
    return lastValue;
  };

  const promise = run().finally(() => {
    if (control.activePromise !== promise) return;
    control.activePromise = null;
    control.activeGeneration = null;
    if (control.retireWhenIdle) {
      resourceControls.delete(controlKey);
      cleanupRetiredEntry(entryKey);
    }
  });
  control.activePromise = promise;
  return promise;
}

function ensureResource(
  scope: ProjectFactsScope,
  kind: ProjectFactKind,
  forceRefresh: boolean,
): Promise<unknown> {
  const entry = touchEntry(scope);
  const resource = entry[kind];
  const control = getControl(scope, kind, resource);
  const now = Date.now();

  if (!forceRefresh && isFresh(resource, kind, now)) {
    return Promise.resolve(resource.value);
  }
  if (control.activePromise) {
    if (forceRefresh) {
      control.desiredGeneration += 1;
      control.pendingForce = true;
    } else if (control.desiredGeneration !== control.activeGeneration) {
      control.pendingRefresh = true;
    }
    return control.activePromise;
  }
  if (!forceRefresh && control.retryAt !== null && now < control.retryAt) {
    return Promise.resolve(resource.value);
  }

  if (forceRefresh) {
    control.desiredGeneration = Math.max(
      control.desiredGeneration,
      control.completedGeneration,
    ) + 1;
    control.pendingForce = true;
  } else if (control.desiredGeneration <= control.completedGeneration) {
    control.desiredGeneration = control.completedGeneration + 1;
  }
  return startControl(control);
}

export async function ensureProjectFacts(
  scope: ProjectFactsScope,
  kinds: readonly ProjectFactKind[],
  options: EnsureProjectFactsOptions = {},
): Promise<void> {
  if (!hasTauri() || !scope.projectId || !scope.rootPath || kinds.length === 0) return;
  const uniqueKinds = [...new Set(kinds)];
  await Promise.allSettled(
    uniqueKinds.map((kind) => ensureResource(scope, kind, options.forceRefresh ?? false)),
  );
}

export function refreshProjectFacts(
  scope: ProjectFactsScope,
  kinds: readonly ProjectFactKind[] = ALL_FACT_KINDS,
): Promise<void> {
  return ensureProjectFacts(scope, kinds, { forceRefresh: true });
}

function invalidateControl(entryKey: string, kind: ProjectFactKind): number {
  const resource = useProjectFactsStore.getState().entries[entryKey]?.[kind] as
    | FactResource<unknown>
    | undefined;
  if (!resource) return 0;
  const control = getControl(scopeFromKey(entryKey), kind, resource);
  control.desiredGeneration = Math.max(
    control.desiredGeneration,
    control.completedGeneration,
    control.activeGeneration ?? 0,
  ) + 1;
  if (control.activePromise && !control.retireWhenIdle) control.pendingRefresh = true;
  return control.desiredGeneration;
}

export function invalidateProjectFacts(
  scope: ProjectFactsScope,
  kinds: readonly ProjectFactKind[] = ALL_FACT_KINDS,
  _reason = "explicit",
): void {
  invalidateProjectFactEntries([projectFactsKey(scope)], kinds);
}

export function invalidateAllProjectFacts(
  kinds: readonly ProjectFactKind[] = ALL_FACT_KINDS,
  _reason = "explicit",
): void {
  invalidateProjectFactEntries(Object.keys(useProjectFactsStore.getState().entries), kinds);
}

function invalidateProjectFactEntries(
  entryKeys: readonly string[],
  kinds: readonly ProjectFactKind[],
): void {
  const uniqueKinds = [...new Set(kinds)];
  const generations = new Map<string, number>();
  for (const entryKey of entryKeys) {
    for (const kind of uniqueKinds) {
      generations.set(requestKey(entryKey, kind), invalidateControl(entryKey, kind));
    }
  }
  useProjectFactsStore.setState((state) => {
    const entries = { ...state.entries };
    let changed = false;
    for (const entryKey of entryKeys) {
      const entry = entries[entryKey];
      if (!entry) continue;
      const next = { ...entry };
      for (const kind of uniqueKinds) {
        const current = next[kind] as FactResource<unknown>;
        next[kind] = {
          ...current,
          status: current.value === null ? "idle" : "stale",
          error: null,
          requestEpoch: generations.get(requestKey(entryKey, kind)) ?? current.requestEpoch,
        } as never;
      }
      entries[entryKey] = next;
      changed = true;
    }
    return changed ? { entries } : state;
  });
}

function supersedeActiveEntry(entryKey: string): void {
  const activeKinds: ProjectFactKind[] = [];
  for (const control of controlsForEntry(entryKey)) {
    if (!control.activePromise) continue;
    control.desiredGeneration += 1;
    control.pendingRefresh = false;
    control.pendingForce = false;
    activeKinds.push(control.kind);
  }
  if (activeKinds.length === 0) return;
  useProjectFactsStore.setState((state) => {
    const entry = state.entries[entryKey];
    if (!entry) return state;
    const next = { ...entry };
    for (const kind of activeKinds) {
      const current = next[kind] as FactResource<unknown>;
      const generation = resourceControls.get(requestKey(entryKey, kind))?.desiredGeneration
        ?? current.requestEpoch + 1;
      next[kind] = {
        ...current,
        status: current.value === null ? "idle" : "stale",
        requestEpoch: generation,
      } as never;
    }
    return { entries: { ...state.entries, [entryKey]: next } };
  });
}

export function bindProjectFactsAuthority(
  scope: ProjectFactsScope,
  authorityIdentityKey: string | null,
): void {
  const key = projectFactsKey(scope);
  const entry = touchEntry(scope);
  if (entry.authorityIdentityKey === authorityIdentityKey) return;

  for (const kind of ALL_FACT_KINDS) {
    const control = resourceControls.get(requestKey(key, kind));
    if (!control) continue;
    control.desiredGeneration += 1;
    if (control.activePromise) control.pendingRefresh = true;
    control.retryAt = null;
    control.failureCount = 0;
  }
  useProjectFactsStore.setState((state) => ({
    entries: {
      ...state.entries,
      [key]: createEntry(key, authorityIdentityKey),
    },
  }));
}

export function projectFactsAuthorityMatches(
  scope: ProjectFactsScope,
  authorityIdentityKey: string,
): boolean {
  const entry = useProjectFactsStore.getState().entries[projectFactsKey(scope)];
  return entry?.authorityIdentityKey === authorityIdentityKey;
}

export function pruneProjectFacts(activeKey: string | null): void {
  if (activeProjectFactsKey !== activeKey) {
    if (activeProjectFactsKey) supersedeActiveEntry(activeProjectFactsKey);
    activeProjectFactsKey = activeKey;
  }
  const state = useProjectFactsStore.getState();
  if (!activeKey) {
    const entries: Record<string, ProjectFactsEntry> = {};
    for (const [key, entry] of Object.entries(state.entries)) {
      if (retireEntry(key)) entries[key] = entry;
    }
    useProjectFactsStore.setState({ entries, accessOrder: [] });
    return;
  }
  const ordered = [
    ...state.accessOrder.filter((key) => key !== activeKey && state.entries[key]),
    ...(state.entries[activeKey] ? [activeKey] : []),
  ];
  const accessOrder = ordered.slice(-MAX_PROJECT_FACTS_ENTRIES);
  const keep = new Set(accessOrder);
  const entries = Object.fromEntries(
    Object.entries(state.entries).filter(([key]) => keep.has(key) || retireEntry(key)),
  );
  useProjectFactsStore.setState({ entries, accessOrder });
}

export function resetProjectFactsStoreForTests(): void {
  resourceControls.clear();
  activeProjectFactsKey = null;
  useProjectFactsStore.setState({ entries: {}, accessOrder: [] });
}
