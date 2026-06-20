import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";

import type {
  SaveWikiPageResponse,
  WikiPageContent,
  WikiTree,
} from "../../types/wiki";

export type WikiMode = "read" | "edit";
export type SaveState = "idle" | "saving" | "saved" | "conflict" | "error";

interface BackendLikeError {
  code?: string;
  message?: string;
}

function isConflictError(error: unknown): boolean {
  const code = (error as BackendLikeError | null | undefined)?.code;
  return code === "FILE_HASH_MISMATCH";
}

interface WikiState {
  tree: WikiTree | null;
  selectedPath: string | null;
  page: WikiPageContent | null;
  mode: WikiMode;
  saveState: SaveState;
  /** Live editor contents (raw markdown including frontmatter). */
  draft: string;
  loadingTree: boolean;
  loadingPage: boolean;
  error: string | null;
  scan: (projectId: string, rootPath: string) => Promise<void>;
  openPage: (projectId: string, rootPath: string, path: string) => Promise<void>;
  setMode: (mode: WikiMode) => void;
  startEdit: () => void;
  cancelEdit: () => void;
  setDraft: (draft: string) => void;
  save: (projectId: string, rootPath: string) => Promise<void>;
  reload: (projectId: string, rootPath: string) => Promise<void>;
  reset: () => void;
}

const initial = {
  tree: null as WikiTree | null,
  selectedPath: null as string | null,
  page: null as WikiPageContent | null,
  mode: "read" as WikiMode,
  saveState: "idle" as SaveState,
  draft: "",
  loadingTree: false,
  loadingPage: false,
  error: null as string | null,
};

export const useWikiStore = create<WikiState>((set, get) => ({
  ...initial,
  scan: async (projectId, rootPath) => {
    set({ loadingTree: true, error: null });
    try {
      const tree = await invoke<WikiTree>("scan_wiki", {
        request: { projectId, projectRootPath: rootPath },
      });
      const firstPage = tree.pages[0]?.path ?? null;
      set({ tree, loadingTree: false });
      if (firstPage && !get().selectedPath) {
        await get().openPage(projectId, rootPath, firstPage);
      }
    } catch (error) {
      set({ loadingTree: false, error: errorMessage(error) });
    }
  },
  openPage: async (projectId, rootPath, path) => {
    set({ loadingPage: true, selectedPath: path, mode: "read", saveState: "idle", error: null });
    try {
      const page = await invoke<WikiPageContent>("read_wiki_page", {
        request: { projectId, projectRootPath: rootPath, relativePath: path },
      });
      set({ page, draft: page.rawMarkdown, loadingPage: false });
    } catch (error) {
      set({ loadingPage: false, error: errorMessage(error) });
    }
  },
  setMode: (mode) => set({ mode }),
  startEdit: () => {
    const page = get().page;
    if (page) set({ mode: "edit", draft: page.rawMarkdown, saveState: "idle" });
  },
  cancelEdit: () => {
    const page = get().page;
    set({
      mode: "read",
      draft: page?.rawMarkdown ?? "",
      saveState: "idle",
    });
  },
  setDraft: (draft) => set({ draft, saveState: "idle" }),
  save: async (projectId, rootPath) => {
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
        if (stillViewing(get().selectedPath)) {
          set({ saveState: "saved" });
        }
      }
    } catch (error) {
      if (isConflictError(error)) {
        set({ saveState: "conflict" });
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
  reset: () => set({ ...initial }),
}));

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(error);
}
