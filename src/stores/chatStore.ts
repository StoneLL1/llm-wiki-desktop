import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";

import type {
  ChatMessage,
  ChatOverwriteRequest,
  ChatRoutePreference,
  ChatSession,
  ChatSessionSummary,
  SaveAnswerResult,
} from "../types/chat";
import type { AgentKind } from "../types/agent";
import type { LlmProviderKind } from "../types/llm";

interface BackendLikeError {
  code?: string;
  message?: string;
}

interface ErrorDetails {
  path?: string;
  currentHash?: string;
}

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(error);
}

function errorCode(error: unknown): string | undefined {
  return (error as BackendLikeError | null | undefined)?.code;
}

function errorDetails(error: unknown): ErrorDetails | undefined {
  const details = (error as { details?: unknown } | null | undefined)?.details;
  if (details && typeof details === "object") {
    return details as ErrorDetails;
  }
  return undefined;
}

interface ChatState {
  sessions: ChatSessionSummary[];
  activeSessionId: string | null;
  activeSession: ChatSession | null;
  /** Task id of the in-flight send, if any. The view watches taskStore for terminal status. */
  sendTaskId: string | null;
  /** Per-message save status keyed by message id. */
  saveStatus: Record<string, "idle" | "saving" | "saved" | "exists" | "error">;
  overwriteRequest: ChatOverwriteRequest | null;
  error: string | null;
  loadingSessions: boolean;
  loadingSession: boolean;

  loadSessions: (projectId: string, rootPath: string) => Promise<void>;
  createSession: (
    projectId: string,
    rootPath: string,
    title?: string,
  ) => Promise<ChatSession | null>;
  selectSession: (projectId: string, rootPath: string, sessionId: string) => Promise<void>;
  renameSession: (
    projectId: string,
    rootPath: string,
    sessionId: string,
    title: string,
  ) => Promise<void>;
  deleteSession: (projectId: string, rootPath: string, sessionId: string) => Promise<void>;
  send: (
    projectId: string,
    rootPath: string,
    sessionId: string,
    content: string,
    route: ChatRoutePreference,
    agent?: AgentKind | null,
    provider?: LlmProviderKind | null,
  ) => Promise<string | null>;
  clearSendTask: () => void;
  /** Reload the active session (used by the view once the send task reaches terminal status). */
  reloadActive: (projectId: string, rootPath: string) => Promise<void>;
  saveAnswer: (
    projectId: string,
    rootPath: string,
    sessionId: string,
    messageId: string,
    targetPath?: string,
  ) => Promise<SaveAnswerResult | null>;
  confirmOverwrite: (projectId: string, rootPath: string) => Promise<void>;
  cancelOverwrite: () => void;
  reset: () => void;
}

const initial = {
  sessions: [] as ChatSessionSummary[],
  activeSessionId: null as string | null,
  activeSession: null as ChatSession | null,
  sendTaskId: null as string | null,
  saveStatus: {} as ChatState["saveStatus"],
  overwriteRequest: null as ChatOverwriteRequest | null,
  error: null as string | null,
  loadingSessions: false,
  loadingSession: false,
};

