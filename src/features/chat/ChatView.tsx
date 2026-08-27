import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useShallow } from "zustand/react/shallow";
import { ChevronDown, Pencil, Sparkles, Trash2 } from "lucide-react";

import { latestAssistantMessage, useChatStore } from "../../stores/chatStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { useProjectStore } from "../../stores/projectStore";
import { fetchTaskActivities, selectProjectTaskById, useTaskStore } from "../../stores/taskStore";
import { useNavigationStore } from "../../stores/navigationStore";
import { useWikiStore } from "../../features/wiki/wikiStore";
import { isTerminalStatus } from "../../types/task";
import type { TaskActivity, TaskStatus } from "../../types/task";
import type { ChatMessage, ChatRoutePreference } from "../../types/chat";
import { ChatComposer } from "./ChatComposer";
import { ChatConveniencePanel } from "./ChatConveniencePanel";
import { ChatSessionList } from "./ChatSessionList";
import { MessageContent } from "./MessageContent";
import { AgentActivityTimeline } from "../../components/agent/AgentActivityTimeline";
import { ActionableErrorNotice } from "../../components/app/ActionableErrorNotice";
import { observeProjectResources } from "../../stores/projectScope";
import { projectResourceKey } from "../../lib/projectResourceFreshness";
import { isAiConfigurationErrorCode, normalizeBackendError } from "../../lib/backendError";
import {
  activateRoutePresentationProject,
  readChatRoutePreference,
  readRouteScrollPosition,
  saveChatRoutePreference,
  saveRouteScrollPosition,
} from "../../hooks/useRouteScrollRestoration";

const SEGMENT_OPTIONS: readonly { value: ChatRoutePreference; key: string }[] = [
  { value: "auto", key: "chat.composer.route.auto" },
  { value: "agent", key: "chat.composer.route.agent" },
  { value: "byok", key: "chat.composer.route.byok" },
];

/** Format an ISO timestamp as HH:MM (locale-independent, 24h). */
export function formatTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

function hasPendingConvenienceEdit(session: { messages: ChatMessage[] } | null): boolean {
  return Boolean(
    session?.messages.some(
      (message) => message.convenienceEdit?.status === "soft_violation_pending",
    ),
  );
}

