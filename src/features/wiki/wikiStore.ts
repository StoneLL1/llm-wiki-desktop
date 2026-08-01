import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";
import { captureProjectScope, isProjectScopeCurrent } from "../../stores/projectScope";
import type { ConfirmedAction, PendingAction } from "../../types/backend";

import type {
  CreateWikiPageInput,
  RenameWikiPageResponse,
  SaveWikiPageResponse,
  WikiPageContent,
  WikiTree,
  WikiTreeNode,
} from "../../types/wiki";

export type WikiMode = "read" | "edit" | "preview";
export type SaveState = "idle" | "saving" | "saved" | "conflict" | "error";

export interface RecentPageEntry {
  path: string;
  title: string;
}

export interface WikiSaveConflict {
  path: string;
  originalContent: string;
  currentContent: string;
  incomingContent: string;
  currentHash: string;
}

interface BackendLikeError {
  code?: string;
  message?: string;
  details?: {
    baselineContent?: string;
    currentHash?: string;
  };
}

function isConflictError(error: unknown): boolean {
  const code = (error as BackendLikeError | null | undefined)?.code;
  return code === "FILE_HASH_MISMATCH";
}

const RECENT_PAGE_LIMIT = 8;
// Project scope rejects cross-project work; this serial also rejects older
// requests inside the same project, including A -> B -> A navigation.
let pageRequestEpoch = 0;

interface WikiState {
  tree: WikiTree | null;
  selectedPath: string | null;
  page: WikiPageContent | null;
  mode: WikiMode;
  saveState: SaveState;
  conflict: WikiSaveConflict | null;
  /** Live editor contents (raw markdown including frontmatter). */
  draft: string;
  loadingTree: boolean;
  loadingPage: boolean;
  error: string | null;
  recentPages: RecentPageEntry[];
  scan: (projectId: string, rootPath: string) => Promise<void>;
  openPage: (projectId: string, rootPath: string, path: string) => Promise<void>;
  setMode: (mode: WikiMode) => void;
  startEdit: () => void;
  cancelEdit: () => void;
  setDraft: (draft: string) => void;
  save: (projectId: string, rootPath: string) => Promise<void>;
  resolveConflict: (
    projectId: string,
    rootPath: string,
    resolution: "keep_current" | "use_incoming" | "manual_merge",
    manualContent?: string,
  ) => Promise<void>;
  reload: (projectId: string, rootPath: string) => Promise<void>;
  toggleBookmark: (projectId: string, rootPath: string) => Promise<void>;
  createPage: (
    projectId: string,
    rootPath: string,
    input: CreateWikiPageInput,
  ) => Promise<void>;
  renamePage: (
    projectId: string,
    rootPath: string,
    relativePath: string,
    newRelativePath: string,
  ) => Promise<void>;
  requestDeletePage: (
    projectId: string,
    rootPath: string,
    relativePath: string,
  ) => Promise<PendingAction | null>;
  confirmDeletePage: (
    projectId: string,
    rootPath: string,
    action: PendingAction,
  ) => Promise<void>;
  cancelPendingAction: (action: PendingAction) => Promise<void>;
  reset: () => void;
}

const initial = {
  tree: null as WikiTree | null,
  selectedPath: null as string | null,
  page: null as WikiPageContent | null,
  mode: "read" as WikiMode,
  saveState: "idle" as SaveState,
  conflict: null as WikiSaveConflict | null,
  draft: "",
  loadingTree: false,
  loadingPage: false,
  error: null as string | null,
  recentPages: [] as RecentPageEntry[],
};

export function updateTreeNodeBookmark(
  node: WikiTreeNode,
  path: string,
  bookmarked: boolean,
): WikiTreeNode {
  if (node.kind === "file" && node.path === path) {
    return { ...node, bookmarked };
  }
  if (node.children.length === 0) return node;
  return {
    ...node,
    children: node.children.map((child) =>
      updateTreeNodeBookmark(child, path, bookmarked),
    ),
  };
}