export const useChatStore = create<ChatState>((set, get) => ({
  ...initial,

  loadSessions: async (projectId, rootPath) => {
    if (!hasTauri()) return;
    set({ loadingSessions: true, error: null });
    try {
      const sessions = await invoke<ChatSessionSummary[]>("list_chat_sessions", {
        request: { projectId, projectRootPath: rootPath },
      });
      set({ sessions, loadingSessions: false });
    } catch (error) {
      set({ loadingSessions: false, error: errorMessage(error) });
    }
  },

  createSession: async (projectId, rootPath, title) => {
    if (!hasTauri()) return null;
    set({ error: null });
    try {
      const session = await invoke<ChatSession>("create_chat_session", {
        request: { projectId, projectRootPath: rootPath, title: title ?? null },
      });
      await get().loadSessions(projectId, rootPath);
      await get().selectSession(projectId, rootPath, session.id);
      return session;
    } catch (error) {
      set({ error: errorMessage(error) });
      return null;
    }
  },

  selectSession: async (projectId, rootPath, sessionId) => {
    if (!hasTauri()) return;
    set({ loadingSession: true, activeSessionId: sessionId, error: null, overwriteRequest: null });
    try {
      const session = await invoke<ChatSession>("load_chat_session", {
        request: { projectId, projectRootPath: rootPath, sessionId },
      });
      set({ activeSession: session, loadingSession: false });
    } catch (error) {
      set({ loadingSession: false, error: errorMessage(error) });
    }
  },

  renameSession: async (projectId, rootPath, sessionId, title) => {
    if (!hasTauri()) return;
    set({ error: null });
    try {
      await invoke<ChatSession>("rename_chat_session", {
        request: { projectId, projectRootPath: rootPath, sessionId, title },
      });
      await get().loadSessions(projectId, rootPath);
      if (get().activeSessionId === sessionId) {
        await get().selectSession(projectId, rootPath, sessionId);
      }
    } catch (error) {
      set({ error: errorMessage(error) });
    }
  },

  deleteSession: async (projectId, rootPath, sessionId) => {
    if (!hasTauri()) return;
    set({ error: null });
    try {
      await invoke("delete_chat_session", {
        request: { projectId, projectRootPath: rootPath, sessionId },
      });
      if (get().activeSessionId === sessionId) {
        set({ activeSessionId: null, activeSession: null });
      }
      await get().loadSessions(projectId, rootPath);
    } catch (error) {
      set({ error: errorMessage(error) });
    }
  },

  send: async (projectId, rootPath, sessionId, content, route, agent, provider) => {
    if (!hasTauri()) return null;
    set({ error: null });
    try {
      const task = await invoke<{ id: string }>("send_chat_message", {
        request: {
          projectId,
          projectRootPath: rootPath,
          sessionId,
          content,
          route,
          agent: agent ?? null,
          provider: provider ?? null,
        },
      });
      set({ sendTaskId: task.id });
      return task.id;
    } catch (error) {
      set({ error: errorMessage(error) });
      return null;
    }
  },

  clearSendTask: () => set({ sendTaskId: null }),

  reloadActive: async (projectId, rootPath) => {
    const sessionId = get().activeSessionId;
    if (!sessionId) return;
    await get().selectSession(projectId, rootPath, sessionId);
  },

  saveAnswer: async (projectId, rootPath, sessionId, messageId, targetPath) => {
    if (!hasTauri()) return null;
    set((state) => ({
      saveStatus: { ...state.saveStatus, [messageId]: "saving" },
      overwriteRequest: null,
      error: null,
    }));
    try {
      const result = await invoke<SaveAnswerResult>("save_answer_to_wiki", {
        request: {
          projectId,
          projectRootPath: rootPath,
          sessionId,
          messageId,
          targetPath: targetPath ?? null,
          expectedHash: null,
          allowOverwrite: false,
        },
      });
      set((state) => ({ saveStatus: { ...state.saveStatus, [messageId]: "saved" } }));
      return result;
    } catch (error) {
      if (errorCode(error) === "FILE_ALREADY_EXISTS") {
        const details = errorDetails(error);
        const path = details?.path ?? "";
        const currentHash = details?.currentHash ?? "";
        set((state) => ({
          saveStatus: { ...state.saveStatus, [messageId]: "exists" },
          overwriteRequest: { messageId, path, currentHash },
        }));
        return null;
      }
      set((state) => ({
        saveStatus: { ...state.saveStatus, [messageId]: "error" },
        error: errorMessage(error),
      }));
      return null;
    }
  },

  confirmOverwrite: async (projectId, rootPath) => {
    if (!hasTauri()) return;
    const request = get().overwriteRequest;
    const sessionId = get().activeSessionId;
    if (!request || !sessionId) return;
    const messageId = request.messageId;
    set((state) => ({ saveStatus: { ...state.saveStatus, [messageId]: "saving" } }));
    try {
      await invoke<SaveAnswerResult>("save_answer_to_wiki", {
        request: {
          projectId,
          projectRootPath: rootPath,
          sessionId,
          messageId,
          targetPath: null,
          expectedHash: request.currentHash,
          allowOverwrite: true,
        },
      });
      set((state) => ({
        saveStatus: { ...state.saveStatus, [messageId]: "saved" },
        overwriteRequest: null,
      }));
    } catch (error) {
      set((state) => ({
        saveStatus: { ...state.saveStatus, [messageId]: "error" },
        overwriteRequest: null,
        error: errorMessage(error),
      }));
    }
  },

  cancelOverwrite: () => set({ overwriteRequest: null }),

  reset: () => set({ ...initial }),
}));

/** The latest assistant message in a session (used for the citations panel). */
export function latestAssistantMessage(session: ChatSession | null): ChatMessage | null {
  if (!session) return null;
  for (let i = session.messages.length - 1; i >= 0; i -= 1) {
    const message = session.messages[i];
    if (message && message.role === "assistant") return message;
  }
  return null;
}