export function ChatView() {
  const { t } = useTranslation();
  const currentProject = useProjectStore((state) => state.currentProject);
  const [creatingSession, setCreatingSession] = useState(false);

  const sessions = useChatStore((state) => state.sessions);
  const activeSessionId = useChatStore((state) => state.activeSessionId);
  const activeSession = useChatStore((state) => state.activeSession);
  const [routePreference, setRoutePreferenceState] = useState<ChatRoutePreference>(readChatRoutePreference);
  const loadingSessions = useChatStore((state) => state.loadingSessions);
  const loadingSession = useChatStore((state) => state.loadingSession);
  const sendTaskId = useChatStore((state) => state.sendTaskId);
  const sendSessionId = useChatStore((state) => state.sendSessionId);
  const sendStarting = useChatStore((state) => state.sendStarting);
  const clearSendTask = useChatStore((state) => state.clearSendTask);
  const saveStatus = useChatStore((state) => state.saveStatus);
  const saveInFlightMessageId = useChatStore((state) => state.saveInFlightMessageId);
  const overwriteRequest = useChatStore((state) => state.overwriteRequest);
  const convenienceMutationKey = useChatStore((state) => state.convenienceMutationKey);
  const savedAnswerPaths = useChatStore((state) => state.savedAnswerPaths);
  const error = useChatStore((state) => state.error);
  const streamingText = useChatStore((state) => state.streamingText);
  const streamingRoute = useChatStore((state) => state.streamingRoute);
  const streamRevision = useChatStore((state) => state.streamRevision);
  const pendingUserMessages = useChatStore((state) => state.pendingUserMessages);

  const ensureSessions = useChatStore((state) => state.ensureSessions);
  const createSession = useChatStore((state) => state.createSession);
  const selectSession = useChatStore((state) => state.selectSession);
  const renameSession = useChatStore((state) => state.renameSession);
  const deleteSession = useChatStore((state) => state.deleteSession);
  const send = useChatStore((state) => state.send);
  const cancelChatTask = useChatStore((state) => state.cancelTask);
  const reloadActive = useChatStore((state) => state.reloadActive);
  const saveAnswer = useChatStore((state) => state.saveAnswer);
  const confirmOverwrite = useChatStore((state) => state.confirmOverwrite);
  const cancelOverwrite = useChatStore((state) => state.cancelOverwrite);
  const resolveConvenienceEdit = useChatStore((state) => state.resolveConvenienceEdit);
  const rollbackLastConvenienceEdit = useChatStore((state) => state.rollbackLastConvenienceEdit);
  const chatConvenienceAuthorization = useSettingsStore((state) => state.chatConvenienceAuthorization);
  const chatConvenienceSaving = useSettingsStore((state) => state.chatConvenienceSaving);
  const ensureChatConvenienceAuthorization = useSettingsStore((state) => state.ensureChatConvenienceAuthorization);
  const setChatConvenienceAuthorization = useSettingsStore((state) => state.setChatConvenienceAuthorization);

  const { projectId, rootPath } = currentProject;
  const relevantTaskIds = useMemo(() => [...new Set([
    ...(sendTaskId ? [sendTaskId] : []),
    ...(activeSession?.messages.flatMap((message) => message.taskId ? [message.taskId] : []) ?? []),
  ])], [activeSession?.messages, sendTaskId]);
  const tasks = useTaskStore(useShallow((state) =>
    relevantTaskIds
      .map((taskId) => selectProjectTaskById(state, projectId, taskId))
      .filter((task): task is NonNullable<typeof task> => Boolean(task)),
  ));
  const activities = useTaskStore(useShallow((state) => Object.fromEntries(
    relevantTaskIds.flatMap((taskId) => state.activities[taskId]
      ? [[taskId, state.activities[taskId]]]
      : []),
  )));
  const openTaskDrawer = useTaskStore((state) => state.openDrawer);
  const setActiveView = useNavigationStore((state) => state.setActiveView);
  const openSettings = useNavigationStore((state) => state.openSettings);
  const openWikiPage = useWikiStore((state) => state.openPage);

  const presentationProjectKey = projectResourceKey(projectId, rootPath);
  const setRoutePreference = useCallback((route: ChatRoutePreference) => {
    saveChatRoutePreference(route);
    setRoutePreferenceState(route);
  }, []);
  const errorPresentation = error ? normalizeBackendError(error, {
    defaultSummaryKey: "backendError.summary.chat",
    defaultActionKind: null,
    defaultRecoverable: true,
  }) : null;
  const sendTask = sendTaskId ? tasks.find((task) => task.id === sendTaskId) ?? null : null;
  const canHandleErrorAction = (
    errorPresentation?.actionKind === "retry"
    && Boolean(sendTask && isTerminalStatus(sendTask.status))
  ) || errorPresentation?.actionKind === "reauthorize"
    || errorPresentation?.actionKind === "open_settings";

  useEffect(() => {
    activateRoutePresentationProject(presentationProjectKey);
    setRoutePreferenceState(readChatRoutePreference());
  }, [presentationProjectKey]);
  const generating =
    sendSessionId === activeSessionId &&
    (sendTask?.status === "running" ||
      sendTask?.status === "queued" ||
      sendTask?.status === "cancelling");
  const chatBusy = Boolean(
    sendStarting ||
      (sendTask &&
      (sendTask.status === "running" ||
        sendTask.status === "queued" ||
        sendTask.status === "cancelling" ||
        sendTask.status === "cancelled")),
  );
  const convenienceBusy = Boolean(convenienceMutationKey || chatConvenienceSaving);
  const saveBlocked = Boolean(saveInFlightMessageId || overwriteRequest || convenienceBusy);
  const destructiveActionBlocked = Boolean(
    chatBusy || overwriteRequest || convenienceBusy,
  );
  const pendingUserMessage =
    sendTaskId && sendSessionId === activeSessionId && chatBusy
      ? pendingUserMessages[sendTaskId] ?? null
      : null;
  const streamActivities = sendTaskId ? activities[sendTaskId] ?? [] : [];
  const activityTaskIds = activeSession?.messages
    .map((message) => message.taskId)
    .filter((taskId): taskId is string => Boolean(taskId))
    .join("|") ?? "";
  const transcriptScroll = useTranscriptScroll(
    activeSessionId,
    activeSession?.messages.length ?? 0,
    streamRevision,
    streamActivities.length,
    presentationProjectKey,
  );

  useEffect(() => {
    const unobserve = observeProjectResources(
      { projectId, rootPath },
      ["chat-sessions", "settings-chat-authorization"],
    );
    void ensureSessions(projectId, rootPath);
    void ensureChatConvenienceAuthorization(projectId, rootPath);
    return unobserve;
  }, [projectId, rootPath, ensureSessions, ensureChatConvenienceAuthorization]);

  useEffect(() => {
    const taskIds = activityTaskIds ? activityTaskIds.split("|") : [];
    taskIds.forEach((taskId) => void fetchTaskActivities(taskId));
  }, [activityTaskIds]);

  // When the send task reaches a terminal status, reload the session to surface
  // the persisted message, then clear the in-flight id without discarding a
  // backend failure that the user needs in order to recover.
  useEffect(() => {
    if (!sendTask || !isTerminalStatus(sendTask.status)) return;
    let cancelled = false;
    const terminalError = sendSessionId === activeSessionId
      ? (sendTask.error ?? (sendTask.status === "failed" ? {
        code: "CHAT_SEND_FAILED",
        message: sendTask.title,
        details: null,
        recoverable: true,
        userActionRequired: false,
      } : null))
      : null;
    void reloadActive(projectId, rootPath).then((reloaded) => {
      if (!cancelled && reloaded) clearSendTask(terminalError);
    });
    return () => {
      cancelled = true;
    };
  }, [sendTask, sendSessionId, activeSessionId, projectId, rootPath, reloadActive, clearSendTask]);

  const handleSend = async (content: string): Promise<boolean> => {
    if (chatBusy) return false;
    let sessionId = activeSessionId;
    if (!sessionId) {
      const created = await createSession(projectId, rootPath);
      // A project/page switch can invalidate creation after the IPC returns;
      // never combine the old request scope with a newly active session.
      if (!created || useChatStore.getState().activeSessionId !== created.id) {
        return false;
      }
      sessionId = created.id;
    }
    if (!sessionId) return false;
    const canUseConvenience = routePreference !== "byok";
    const taskId = await send(projectId, rootPath, sessionId, content, routePreference, {
      convenienceEnabled: Boolean(
        canUseConvenience && chatConvenienceAuthorization?.enabled && !hasPendingConvenienceEdit(activeSession),
      ),
    });
    return Boolean(taskId);
  };

  const handleCancel = () => {
    if (!sendTaskId) return;
    void cancelChatTask(sendTaskId);
  };

  const latestAssistant = latestAssistantMessage(activeSession);
  const resolvedRoute = latestAssistant?.route ?? null;
  const convenienceEnabled = Boolean(chatConvenienceAuthorization?.enabled);
  const conveniencePending = hasPendingConvenienceEdit(activeSession);

  const openCitation = (path: string) => {
    setActiveView("wiki");
    void openWikiPage(projectId, rootPath, path);
  };

  const setConvenienceEnabled = (enabled: boolean) => {
    if (enabled && !chatConvenienceAuthorization?.enabled) {
      const ok = window.confirm(t("chat.convenience.confirmEnable"));
      if (!ok) return;
    }
    void setChatConvenienceAuthorization(projectId, rootPath, enabled);
  };

  return (
    <div className="chat-view-layout">
      <div className="border-r border-[var(--border)] bg-[var(--surface)]">
        <ChatSessionList
          sessions={sessions}
          activeSessionId={activeSessionId}
          loading={loadingSessions}
          onSelect={(sessionId) => void selectSession(projectId, rootPath, sessionId)}
          createDisabled={creatingSession || destructiveActionBlocked}
          onCreate={() => {
            if (creatingSession || destructiveActionBlocked) return;
            setCreatingSession(true);
            void createSession(projectId, rootPath).finally(() => setCreatingSession(false));
          }}
          onRename={(sessionId, title) => void renameSession(projectId, rootPath, sessionId, title)}
          deleteDisabled={destructiveActionBlocked}
          deleteDisabledSessionId={destructiveActionBlocked ? sendSessionId : null}
          onDelete={(sessionId) => {
            if (window.confirm(t("chat.sessions.deleteConfirm"))) {
              void deleteSession(projectId, rootPath, sessionId);
            }
          }}
        />
      </div>

      <div className="chat-stream-wrap flex min-h-0 min-w-0 flex-col overflow-hidden">
        {error ? (
          <ActionableErrorNotice
            className="rounded-none border-x-0 border-t-0 px-4"
            error={error}
            onAction={canHandleErrorAction ? async (kind) => {
              if (kind === "reauthorize") {
                openSettings("ai");
                return;
              }
              if (kind === "open_settings") {
                openSettings(isAiConfigurationErrorCode(errorPresentation?.code ?? null) ? "ai" : "security");
                return;
              }
              if (sendTask && isTerminalStatus(sendTask.status)) {
                const reloaded = await reloadActive(projectId, rootPath);
                if (reloaded) clearSendTask(null);
              }
            } : undefined}
          />
        ) : null}
        {activeSession ? (
          <SessionToolbar
            session={activeSession}
            routePreference={routePreference}
            onRouteChange={setRoutePreference}
            convenienceEnabled={convenienceEnabled}
            conveniencePending={conveniencePending}
            onConvenienceEnabledChange={setConvenienceEnabled}
            convenienceBusy={convenienceBusy}
            deleteDisabled={destructiveActionBlocked}
            onRollbackLast={() => {
              if (!activeSessionId) return;
              void rollbackLastConvenienceEdit(projectId, rootPath, activeSessionId);
            }}
            t={t}
          />
        ) : null}
        <div
          ref={transcriptScroll.ref}
          onScroll={transcriptScroll.onScroll}
          className="chat-scroll-region relative min-h-0 flex-1 overflow-y-auto px-6 py-4"
          role="log"
          aria-label={t("chat.thread.transcriptLabel")}
          aria-busy={generating}
        >
          {loadingSession ? (
            <div className="mx-auto flex w-full max-w-[820px] flex-col gap-3 px-4 py-3" aria-busy="true">
              <div className="h-3 w-2/5 animate-pulse rounded bg-[var(--surface-muted)]" />
              <div className="h-16 w-4/5 animate-pulse rounded-[var(--radius-md)] bg-[var(--surface-muted)]" />
              <div className="h-3 w-1/3 animate-pulse rounded bg-[var(--surface-muted)]" />
            </div>
          ) : !activeSession ? (
            <div className="flex h-full items-center justify-center text-[13px] text-[var(--text-muted)]">
              {t("chat.thread.empty")}
            </div>
          ) : (
            <div className="chat-stream mx-auto w-full max-w-[820px] px-4">
              {[...activeSession.messages, ...(pendingUserMessage ? [pendingUserMessage] : [])].map((message) => (
                <MessageBubble
                  key={message.id}
                  message={message}
                  activities={message.taskId ? activities[message.taskId] ?? [] : []}
                  taskStatus={message.taskId ? tasks.find((task) => task.id === message.taskId)?.status : undefined}
                  t={t}
                  generating={generating}
                  saveStatus={saveStatus[message.id] ?? "idle"}
                  saveBlocked={saveBlocked}
                  convenienceBusy={convenienceBusy}
                  savedPath={message.savedPath ?? savedAnswerPaths[message.id]}
                  onCitationClick={(ref) => {
                    const citation = resolveCitationRef(message.citations ?? [], ref);
                    if (citation) openCitation(citation.pagePath);
                  }}
                  onOpenCitation={(path) => openCitation(path)}
                  onSave={() => {
                    if (!activeSessionId) return;
                    void saveAnswer(projectId, rootPath, activeSessionId, message.id);
                  }}
                  onKeepConvenience={() => {
                    if (!activeSessionId) return;
                    void resolveConvenienceEdit(projectId, rootPath, activeSessionId, message.id, true);
                  }}
                  onRollbackConvenience={() => {
                    if (!activeSessionId) return;
                    void resolveConvenienceEdit(projectId, rootPath, activeSessionId, message.id, false);
                  }}
                />
              ))}
              {generating ? (
                <StreamingBubble
                  text={streamingText}
                  activities={streamActivities}
                  taskStatus={sendTask?.status}
                  routeLabel={streamingRoute ? t(`chat.composer.route.${streamingRoute}`) : null}
                  agentLabel={t("chat.thread.agent")}
                  placeholder={t("chat.thread.generating")}
                  onOpenLogs={() => openTaskDrawer(sendTaskId ?? undefined)}
                  openLogsLabel={t("chat.thread.openLogs")}
                />
              ) : null}
            </div>
          )}
          <div className="sr-only" aria-live="polite">
            {generating ? t("chat.composer.busy") : ""}
          </div>
          {transcriptScroll.showBackToLatest ? (
            <button
              type="button"
              className="chat-back-latest"
              onClick={transcriptScroll.backToLatest}
            >
              <ChevronDown size={14} aria-hidden="true" />
              {t("chat.thread.backToLatest")}
            </button>
          ) : null}
        </div>
        {overwriteRequest ? (
          <ChatOverwritePrompt
            request={overwriteRequest}
            busy={Boolean(saveInFlightMessageId)}
            onConfirm={() => void confirmOverwrite(projectId, rootPath)}
            onCancel={() => void cancelOverwrite()}
            t={t}
          />
        ) : null}
        <ChatComposer
          routePreference={routePreference}
          lastResolvedRoute={resolvedRoute}
          generating={generating}
          onSend={handleSend}
          onCancel={handleCancel}
          draftKey={activeSessionId ? `${projectId}:chat:${activeSessionId}` : `${projectId}:chat:new`}
          blocked={chatBusy || saveBlocked || convenienceBusy}
        />
      </div>
    </div>
  );
}