export const useWikiStore = create<WikiState>((set, get) => ({
  ...initial,
  scan: async (projectId, rootPath) => {
    const scope = captureProjectScope();
    set({ loadingTree: true, error: null });
    try {
      const tree = await invoke<WikiTree>("scan_wiki", {
        request: { projectId, projectRootPath: rootPath },
      });
      if (!isProjectScopeCurrent(scope)) return;
      const firstPage = tree.pages[0]?.path ?? null;
      set({ tree, loadingTree: false });
      if (firstPage && !get().selectedPath) {
        await get().openPage(projectId, rootPath, firstPage);
      }
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      set({ loadingTree: false, error: errorMessage(error) });
    }
  },
  openPage: async (projectId, rootPath, path) => {
    const scope = captureProjectScope();
    const requestEpoch = ++pageRequestEpoch;
    set({ loadingPage: true, selectedPath: path, mode: "read", saveState: "idle", conflict: null, error: null });
    try {
      const page = await invoke<WikiPageContent>("read_wiki_page", {
        request: { projectId, projectRootPath: rootPath, relativePath: path },
      });
      if (!isProjectScopeCurrent(scope) || requestEpoch !== pageRequestEpoch) return;
      set((state) => {
        const entry: RecentPageEntry = { path: page.meta.path, title: page.meta.title };
        const rest = state.recentPages.filter((p) => p.path !== entry.path);
        return {
          page,
          draft: page.rawMarkdown,
          loadingPage: false,
          recentPages: [entry, ...rest].slice(0, RECENT_PAGE_LIMIT),
        };
      });
    } catch (error) {
      if (!isProjectScopeCurrent(scope) || requestEpoch !== pageRequestEpoch) return;
      set({ loadingPage: false, error: errorMessage(error) });
    }
  },
  setMode: (mode) => set({ mode }),
  startEdit: () => {
    const page = get().page;
    if (page) set({ mode: "edit", draft: page.rawMarkdown, saveState: "idle", conflict: null });
  },
  cancelEdit: () => {
    const page = get().page;
    set({
      mode: "read",
      draft: page?.rawMarkdown ?? "",
      saveState: "idle",
      conflict: null,
    });
  },
  setDraft: (draft) => set({ draft, saveState: "idle" }),
  save: async (projectId, rootPath) => {
    const scope = captureProjectScope();
    const { page, draft } = get();
    if (!page) return;
    const savedPath = page.meta.path;
    set({ saveState: "saving" });
    try {
      const response = await invoke<SaveWikiPageResponse>("save_wiki_page", {
        request: {
          projectId,
          projectRootPath: rootPath,
          relativePath: savedPath,
          contents: draft,
          expectedHash: page.meta.hash,
        },
      });
      if (!isProjectScopeCurrent(scope)) return;
      // The write succeeded — re-read to reflect saved bytes, but don't let a
      // transient re-read failure mask the successful save. Skip applying only
      // if the user navigated to a different page mid-save.
      const stillViewing = (current: string | null) => current === null || current === savedPath;
      try {
        const refreshed = await invoke<WikiPageContent>("read_wiki_page", {
          request: {
            projectId,
            projectRootPath: rootPath,
            relativePath: response.relativePath,
          },
        });
        if (!isProjectScopeCurrent(scope)) return;
        if (stillViewing(get().selectedPath)) {
          set((state) => ({
            page: refreshed,
            draft: refreshed.rawMarkdown,
            mode: "read",
            saveState: "saved",
            // Keep the tree's flat page list fresh so the file list, type
            // filter, and computed backlinks reflect the saved title/tags.
            tree: state.tree
              ? {
                  ...state.tree,
                  pages: state.tree.pages.map((p) =>
                    p.path === savedPath ? refreshed.meta : p,
                  ),
                }
              : state.tree,
          }));
        }
      } catch {
        if (!isProjectScopeCurrent(scope)) return;
        if (stillViewing(get().selectedPath)) {
          set({ saveState: "saved" });
        }
      }
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      if (isConflictError(error)) {
        const details = (error as BackendLikeError).details;
        set({
          saveState: "conflict",
          conflict:
            details?.baselineContent != null && details.currentHash
              ? {
                  path: savedPath,
                  originalContent: page.rawMarkdown,
                  currentContent: details.baselineContent,
                  incomingContent: draft,
                  currentHash: details.currentHash,
                }
              : null,
        });
      } else {
        set({ saveState: "error", error: errorMessage(error) });
      }
    }
  },
  resolveConflict: async (projectId, rootPath, resolution, manualContent) => {
    const scope = captureProjectScope();
    const conflict = get().conflict;
    if (!conflict) return;

    if (resolution === "keep_current") {
      await get().openPage(projectId, rootPath, conflict.path);
      return;
    }

    const contents =
      resolution === "manual_merge" ? (manualContent ?? "") : conflict.incomingContent;
    set({ saveState: "saving", error: null });
    try {
      await invoke("create_git_checkpoint", {
        request: {
          projectId,
          projectRootPath: rootPath,
          purpose: "high_risk_operation",
          message: `Before resolving wiki conflict: ${conflict.path}`,
        },
      });
      if (!isProjectScopeCurrent(scope)) return;
      const response = await invoke<SaveWikiPageResponse>("save_wiki_page", {
        request: {
          projectId,
          projectRootPath: rootPath,
          relativePath: conflict.path,
          contents,
          expectedHash: conflict.currentHash,
        },
      });
      if (!isProjectScopeCurrent(scope)) return;
      const refreshed = await invoke<WikiPageContent>("read_wiki_page", {
        request: {
          projectId,
          projectRootPath: rootPath,
          relativePath: response.relativePath,
        },
      });
      if (!isProjectScopeCurrent(scope)) return;
      set((state) => ({
        page: refreshed,
        draft: refreshed.rawMarkdown,
        mode: "read",
        saveState: "saved",
        conflict: null,
        tree: state.tree
          ? {
              ...state.tree,
              pages: state.tree.pages.map((item) =>
                item.path === refreshed.meta.path ? refreshed.meta : item,
              ),
            }
          : state.tree,
      }));
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      if (isConflictError(error)) {
        const details = (error as BackendLikeError).details;
        set({
          saveState: "conflict",
          conflict:
            details?.baselineContent != null && details.currentHash
              ? {
                  ...conflict,
                  currentContent: details.baselineContent,
                  incomingContent: contents,
                  currentHash: details.currentHash,
                }
              : conflict,
        });
      } else {
        set({ saveState: "error", error: errorMessage(error) });
      }
    }
  },
  reload: async (projectId, rootPath) => {
    const selected = get().selectedPath;
    await get().scan(projectId, rootPath);
    if (selected) {
      await get().openPage(projectId, rootPath, selected);
    }
  },
  toggleBookmark: async (projectId, rootPath) => {
    const scope = captureProjectScope();
    const { page } = get();
    if (!page) return;
    try {
      const result = await invoke<{ relativePath: string; bookmarked: boolean }>(
        "toggle_bookmark",
        { request: { projectId, projectRootPath: rootPath, relativePath: page.meta.path } },
      );
      if (!isProjectScopeCurrent(scope)) return;
      set((state) => {
        const nextPage =
          state.page?.meta.path === result.relativePath
            ? {
                ...state.page,
                meta: { ...state.page.meta, bookmarked: result.bookmarked },
              }
            : state.page;
        return {
          page: nextPage,
          tree: state.tree
            ? {
                ...state.tree,
                root: updateTreeNodeBookmark(
                  state.tree.root,
                  result.relativePath,
                  result.bookmarked,
                ),
                pages: state.tree.pages.map((p) =>
                  p.path === result.relativePath ? { ...p, bookmarked: result.bookmarked } : p,
                ),
              }
            : state.tree,
        };
      });
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      set({ error: errorMessage(error) });
    }
  },
  createPage: async (projectId, rootPath, input) => {
    const scope = captureProjectScope();
    set({ error: null });
    try {
      const response = await invoke<SaveWikiPageResponse>("create_wiki_page", {
        request: {
          projectId,
          projectRootPath: rootPath,
          relativePath: input.relativePath,
          title: input.title,
          pageType: input.pageType,
        },
      });
      if (!isProjectScopeCurrent(scope)) return;
      set({ selectedPath: response.relativePath });
      await get().scan(projectId, rootPath);
      if (!isProjectScopeCurrent(scope)) return;
      await get().openPage(projectId, rootPath, response.relativePath);
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      set({ error: errorMessage(error) });
    }
  },
  renamePage: async (projectId, rootPath, relativePath, newRelativePath) => {
    const scope = captureProjectScope();
    set({ error: null });
    try {
      const response = await invoke<RenameWikiPageResponse>("rename_wiki_page", {
        request: {
          projectId,
          projectRootPath: rootPath,
          relativePath,
          newRelativePath,
        },
      });
      if (!isProjectScopeCurrent(scope)) return;
      set({ selectedPath: response.relativePath });
      await get().scan(projectId, rootPath);
      if (!isProjectScopeCurrent(scope)) return;
      await get().openPage(projectId, rootPath, response.relativePath);
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      set({ error: errorMessage(error) });
    }
  },
  requestDeletePage: async (projectId, rootPath, relativePath) => {
    const scope = captureProjectScope();
    set({ error: null });
    try {
      const action = await invoke<PendingAction>("request_delete_wiki_page", {
        request: { projectId, projectRootPath: rootPath, relativePath },
      });
      if (!isProjectScopeCurrent(scope)) return null;
      return action;
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return null;
      set({ error: errorMessage(error) });
      return null;
    }
  },
  confirmDeletePage: async (projectId, rootPath, action) => {
    const scope = captureProjectScope();
    set({ error: null });
    try {
      await invoke<ConfirmedAction>("confirm_pending_action", {
        request: { actionId: action.id, status: "confirmed" },
      });
      if (!isProjectScopeCurrent(scope)) return;
      set({ selectedPath: null, page: null, draft: "", mode: "read" });
      await get().scan(projectId, rootPath);
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      set({ error: errorMessage(error) });
    }
  },
  cancelPendingAction: async (action) => {
    try {
      await invoke<ConfirmedAction>("confirm_pending_action", {
        request: { actionId: action.id, status: "cancelled" },
      });
    } catch (error) {
      set({ error: errorMessage(error) });
    }
  },
  reset: () => {
    pageRequestEpoch += 1;
    set({ ...initial });
  },
}));

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(error);
}
