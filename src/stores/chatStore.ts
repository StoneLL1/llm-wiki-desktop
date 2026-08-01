import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";
import { i18next } from "../i18n";

import type {
  ChatMessage,
  ChatOverwriteRequest,
  ChatRoute,
  ChatRoutePreference,
  ChatSession,
  ChatSessionSummary,
  SaveAnswerResult,
} from "../types/chat";
import type { AgentKind } from "../types/agent";
import type { LlmProviderKind } from "../types/llm";
import { captureProjectScope, isProjectScopeCurrent } from "./projectScope";
import { fetchTaskActivities, useTaskStore } from "./taskStore";
import type { BackendTask } from "../types/task";

interface BackendLikeError {
  code?: string;
  message?: string;
}

interface ErrorDetails {
  path?: string;
  currentHash?: string;
  actionId?: string;
}

export interface SendChatOptions {
  agent?: AgentKind | null;
  provider?: LlmProviderKind | null;
  pinnedPagePath?: string | null;
  convenienceEnabled?: boolean;
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
  /** Session the in-flight send targeted, so a terminal-status reload only
   *  refreshes that session — not whatever the user switched to mid-send. */
  sendSessionId: string | null;
  /** Prevents two panes from racing before the backend returns a task id. */
  sendStarting: boolean;
  /** Per-message save status keyed by message id. */
  saveStatus: Record<string, "idle" | "saving" | "saved" | "exists" | "error">;
  /** A single save mutation may be in flight across all Chat surfaces. */
  saveInFlightMessageId: string | null;
  /** Save paths returned by the backend before the active session is reloaded. */
  savedAnswerPaths: Record<string, string>;
  /** Serializes keep/rollback decisions for convenience edits. */
  convenienceMutationKey: string | null;
  overwriteRequest: ChatOverwriteRequest | null;
  error: string | null;
  loadingSessions: boolean;
  loadingSession: boolean;
  /** Live-streamed assistant text (ephemeral; replaced by the persisted
   *  message once the send task reaches a terminal status and the session
   *  reloads). Backend channel: `task://stream-output`. */
  streamingText: string;
  streamingRoute: ChatRoute | null;
  /** Deltas can arrive before the send IPC response binds the task id. */
  pendingStreamDeltas: Record<string, { text: string; route: ChatRoute | null; receivedAt: number }>;
  /** User messages accepted by the backend but not yet visible in a reloaded session. */
  pendingUserMessages: Record<string, ChatMessage>;

  /** Monotonic counter bumped each time ensurePageSession starts. An in-flight
   *  ensure bails after any await if a newer page focus has superseded it,
   *  so a slow list/select for page A can't drop page A's thread onto page B
   *  after the user switched away. */
  pageSessionEpoch: number;
  /** Invalidates out-of-order ordinary session loads/selections. */
  selectionEpoch: number;

  loadSessions: (
    projectId: string,
    rootPath: string,
    options?: { autoSelect?: boolean },
  ) => Promise<void>;
  createSession: (
    projectId: string,
    rootPath: string,
    title?: string,
    contextPagePath?: string | null,
  ) => Promise<ChatSession | null>;
  /** Resolve the chat session for a wiki page (Wiki AI sidebar). Lazy on
   *  visit, explicit on send:
   *  - `forceNew=false` (page focus): reuse an existing session scoped to
   *    `pagePath` if one exists; otherwise clear any stale active session
   *    and return null WITHOUT creating. The first send in PageChatPanel
   *    creates the session (see handleSend). This stops a different page's
   *    thread from bleeding onto the new page.
   *  - `forceNew=true` (New Chat button): always create a fresh session
   *    tagged with `contextPagePath`.
   *  Returns the active session, or null if none applies / backend unavailable. */
  ensurePageSession: (
    projectId: string,
    rootPath: string,
    pagePath: string,
    pageTitle: string,
    forceNew: boolean,
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
    options?: SendChatOptions,
  ) => Promise<string | null>;
  cancelTask: (taskId: string) => Promise<void>;
  clearSendTask: (error?: string | null) => void;
  /** Reload the active session (used by the view once the send task reaches terminal status). */
  reloadActive: (projectId: string, rootPath: string) => Promise<void>;
  /** Reload a known session without allowing a later selection to clobber it. */
  reloadSession: (projectId: string, rootPath: string, sessionId: string) => Promise<void>;
  saveAnswer: (
    projectId: string,
    rootPath: string,
    sessionId: string,
    messageId: string,
    targetPath?: string,
  ) => Promise<SaveAnswerResult | null>;
  confirmOverwrite: (projectId: string, rootPath: string) => Promise<void>;
  cancelOverwrite: () => Promise<void>;
  resolveConvenienceEdit: (
    projectId: string,
    rootPath: string,
    sessionId: string,
    messageId: string,
    keep: boolean,
  ) => Promise<void>;
  rollbackLastConvenienceEdit: (
    projectId: string,
    rootPath: string,
    sessionId: string,
  ) => Promise<void>;
  appendStreamDelta: (taskId: string, delta: string, route: ChatRoute | null) => void;
  reset: () => void;
}