export interface MessageBubbleProps {
  message: ChatMessage;
  activities?: TaskActivity[];
  taskStatus?: TaskStatus;
  t: (k: string, opts?: Record<string, unknown>) => string;
  generating: boolean;
  saveStatus: "idle" | "saving" | "saved" | "exists" | "error";
  saveBlocked?: boolean;
  convenienceBusy?: boolean;
  savedPath?: string | null;
  onCitationClick: (ref: string) => void;
  onOpenCitation: (path: string) => void;
  onSave: () => void;
  onKeepConvenience?: () => void;
  onRollbackConvenience?: () => void;
}

export interface ChatOverwritePromptProps {
  request: { path: string };
  onConfirm: () => void;
  onCancel: () => void;
  busy?: boolean;
  t: (k: string, opts?: Record<string, unknown>) => string;
}

export function ChatOverwritePrompt({ request, onConfirm, onCancel, busy = false, t }: ChatOverwritePromptProps) {
  return (
    <div
      className="flex flex-col gap-2 rounded-[var(--radius-md)] border border-[var(--warning)] bg-[var(--warning-soft)] p-3"
      role="alert"
    >
      <div className="text-[12px] font-medium">{t("chat.thread.overwriteTitle")}</div>
      <p className="m-0 text-[11.5px] text-[var(--text-secondary)]">
        {t("chat.thread.overwriteBody", { path: request.path })}
      </p>
      <div className="flex gap-2">
        <button
          type="button"
          disabled={busy}
          onClick={onConfirm}
          className="h-[28px] rounded-[var(--radius-md)] bg-[var(--foreground)] px-3 text-[12px] font-medium text-[var(--text-inverse)] hover:bg-[var(--primary-hover)] disabled:cursor-wait disabled:opacity-50"
        >
          {t("chat.thread.overwrite")}
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={onCancel}
          className="h-[28px] rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] hover:bg-[var(--surface-muted)] disabled:cursor-not-allowed disabled:opacity-50"
        >
          {t("chat.thread.cancel")}
        </button>
      </div>
    </div>
  );
}

