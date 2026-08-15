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

interface InFlightRequest {
  forceRefresh: boolean;
  promise: Promise<unknown>;
}

const MAX_PROJECT_FACTS_ENTRIES = 3;

export const PROJECT_FACT_TTL_MS: Record<ProjectFactKind, number> = {
  git: 5_000,
  agents: 30_000,
  providers: 30_000,
};

const ALL_FACT_KINDS: readonly ProjectFactKind[] = ["git", "agents", "providers"];
const inFlightRequests = new Map<string, InFlightRequest>();
let nextRequestEpoch = 0;
let activeProjectFactsKey: string | null = null;

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

function requestKey(entryKey: string, kind: ProjectFactKind): string {
  return `${entryKey}\0${kind}`;
}

export const useProjectFactsStore = create<ProjectFactsState>(() => ({
  entries: {},
  accessOrder: [],
}));

function touchEntry(scope: ProjectFactsScope): ProjectFactsEntry {
  const key = projectFactsKey(scope);
  const state = useProjectFactsStore.getState();
  const existing = state.entries[key];
  const entry = existing ?? createEntry(key);
  const accessOrder = [
    ...state.accessOrder.filter((candidate) => candidate !== key),
    key,
  ].slice(-MAX_PROJECT_FACTS_ENTRIES);
  const keep = new Set(accessOrder);
  const entries = Object.fromEntries(
    Object.entries(existing ? state.entries : { ...state.entries, [key]: entry })
      .filter(([entryKey]) => keep.has(entryKey)),
  );
  for (const entryKey of Object.keys(state.entries)) {
    if (!keep.has(entryKey)) discardInFlightForEntry(entryKey);
  }
  useProjectFactsStore.setState({
    entries,
    accessOrder,
  });
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
    return {
      entries: {
        ...state.entries,
        [entryKey]: {
          ...entry,
          [kind]: update(entry[kind] as FactResource<unknown>),
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

export function nextProjectFactsExpiryAt(
  entry: ProjectFactsEntry | null,
  kinds: readonly ProjectFactKind[],
): number | null {
  if (!entry) return null;
  const expiries = [...new Set(kinds)].flatMap((kind) => {
    const resource = entry[kind];
    if (resource.status !== "ready" || resource.updatedAt === null) return [];
    return [resource.updatedAt + PROJECT_FACT_TTL_MS[kind]];
  });
  return expiries.length > 0 ? Math.min(...expiries) : null;
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
  switch (kind) {
    case "git":
      return invoke<GitRepositoryStatus>("git_status", { request });
    case "agents":
      return invoke<AgentInfo[]>("detect_agents", { request });
    case "providers":
      return invoke<ProviderStatus[]>("list_llm_providers", { request });
  }
}

function ensureResource(
  scope: ProjectFactsScope,
  kind: ProjectFactKind,
  forceRefresh: boolean,
): Promise<unknown> {
  const entry = touchEntry(scope);
  const key = entry.key;
  const resource = entry[kind];
  const inFlightKey = requestKey(key, kind);
  const currentRequest = inFlightRequests.get(inFlightKey);

  if (!forceRefresh && isFresh(resource, kind, Date.now())) {
    return Promise.resolve(resource.value);
  }
  if (currentRequest && !forceRefresh) {
    return currentRequest.promise;
  }

  const requestEpoch = ++nextRequestEpoch;
  updateResource(key, kind, (current) => ({
    ...current,
    status: current.value === null ? "loading" : "stale",
    error: null,
    requestEpoch,
  }));

  const promise = invokeFact(scope, kind, forceRefresh).then(
    (value) => {
      updateResource(key, kind, (current) => {
        if (current.requestEpoch !== requestEpoch) return current;
        return {
          value,
          status: "ready",
          updatedAt: Date.now(),
          error: null,
          requestEpoch,
        };
      });
      return value;
    },
    (error: unknown) => {
      updateResource(key, kind, (current) => {
        if (current.requestEpoch !== requestEpoch) return current;
        return {
          ...current,
          status: "error",
          error,
          requestEpoch,
        };
      });
      throw error;
    },
  );
  const request: InFlightRequest = { forceRefresh, promise };
  inFlightRequests.set(inFlightKey, request);
  void promise.finally(() => {
    if (inFlightRequests.get(inFlightKey) === request) {
      inFlightRequests.delete(inFlightKey);
    }
  }).catch(() => undefined);
  return promise;
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
  for (const entryKey of entryKeys) {
    for (const kind of uniqueKinds) {
      inFlightRequests.delete(requestKey(entryKey, kind));
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
          requestEpoch: ++nextRequestEpoch,
        } as never;
      }
      entries[entryKey] = next;
      changed = true;
    }
    return changed ? { entries } : state;
  });
}

function discardInFlightForEntry(entryKey: string): void {
  for (const kind of ALL_FACT_KINDS) {
    inFlightRequests.delete(requestKey(entryKey, kind));
  }
}

function deactivateProjectFactsEntry(entryKey: string): void {
  const pendingKinds = ALL_FACT_KINDS.filter((kind) =>
    inFlightRequests.has(requestKey(entryKey, kind))
  );
  if (pendingKinds.length === 0) return;
  discardInFlightForEntry(entryKey);
  useProjectFactsStore.setState((state) => {
    const entry = state.entries[entryKey];
    if (!entry) return state;
    const next = { ...entry };
    for (const kind of pendingKinds) {
      const current = next[kind] as FactResource<unknown>;
      next[kind] = {
        ...current,
        status: current.value === null ? "idle" : "stale",
        requestEpoch: ++nextRequestEpoch,
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
  if (entry.authorityIdentityKey === null && authorityIdentityKey !== null) {
    useProjectFactsStore.setState((state) => ({
      entries: {
        ...state.entries,
        [key]: { ...state.entries[key], authorityIdentityKey },
      },
    }));
    return;
  }

  discardInFlightForEntry(key);
  useProjectFactsStore.setState((state) => ({
    entries: {
      ...state.entries,
      [key]: createEntry(key, authorityIdentityKey),
    },
  }));
}

export function pruneProjectFacts(activeKey: string | null): void {
  if (activeProjectFactsKey !== activeKey) {
    if (activeProjectFactsKey) deactivateProjectFactsEntry(activeProjectFactsKey);
    activeProjectFactsKey = activeKey;
  }
  const state = useProjectFactsStore.getState();
  if (!activeKey) {
    for (const key of Object.keys(state.entries)) discardInFlightForEntry(key);
    useProjectFactsStore.setState({ entries: {}, accessOrder: [] });
    return;
  }
  const ordered = [
    ...state.accessOrder.filter((key) => key !== activeKey && state.entries[key]),
    ...(state.entries[activeKey] ? [activeKey] : []),
  ];
  const keep = new Set(ordered.slice(-MAX_PROJECT_FACTS_ENTRIES));
  const entries = Object.fromEntries(
    Object.entries(state.entries).filter(([key]) => keep.has(key)),
  );
  for (const key of Object.keys(state.entries)) {
    if (!keep.has(key)) discardInFlightForEntry(key);
  }
  useProjectFactsStore.setState({ entries, accessOrder: ordered.filter((key) => keep.has(key)) });
}

export function resetProjectFactsStoreForTests(): void {
  inFlightRequests.clear();
  activeProjectFactsKey = null;
  useProjectFactsStore.setState({ entries: {}, accessOrder: [] });
}