const initial = {
  sessions: [] as ChatSessionSummary[],
  activeSessionId: null as string | null,
  activeSession: null as ChatSession | null,
  sendTaskId: null as string | null,
  sendSessionId: null as string | null,
  sendStarting: false,
  saveStatus: {} as ChatState["saveStatus"],
  saveInFlightMessageId: null as string | null,
  savedAnswerPaths: {} as ChatState["savedAnswerPaths"],
  convenienceMutationKey: null as string | null,
  overwriteRequest: null as ChatOverwriteRequest | null,
  error: null as string | null,
  loadingSessions: false,
  loadingSession: false,
  streamingText: "",
  streamingRoute: null as ChatRoute | null,
  pendingStreamDeltas: {} as ChatState["pendingStreamDeltas"],
  pendingUserMessages: {} as ChatState["pendingUserMessages"],
  pageSessionEpoch: 0,
  selectionEpoch: 0,
};

export const useChatStore = create<ChatState>((set, get) => ({
  ...initial,

  loadSessions: async (projectId, rootPath, options) => {
    if (!hasTauri()) return;
    const autoSelect = options?.autoSelect ?? true;
    const scope = captureProjectScope();
    set({ loadingSessions: true, error: null });
    try {
      const sessions = await invoke<ChatSessionSummary[]>("list_chat_sessions", {
        request: { projectId, projectRootPath: rootPath },
      });
      if (!isProjectScopeCurrent(scope)) return;
      set({ sessions, loadingSessions: false });
      // Auto-select the newest session (summaries are sorted newest-first by
      // the backend) only when the user has not already selected one. This
      // restores a usable composer when reopening Chat; an explicit selection
      // survives a list refresh. Suppressed during page-session resolution,
      // where ensurePageSession owns selection and a mid-flight auto-select
      // could race a page switch.
      if (autoSelect && !get().activeSessionId && sessions[0]) {
        await get().selectSession(projectId, rootPath, sessions[0].id);
        if (!isProjectScopeCurrent(scope)) return;
      }
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      set({ loadingSessions: false, error: errorMessage(error) });
    }
  },

  createSession: async (projectId, rootPath, title, contextPagePath) => {
    if (!hasTauri()) return null;
    const scope = captureProjectScope();
    const selectionEpoch = get().selectionEpoch;
    set({ error: null });
    try {
      const session = await invoke<ChatSession>("create_chat_session", {
        request: {
          projectId,
          projectRootPath: rootPath,
          title: title ?? null,
          contextPagePath: contextPagePath ?? null,
        },
      });
      if (!isProjectScopeCurrent(scope)) return null;
      await get().loadSessions(projectId, rootPath, { autoSelect: false });
      if (!isProjectScopeCurrent(scope)) return null;
      // A user selection that happened while creation was in flight owns the
      // view. The new session remains persisted and appears in the refreshed
      // list, but must not steal the selection back.
      if (get().selectionEpoch !== selectionEpoch) return session;
      await get().selectSession(projectId, rootPath, session.id);
      return session;
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return null;
      set({ error: errorMessage(error) });
      return null;
    }
  },

  ensurePageSession: async (projectId, rootPath, pagePath, pageTitle, forceNew) => {
    if (!hasTauri()) return null;
    const scope = captureProjectScope();
    // Claim the latest page-focus slot. Any older in-flight ensure bails once
    // it sees pageSessionEpoch has moved past its captured value, so a slow
    // list/select for page A cannot drop page A's thread onto page B after a
    // rapid switch.
    const epoch = get().pageSessionEpoch + 1;
    const selectionEpoch = get().selectionEpoch + 1;
    set({
      pageSessionEpoch: epoch,
      selectionEpoch,
      activeSessionId: null,
      activeSession: null,
      error: null,
    });
    const superseded = () =>
      get().pageSessionEpoch !== epoch || get().selectionEpoch !== selectionEpoch;
    try {
      // Make sure the session list reflects disk before we search it, so a
      // previously-created page session is reused rather than duplicated.
      // autoSelect:false keeps loadSessions' newest-select side-effect from
      // racing this resolution (it would otherwise fire before we can check
      // the epoch below).
      await get().loadSessions(projectId, rootPath, { autoSelect: false });
      if (!isProjectScopeCurrent(scope) || superseded()) return null;
      const normalized = normalizePagePath(pagePath);
      const existing = get().sessions.find(
        (summary) => normalizePagePath(summary.contextPagePath ?? "") === normalized,
      );
      if (existing && !forceNew) {
        // Load + commit inline. Going through selectSession would set
        // activeSessionId synchronously and activeSession after its await —
        // both without an epoch check — so a superseded call could drop
        // page A's thread onto page B after a rapid switch. Committing here
        // lets us re-check the epoch right before the set.
        const loaded = await invoke<ChatSession>("load_chat_session", {
          request: { projectId, projectRootPath: rootPath, sessionId: existing.id },
        });
        if (!isProjectScopeCurrent(scope) || superseded()) return null;
        set({ activeSessionId: existing.id, activeSession: loaded });
        return loaded;
      }
      if (!forceNew) {
        // Lazy: do not create on visit. Clear any stale active session so a
        // different page's thread does not bleed onto this page; the first
        // send creates the session (PageChatPanel.handleSend).
        set({ activeSessionId: null, activeSession: null });
        return null;
      }
      // forceNew: create inline (createSession would selectSession internally,
      // same supersession leak as the reuse branch). The session is written to
      // disk tagged with contextPagePath regardless; we only skip selecting it
      // onto a stale view if a newer page focus has superseded this one.
      const title = pageTitle.trim() ? `Ask: ${pageTitle.trim()}` : "New page chat";
      const created = await invoke<ChatSession>("create_chat_session", {
        request: { projectId, projectRootPath: rootPath, title, contextPagePath: normalized },
      });
      if (!isProjectScopeCurrent(scope) || superseded()) return null;
      await get().loadSessions(projectId, rootPath, { autoSelect: false });
      if (!isProjectScopeCurrent(scope) || superseded()) return null;
      set({ activeSessionId: created.id, activeSession: created });
      return created;
    } catch (error) {
      if (!isProjectScopeCurrent(scope) || superseded()) return null;
      set({ error: errorMessage(error) });
      return null;
    }
  },

  selectSession: async (projectId, rootPath, sessionId) => {
    if (!hasTauri()) return;
    const scope = captureProjectScope();
    const selectionEpoch = get().selectionEpoch + 1;
    set({
      selectionEpoch,
      loadingSession: true,
      activeSessionId: sessionId,
      activeSession: null,
      error: null,
    });
    try {
      const session = await invoke<ChatSession>("load_chat_session", {
        request: { projectId, projectRootPath: rootPath, sessionId },
      });
      if (!isProjectScopeCurrent(scope) || get().selectionEpoch !== selectionEpoch) return;
      set({ activeSession: session, loadingSession: false });
    } catch (error) {
      if (!isProjectScopeCurrent(scope) || get().selectionEpoch !== selectionEpoch) return;
      set({ loadingSession: false, error: errorMessage(error) });
    }
  },

  renameSession: async (projectId, rootPath, sessionId, title) => {
    if (!hasTauri()) return;
    const scope = captureProjectScope();
    set({ error: null });
    try {
      await invoke<ChatSession>("rename_chat_session", {
        request: { projectId, projectRootPath: rootPath, sessionId, title },
      });
      if (!isProjectScopeCurrent(scope)) return;
      await get().loadSessions(projectId, rootPath);
      if (get().activeSessionId === sessionId) {
        await get().selectSession(projectId, rootPath, sessionId);
      }
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      set({ error: errorMessage(error) });
    }
  },

  deleteSession: async (projectId, rootPath, sessionId) => {
    if (!hasTauri()) return;
    const scope = captureProjectScope();
    const deletingActive = get().activeSessionId === sessionId;
    if (deletingActive) {
      // Invalidate any in-flight load and stop its skeleton immediately. The
      // stale response will be ignored by selectionEpoch after deletion.
      set((state) => ({ selectionEpoch: state.selectionEpoch + 1, loadingSession: false }));
    }
    set({ error: null });
    try {
      await invoke("delete_chat_session", {
        request: { projectId, projectRootPath: rootPath, sessionId },
      });
      if (!isProjectScopeCurrent(scope)) return;
      if (deletingActive && get().activeSessionId === sessionId) {
        set({ activeSessionId: null, activeSession: null });
      }
      await get().loadSessions(projectId, rootPath);
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      set({ error: errorMessage(error) });
    }
  },

  send: async (projectId, rootPath, sessionId, content, route, options) => {
    if (!hasTauri()) return null;
    if (
      get().sendStarting ||
      get().sendTaskId ||
      get().saveInFlightMessageId ||
      get().overwriteRequest ||
      get().convenienceMutationKey
    ) return null;
    const scope = captureProjectScope();
    set({ error: null, sendStarting: true });
    try {
      const task = await invoke<BackendTask>("send_chat_message", {
        request: {
          projectId,
          projectRootPath: rootPath,
          sessionId,
          content,
          route,
          agent: options?.agent ?? null,
          provider: options?.provider ?? null,
          pinnedPagePath: options?.pinnedPagePath ?? null,
          convenienceEnabled: options?.convenienceEnabled ?? false,
        },
      });
      // Task facts are global history and must survive a project switch. Only
      // the presentation binding below is project-scoped.
      useTaskStore.getState().upsertTask(task);
      if (!isProjectScopeCurrent(scope)) return null;
      const pending = get().pendingStreamDeltas[task.id];
      set((state) => {
        const pendingStreamDeltas = { ...state.pendingStreamDeltas };
        delete pendingStreamDeltas[task.id];
        return {
          sendTaskId: task.id,
          sendSessionId: sessionId,
          streamingText: pending?.text ?? "",
          streamingRoute: pending?.route ?? null,
          pendingUserMessages: {
            ...state.pendingUserMessages,
            [task.id]: {
              id: `pending-user-${task.id}`,
              role: "user",
              content,
              createdAt: new Date().toISOString(),
              taskId: task.id,
            },
          },
          pendingStreamDeltas,
        };
      });
      void fetchTaskActivities(task.id);
      return task.id;
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return null;
      set({ error: errorMessage(error) });
      return null;
    } finally {
      // A late response from a previous project must not release the start
      // gate for a newer project's send attempt.
      if (isProjectScopeCurrent(scope)) set({ sendStarting: false });
    }
  },

  cancelTask: async (taskId) => {
    if (!hasTauri()) return;
    const scope = captureProjectScope();
    const { activeProjectId: projectId, activeProjectRootPath: projectRootPath } = useTaskStore.getState();
    if (!projectId || !projectRootPath) return;
    try {
      const task = await invoke<BackendTask>("cancel_task", {
        request: { taskId, projectId, projectRootPath },
      });
      if (!isProjectScopeCurrent(scope)) return;
      useTaskStore.getState().upsertTask(task);
    } catch (error) {
      if (isProjectScopeCurrent(scope)) set({ error: errorMessage(error) });
    }
  },

  clearSendTask: (error = null) =>
    set((state) => {
      const pendingStreamDeltas = { ...state.pendingStreamDeltas };
      const pendingUserMessages = { ...state.pendingUserMessages };
      if (state.sendTaskId) delete pendingStreamDeltas[state.sendTaskId];
      if (state.sendTaskId) delete pendingUserMessages[state.sendTaskId];
      return {
        sendTaskId: null,
        sendSessionId: null,
        sendStarting: false,
        streamingText: "",
        streamingRoute: null,
        pendingStreamDeltas,
        pendingUserMessages,
        error: error ?? state.error,
      };
    }),

  reloadActive: async (projectId, rootPath) => {
    // Only reload the session the send targeted; if the user switched away
    // mid-send, leave their current view alone rather than yanking it back.
    const sessionId = get().sendSessionId;
    if (!sessionId) return;
    await get().reloadSession(projectId, rootPath, sessionId);
  },

  reloadSession: async (projectId, rootPath, sessionId) => {
    if (!hasTauri() || get().activeSessionId !== sessionId) return;
    const scope = captureProjectScope();
    const selectionEpoch = get().selectionEpoch;
    set({ loadingSession: true });
    try {
      const session = await invoke<ChatSession>("load_chat_session", {
        request: { projectId, projectRootPath: rootPath, sessionId },
      });
      if (
        !isProjectScopeCurrent(scope) ||
        get().selectionEpoch !== selectionEpoch ||
        get().activeSessionId !== sessionId
      ) return;
      set({ activeSession: session, loadingSession: false });
    } catch (error) {
      if (
        !isProjectScopeCurrent(scope) ||
        get().selectionEpoch !== selectionEpoch ||
        get().activeSessionId !== sessionId
      ) return;
      set({ loadingSession: false, error: errorMessage(error) });
    }
  },

  saveAnswer: async (projectId, rootPath, sessionId, messageId, targetPath) => {
    if (!hasTauri()) return null;
    const pendingOverwrite = get().overwriteRequest;
    if (pendingOverwrite || get().saveInFlightMessageId) {
      // A pending backend-issued action owns the exact answer/path. Do not
      // clear it and create another action when a save button is clicked
      // again; both same-message retries and cross-session saves must wait
      // for the visible confirm/cancel decision.
      set({
        error: i18next.t(
          pendingOverwrite ? "chat.errors.overwritePending" : "chat.errors.savePending",
        ),
      });
      return null;
    }
    const scope = captureProjectScope();
    set((state) => ({
      saveStatus: { ...state.saveStatus, [messageId]: "saving" },
      saveInFlightMessageId: messageId,
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
      if (!isProjectScopeCurrent(scope)) return null;
      set((state) => ({
        saveStatus: { ...state.saveStatus, [messageId]: "saved" },
        savedAnswerPaths: { ...state.savedAnswerPaths, [messageId]: result.path },
      }));
      return result;
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return null;
      if (errorCode(error) === "FILE_ALREADY_EXISTS") {
        const details = errorDetails(error);
        const path = details?.path ?? "";
        const currentHash = details?.currentHash ?? "";
        const actionId = details?.actionId ?? "";
        set((state) => ({
          saveStatus: { ...state.saveStatus, [messageId]: "exists" },
          overwriteRequest: { sessionId, messageId, path, currentHash, actionId },
        }));
        return null;
      }
      set((state) => ({
        saveStatus: { ...state.saveStatus, [messageId]: "error" },
        error: errorMessage(error),
      }));
      return null;
    } finally {
      set((state) =>
        state.saveInFlightMessageId === messageId ? { saveInFlightMessageId: null } : {},
      );
    }
  },

  confirmOverwrite: async (projectId, rootPath) => {
    if (!hasTauri()) return;
    const scope = captureProjectScope();
    const request = get().overwriteRequest;
    const sessionId = request?.sessionId;
    if (!request || !sessionId) return;
    if (get().saveInFlightMessageId) return;
    const messageId = request.messageId;
    set((state) => ({
      saveStatus: { ...state.saveStatus, [messageId]: "saving" },
      saveInFlightMessageId: messageId,
      error: null,
    }));
    try {
      const result = await invoke<SaveAnswerResult>("save_answer_to_wiki", {
        request: {
          projectId,
          projectRootPath: rootPath,
          sessionId,
          messageId,
          // Re-target the exact page the first attempt collided on, rather
          // than falling back to the regenerated slug (a different file).
          targetPath: request.path,
          expectedHash: request.currentHash,
          allowOverwrite: true,
          actionId: request.actionId,
        },
      });
      if (!isProjectScopeCurrent(scope)) return;
      set((state) => ({
        saveStatus: { ...state.saveStatus, [messageId]: "saved" },
        savedAnswerPaths: { ...state.savedAnswerPaths, [messageId]: result.path },
        overwriteRequest: null,
      }));
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      set((state) => ({
        saveStatus: { ...state.saveStatus, [messageId]: "error" },
        overwriteRequest: null,
        error: errorMessage(error),
      }));
    } finally {
      set((state) =>
        state.saveInFlightMessageId === messageId ? { saveInFlightMessageId: null } : {},
      );
    }
  },

  cancelOverwrite: async () => {
    const scope = captureProjectScope();
    const request = get().overwriteRequest;
    if (get().saveInFlightMessageId) return;
    const actionId = request?.actionId;
    set({ overwriteRequest: null });
    if (!actionId || !hasTauri()) return;
    try {
      await invoke("confirm_pending_action", {
        request: { actionId, status: "cancelled" },
      });
    } catch (error) {
      if (isProjectScopeCurrent(scope)) {
        set({ overwriteRequest: request, error: errorMessage(error) });
      }
    }
  },

  resolveConvenienceEdit: async (projectId, rootPath, sessionId, messageId, keep) => {
    if (!hasTauri()) return;
    const mutationKey = `resolve:${sessionId}:${messageId}`;
    if (get().convenienceMutationKey) return;
    const scope = captureProjectScope();
    const selectionEpoch = get().selectionEpoch;
    set({ error: null, convenienceMutationKey: mutationKey });
    try {
      const session = await invoke<ChatSession>("resolve_chat_convenience_edit", {
        request: { projectId, projectRootPath: rootPath, sessionId, messageId, keep },
      });
      if (!isProjectScopeCurrent(scope)) return;
      if (get().selectionEpoch === selectionEpoch && get().activeSessionId === sessionId) {
        set({ activeSession: session, activeSessionId: session.id });
      }
      await get().loadSessions(projectId, rootPath, { autoSelect: false });
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      if (get().selectionEpoch === selectionEpoch && get().activeSessionId === sessionId) {
        await get().reloadSession(projectId, rootPath, sessionId);
        if (get().selectionEpoch === selectionEpoch && get().activeSessionId === sessionId) {
          set({ error: errorMessage(error) });
        }
      }
    } finally {
      if (isProjectScopeCurrent(scope)) {
        set((state) =>
          state.convenienceMutationKey === mutationKey ? { convenienceMutationKey: null } : {},
        );
      }
    }
  },

  rollbackLastConvenienceEdit: async (projectId, rootPath, sessionId) => {
    if (!hasTauri()) return;
    const mutationKey = `rollback:${sessionId}`;
    if (get().convenienceMutationKey) return;
    const scope = captureProjectScope();
    const selectionEpoch = get().selectionEpoch;
    set({ error: null, convenienceMutationKey: mutationKey });
    try {
      const session = await invoke<ChatSession>("rollback_last_chat_convenience_edit", {
        request: { projectId, projectRootPath: rootPath, sessionId },
      });
      if (!isProjectScopeCurrent(scope)) return;
      if (get().selectionEpoch === selectionEpoch && get().activeSessionId === sessionId) {
        set({ activeSession: session, activeSessionId: session.id });
      }
      await get().loadSessions(projectId, rootPath, { autoSelect: false });
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      if (get().selectionEpoch === selectionEpoch && get().activeSessionId === sessionId) {
        await get().reloadSession(projectId, rootPath, sessionId);
        if (get().selectionEpoch === selectionEpoch && get().activeSessionId === sessionId) {
          set({ error: errorMessage(error) });
        }
      }
    } finally {
      if (isProjectScopeCurrent(scope)) {
        set((state) =>
          state.convenienceMutationKey === mutationKey ? { convenienceMutationKey: null } : {},
        );
      }
    }
  },

  appendStreamDelta: (taskId, delta, route) => {
    // Most events arrive after send() binds the returned task. Keep a bounded
    // per-task buffer for the small race where the worker emits before the IPC
    // response reaches the frontend; task ids prevent stale sends from
    // bleeding into the next generation.
    if (taskId !== get().sendTaskId) {
      set((state) => {
        const now = Date.now();
        const pendingStreamDeltas = Object.fromEntries(
          Object.entries(state.pendingStreamDeltas)
            .filter(([, value]) => now - (value.receivedAt ?? now) < 30_000)
            .sort(([, left], [, right]) => (left.receivedAt ?? 0) - (right.receivedAt ?? 0))
            .slice(-7),
        ) as ChatState["pendingStreamDeltas"];
        const existing = pendingStreamDeltas[taskId] ?? { text: "", route: null, receivedAt: now };
        const text = `${existing.text}${delta}`.slice(-256 * 1024);
        pendingStreamDeltas[taskId] = {
          text,
          route: route ?? existing.route,
          receivedAt: existing.receivedAt ?? now,
        };
        return {
          pendingStreamDeltas,
        };
      });
      return;
    }
    set((state) => ({
      streamingText: state.streamingText + delta,
      streamingRoute: route ?? state.streamingRoute,
    }));
  },

  reset: () => set({ ...initial }),
}));

function normalizePagePath(path: string): string {
  const normalized = path.replace(/\\/g, "/").trim();
  // Windows paths are case-insensitive. Keep Linux/macOS matching
  // case-sensitive so two intentionally distinct wiki pages remain distinct.
  return typeof navigator !== "undefined" && /windows/i.test(navigator.userAgent)
    ? normalized.toLowerCase()
    : normalized;
}

/** The latest assistant message in a session (used for the citations panel). */
export function latestAssistantMessage(session: ChatSession | null): ChatMessage | null {
  if (!session) return null;
  for (let i = session.messages.length - 1; i >= 0; i -= 1) {
    const message = session.messages[i];
    if (message && message.role === "assistant") return message;
  }
  return null;
}