export function MessageBubble({
  message,
  activities = [],
  taskStatus,
  t,
  generating,
  saveStatus,
  saveBlocked = false,
  convenienceBusy = false,
  savedPath,
  onCitationClick,
  onOpenCitation,
  onSave,
  onKeepConvenience,
  onRollbackConvenience,
}: MessageBubbleProps) {
  const isUser = message.role === "user";
  const citations = message.citations ?? [];
  const diagnostics = message.retrievalDiagnostics;
  const invalidCitationIds = diagnostics?.invalidCitationIds ?? [];
  const time = formatTime(message.createdAt);
  const routeLabel = message.route ? t(`chat.composer.route.${message.route}`) : null;
  const providerLabel = message.provider ? t(`provider.name.${message.provider}`) : null;

  return (
    <div className={`msg ${isUser ? "msg--user" : ""}`}>
      <div className={`msg__avatar ${isUser ? "msg__avatar--user" : "msg__avatar--ai"}`}>
        {isUser ? t("chat.thread.youShort") : t("chat.thread.assistantShort")}
      </div>
      <div className="msg__body">
        <div className="msg__head">
          <span className="msg__name">{isUser ? t("chat.thread.you") : t("chat.thread.assistant")}</span>
          {time ? <span className="msg__time">{time}</span> : null}
          {!isUser && routeLabel ? (
            <span className="msg__route-badge">
              {providerLabel ? `${routeLabel} · ${providerLabel}` : routeLabel}
            </span>
          ) : null}
        </div>
        {isUser ? (
          <div className="msg__bubble-user">{message.content}</div>
        ) : (
          <>
            <AgentActivityTimeline activities={activities} taskStatus={taskStatus} compact />
            <MessageContent
              content={message.content}
              citationCount={citations.length}
              citationIds={citations.flatMap((citation) => citation.sourceId ?? [])}
              onCitationClick={onCitationClick}
            />
            {invalidCitationIds.length > 0 || diagnostics?.hasUnverified ? (
              <div
                className="mt-2 flex flex-col gap-1 rounded-[var(--radius-sm)] border border-[var(--warning)] bg-[var(--warning-soft)] px-2.5 py-2 text-[11px] text-[var(--text-secondary)]"
                role="status"
              >
                {invalidCitationIds.length > 0 ? (
                  <span>
                    {t("chat.trust.invalidCitations", {
                      ids: invalidCitationIds.join(", "),
                    })}
                  </span>
                ) : null}
                {diagnostics?.hasUnverified ? (
                  <span>{t("chat.trust.unverified")}</span>
                ) : null}
              </div>
            ) : null}
            {!isUser && citations.length > 0 ? (
              <div className="msg__citations">
                {citations.map((citation, index) => (
                  <button
                    key={citation.pagePath}
                    type="button"
                    className="msg__citation"
                    onClick={() => onOpenCitation(citation.pagePath)}
                    title={t("chat.citations.openPage")}
                  >
                    <span className="msg__citation-idx">{citation.sourceId ?? index + 1}</span>
                    <span className="msg__citation-title">{citation.title}</span>
                    {citation.isPinned ? (
                      <span className="rounded-[var(--radius-sm)] bg-[var(--accent-soft)] px-1.5 py-0.5 text-[10px] font-medium text-[var(--accent-hover)]">
                        {t("chat.citations.currentPage")}
                      </span>
                    ) : null}
                    <span className="msg__citation-path">{citation.pagePath}</span>
                  </button>
                ))}
              </div>
            ) : null}
            {!isUser ? (
              <div className="mt-2 flex items-center gap-2">
                <SaveAnswerButton
                  status={saveStatus}
                  savedPath={savedPath}
                  disabled={generating || saveBlocked}
                  onSave={onSave}
                />
              </div>
            ) : null}
            {message.convenienceEdit ? (
              <ChatConveniencePanel
                enabled
                pending={message.convenienceEdit.status === "soft_violation_pending"}
                edit={message.convenienceEdit}
                onSetEnabled={() => {}}
                onKeep={onKeepConvenience}
                onRollback={onRollbackConvenience}
                busy={convenienceBusy}
              />
            ) : null}
          </>
        )}
      </div>
    </div>
  );
}

function resolveCitationRef(citations: NonNullable<ChatMessage["citations"]>, ref: string) {
  const normalized = ref.toUpperCase();
  const bySourceId = citations.find(
    (citation) => citation.sourceId?.toUpperCase() === normalized,
  );
  if (bySourceId) return bySourceId;
  const index = Number.parseInt(ref, 10);
  return Number.isFinite(index) ? citations[index - 1] : undefined;
}

interface SessionToolbarProps {
  session: { id: string; title: string; messages: unknown[]; updatedAt: string };
  routePreference: ChatRoutePreference;
  onRouteChange: (value: ChatRoutePreference) => void;
  convenienceEnabled: boolean;
  conveniencePending: boolean;
  onConvenienceEnabledChange: (enabled: boolean) => void;
  deleteDisabled: boolean;
  convenienceBusy: boolean;
  onRollbackLast: () => void;
  t: (k: string, opts?: Record<string, unknown>) => string;
}

function SessionToolbar({
  session,
  routePreference,
  onRouteChange,
  convenienceEnabled,
  conveniencePending,
  onConvenienceEnabledChange,
  deleteDisabled,
  convenienceBusy,
  onRollbackLast,
  t,
}: SessionToolbarProps) {
  const rename = useChatStore((state) => state.renameSession);
  const del = useChatStore((state) => state.deleteSession);
  const { projectId, rootPath } = useProjectStore((state) => state.currentProject);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");

  const msgCount = session.messages.length;
  const time = formatTime(session.updatedAt);

  const commitRename = () => {
    const trimmed = draft.trim();
    if (trimmed && trimmed !== session.title) {
      void rename(projectId, rootPath, session.id, trimmed);
    }
    setEditing(false);
  };

  const handleEdit = () => {
    setDraft(session.title);
    setEditing(true);
  };

  return (
    <div className="view-toolbar border-b border-[var(--border-subtle)] px-4">
      <div className="chat-route-seg" role="group" aria-label={t("chat.composer.route.label")}>
        {SEGMENT_OPTIONS.map((opt) => (
          <button
            key={opt.value}
            type="button"
            className={`chat-route-seg__btn${routePreference === opt.value ? " is-active" : ""}`}
            aria-pressed={routePreference === opt.value}
            onClick={() => onRouteChange(opt.value)}
          >
            {t(opt.key)}
          </button>
        ))}
      </div>
      <ChatConveniencePanel
        enabled={convenienceEnabled}
        pending={conveniencePending}
        onSetEnabled={onConvenienceEnabledChange}
        onRollbackLast={convenienceEnabled ? onRollbackLast : undefined}
        busy={convenienceBusy}
      />
      <div className="flex min-w-0 flex-1 items-center gap-2">
        {editing ? (
          <input
            autoFocus
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onBlur={commitRename}
            onKeyDown={(e) => {
              if (e.key === "Enter") commitRename();
              if (e.key === "Escape") setEditing(false);
            }}
            className="h-[24px] min-w-0 flex-1 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--background)] px-2 text-[12.5px] font-semibold"
          />
        ) : (
          <span className="truncate text-[12.5px] font-semibold text-[var(--text-primary)]">
            {session.title}
          </span>
        )}
        <span className="font-mono text-[11px] text-[var(--text-muted)]">
          {msgCount} · {time}
        </span>
      </div>
      <div className="flex items-center gap-1">
        <button
          type="button"
          onClick={handleEdit}
          className="icon-button"
          aria-label={t("chat.sessions.rename")}
          title={t("chat.sessions.rename")}
        >
          <Pencil aria-hidden="true" size={14} />
        </button>
        <button
          type="button"
          disabled={deleteDisabled}
          onClick={() => {
            if (window.confirm(t("chat.sessions.deleteConfirm"))) {
              void del(projectId, rootPath, session.id);
            }
          }}
          className="icon-button hover:text-[var(--danger)]"
          aria-label={t("chat.sessions.delete")}
          title={t("chat.sessions.delete")}
        >
          <Trash2 aria-hidden="true" size={14} />
        </button>
      </div>
    </div>
  );
}

export interface StreamingBubbleProps {
  text: string;
  activities: TaskActivity[];
  taskStatus?: TaskStatus;
  routeLabel: string | null;
  agentLabel: string;
  placeholder: string;
  onOpenLogs: () => void;
  openLogsLabel: string;
}

export function StreamingBubble({ text, activities, taskStatus, routeLabel, agentLabel, placeholder, onOpenLogs, openLogsLabel }: StreamingBubbleProps) {
  return (
    <div className="chat-transcript-item">
      <div className="chat-agent-header">
        <span className="chat-agent-mark" aria-hidden="true"><Sparkles size={15} /></span>
        <span className="chat-agent-name">{routeLabel ?? agentLabel}</span>
        <span className="chat-agent-live">{placeholder}</span>
        <button
          type="button"
          onClick={onOpenLogs}
          className="ml-auto text-[11px] text-[var(--accent-hover)] hover:underline"
        >
          {openLogsLabel}
        </button>
      </div>
      <AgentActivityTimeline activities={activities} taskStatus={taskStatus} />
      <div className="chat-agent-answer">
        {text ? (
          <div className="chat-streaming-text">
            <span>{text}</span>
            <span className="stream-cursor" aria-hidden="true" />
          </div>
        ) : (
          <div className="flex items-center gap-2 text-[12px] text-[var(--text-muted)]">
            <span>{placeholder}</span>
            <span className="stream-cursor" aria-hidden="true" />
          </div>
        )}
      </div>
    </div>
  );
}

export function useTranscriptScroll(
  sessionId: string | null,
  messageCount: number,
  streamRevision: number,
  activityCount: number,
  presentationProjectKey?: string,
) {
  const ref = useRef<HTMLDivElement>(null);
  const [isPinned, setIsPinned] = useState(true);
  const pinnedRef = useRef(true);
  const scrollTopRef = useRef(0);
  const scheduledFrameRef = useRef<{ id: number; kind: "raf" | "timeout" } | null>(null);

  const cancelScheduledScroll = useCallback(() => {
    const scheduled = scheduledFrameRef.current;
    if (!scheduled) return;
    if (scheduled.kind === "raf" && typeof window.cancelAnimationFrame === "function") {
      window.cancelAnimationFrame(scheduled.id);
    } else {
      window.clearTimeout(scheduled.id);
    }
    scheduledFrameRef.current = null;
  }, []);

  const setPinned = useCallback((nextPinned: boolean) => {
    if (pinnedRef.current === nextPinned) return;
    pinnedRef.current = nextPinned;
    setIsPinned(nextPinned);
  }, []);

  const onScroll = useCallback(() => {
    const element = ref.current;
    if (!element) return;
    scrollTopRef.current = element.scrollTop;
    const distance = element.scrollHeight - element.scrollTop - element.clientHeight;
    setPinned(distance < 72);
  }, [setPinned]);

  const schedulePinnedScroll = useCallback(() => {
    if (!pinnedRef.current || scheduledFrameRef.current) return;
    const scroll = () => {
      scheduledFrameRef.current = null;
      const element = ref.current;
      if (element && pinnedRef.current) {
        element.scrollTop = element.scrollHeight;
        scrollTopRef.current = element.scrollTop;
      }
    };
    if (typeof window.requestAnimationFrame === "function") {
      scheduledFrameRef.current = {
        id: window.requestAnimationFrame(scroll),
        kind: "raf",
      };
    } else {
      scheduledFrameRef.current = {
        id: window.setTimeout(scroll, 16),
        kind: "timeout",
      };
    }
  }, []);

  useEffect(() => {
    cancelScheduledScroll();
    const element = ref.current;
    const routeKey = `chat:transcript:${sessionId ?? "empty"}`;
    const restored = presentationProjectKey
      ? (() => {
          activateRoutePresentationProject(presentationProjectKey);
          return readRouteScrollPosition(presentationProjectKey, routeKey);
        })()
      : null;

    if (element && restored !== null) {
      element.scrollTop = restored;
      scrollTopRef.current = element.scrollTop;
      const distance = element.scrollHeight - element.scrollTop - element.clientHeight;
      setPinned(distance < 72);
    } else {
      pinnedRef.current = true;
      setIsPinned((current) => (current ? current : true));
      if (element) {
        element.scrollTop = element.scrollHeight;
        scrollTopRef.current = element.scrollTop;
      }
    }

    return () => {
      cancelScheduledScroll();
      if (presentationProjectKey) {
        saveRouteScrollPosition(
          presentationProjectKey,
          routeKey,
          ref.current?.scrollTop ?? scrollTopRef.current,
        );
      }
    };
  }, [sessionId, presentationProjectKey, cancelScheduledScroll, setPinned]);

  useEffect(() => {
    if (isPinned) schedulePinnedScroll();
  }, [isPinned, messageCount, streamRevision, activityCount, schedulePinnedScroll]);

  return {
    ref,
    onScroll,
    showBackToLatest: !isPinned,
    backToLatest: () => {
      const element = ref.current;
      if (!element) return;
      cancelScheduledScroll();
      element.scrollTo({ top: element.scrollHeight, behavior: "smooth" });
      scrollTopRef.current = element.scrollHeight;
      setPinned(true);
    },
  };
}

interface SaveAnswerButtonProps {
  status: "idle" | "saving" | "saved" | "exists" | "error";
  savedPath?: string | null;
  disabled: boolean;
  onSave: () => void;
}

function SaveAnswerButton({ status, savedPath, disabled, onSave }: SaveAnswerButtonProps) {
  const { t } = useTranslation();
  if (status === "saved" || savedPath) {
    return (
      <span className="max-w-full truncate text-[10.5px] text-[var(--text-muted)]" title={savedPath ?? undefined}>
        {savedPath ? t("chat.thread.savePath", { path: savedPath }) : t("chat.thread.saveDone")}
      </span>
    );
  }
  return (
    <button
      type="button"
      onClick={onSave}
      disabled={disabled || status === "saving"}
      className="h-[22px] rounded-[var(--radius-sm)] px-2 text-[10.5px] font-medium text-[var(--accent-hover)] hover:bg-[var(--surface-muted)] disabled:opacity-40"
    >
      {status === "saving" ? "…" : t("chat.thread.saveAnswer")}
    </button>
  );
}
