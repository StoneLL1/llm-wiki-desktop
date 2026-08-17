import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { BookOpenText, ChevronDown, Plus } from "lucide-react";

import { latestAssistantMessage, useChatStore } from "../../stores/chatStore";
import { useNavigationStore } from "../../stores/navigationStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { fetchTaskActivities, useTaskStore } from "../../stores/taskStore";
import type { ChatMessage } from "../../types/chat";
import { isTerminalStatus } from "../../types/task";
import type { WikiPageContent } from "../../types/wiki";
import { ChatComposer } from "./ChatComposer";
import { ActionableErrorNotice } from "../../components/app/ActionableErrorNotice";
import { isAiConfigurationErrorCode, normalizeBackendError } from "../../lib/backendError";
import {
  ChatOverwritePrompt,
  MessageBubble,
  StreamingBubble,
  useTranscriptScroll,
} from "./ChatView";

interface PageChatPanelProps {
  page: WikiPageContent | null;
  projectId: string;
  rootPath: string;
  onShowRelatedPages?: () => void;
  /** Open a cited wiki page (e.g. navigate the wiki view to it). When
   *  unset, citation clicks are no-ops. */
  onOpenCitation?: (path: string) => void;
}

export function PageChatPanel({
  page,
  projectId,
  rootPath,
  onShowRelatedPages,
  onOpenCitation,
}: PageChatPanelProps) {
  const { t } = useTranslation();
  const [creatingPageSession, setCreatingPageSession] = useState(false);
  const activeSessionId = useChatStore((state) => state.activeSessionId);
  const activeSession = useChatStore((state) => state.activeSession);
  const loadingSession = useChatStore((state) => state.loadingSession);
  const sendTaskId = useChatStore((state) => state.sendTaskId);
  const sendSessionId = useChatStore((state) => state.sendSessionId);
  const sendStarting = useChatStore((state) => state.sendStarting);
  const clearSendTask = useChatStore((state) => state.clearSendTask);
  const error = useChatStore((state) => state.error);
  const overwriteRequest = useChatStore((state) => state.overwriteRequest);
  const streamingText = useChatStore((state) => state.streamingText);
  const streamingRoute = useChatStore((state) => state.streamingRoute);
  const streamRevision = useChatStore((state) => state.streamRevision);
  const pendingUserMessages = useChatStore((state) => state.pendingUserMessages);
  const ensurePageSession = useChatStore((state) => state.ensurePageSession);
  const send = useChatStore((state) => state.send);
  const cancelChatTask = useChatStore((state) => state.cancelTask);
  const saveStatus = useChatStore((state) => state.saveStatus);
  const saveInFlightMessageId = useChatStore((state) => state.saveInFlightMessageId);
  const convenienceMutationKey = useChatStore((state) => state.convenienceMutationKey);
  const chatConvenienceSaving = useSettingsStore((state) => state.chatConvenienceSaving);
  const savedAnswerPaths = useChatStore((state) => state.savedAnswerPaths);
  const saveAnswer = useChatStore((state) => state.saveAnswer);
  const resolveConvenienceEdit = useChatStore((state) => state.resolveConvenienceEdit);
  const reloadActive = useChatStore((state) => state.reloadActive);
  const openSettings = useNavigationStore((state) => state.openSettings);

  const tasks = useTaskStore((state) => state.tasks);
  const activities = useTaskStore((state) => state.activities);
  const openTaskDrawer = useTaskStore((state) => state.openDrawer);

  const sendTask = sendTaskId ? tasks.find((task) => task.id === sendTaskId) ?? null : null;
  const activeSessionMatchesPage =
    Boolean(page) &&
    Boolean(activeSession) &&
    normalizePagePath(activeSession?.contextPagePath ?? "") === normalizePagePath(page?.meta.path ?? "");
  const pageSession = activeSessionMatchesPage ? activeSession : null;
  const pageSessionId = activeSessionMatchesPage ? activeSessionId : null;
  const errorPresentation = error ? normalizeBackendError(error, {
    defaultSummaryKey: "backendError.summary.chat",
    defaultActionKind: null,
    defaultRecoverable: true,
  }) : null;
  const canHandleErrorAction = (
    errorPresentation?.actionKind === "retry"
    && Boolean(sendTask && isTerminalStatus(sendTask.status))
  ) || errorPresentation?.actionKind === "reauthorize"
    || errorPresentation?.actionKind === "open_settings";
  const generating =
    sendSessionId === pageSessionId &&
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
  const pendingUserMessage =
    sendTaskId && sendSessionId === pageSessionId && chatBusy
      ? pendingUserMessages[sendTaskId] ?? null
      : null;
  const latestAssistant = latestAssistantMessage(pageSession);
  const pinnedCitation = !chatBusy && !loadingSession ? latestAssistant?.citations?.find(
    (citation) =>
      citation.isPinned && normalizePagePath(citation.pagePath) === normalizePagePath(page?.meta.path ?? ""),
  ) : undefined;
  const streamActivities = sendTaskId ? activities[sendTaskId] ?? [] : [];
  const activityTaskIds = pageSession?.messages
    .map((message) => message.taskId)
    .filter((taskId): taskId is string => Boolean(taskId))
    .join("|") ?? "";
  const transcriptScroll = useTranscriptScroll(
    pageSessionId,
    pageSession?.messages.length ?? 0,
    streamRevision,
    streamActivities.length,
  );

  useEffect(() => {
    if (!sendTask || !isTerminalStatus(sendTask.status)) return;
    let cancelled = false;
    const terminalError =
      sendSessionId === pageSessionId
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
  }, [sendTask, sendSessionId, pageSessionId, projectId, rootPath, reloadActive, clearSendTask]);

  useEffect(() => {
    const taskIds = activityTaskIds ? activityTaskIds.split("|") : [];
    taskIds.forEach((taskId) => void fetchTaskActivities(taskId));
  }, [activityTaskIds]);

  // Bind a chat session to the current wiki page: reuse an existing
  // page-scoped session if one exists, otherwise create one. This is what
  // stops different wiki pages from sharing a single global chat.
  // Deps are the identity-relevant fields, NOT the whole `page` object — the
  // wiki store returns a fresh page object reference on unrelated re-renders
  // (scroll, frontmatter reparse), and depending on it would re-fire this
  // effect (and a backend list/select round-trip) on every churn.
  useEffect(() => {
    if (!page) return;
    void ensurePageSession(projectId, rootPath, page.meta.path, page.meta.title, false);
  }, [projectId, rootPath, page?.meta.path, page?.meta.title, ensurePageSession]);

  const handleSend = async (content: string): Promise<boolean> => {
    if (!page || chatBusy) return false;
    let sessionId = pageSessionId;
    if (!sessionId) {
      const created = await ensurePageSession(
        projectId,
        rootPath,
        page.meta.path,
        page.meta.title,
        true,
      );
      sessionId = created?.id ?? null;
    }
    if (!sessionId) return false;
    const taskId = await send(projectId, rootPath, sessionId, content, "auto", {
      pinnedPagePath: page.meta.path,
    });
    return Boolean(taskId);
  };

  const handleNewPageChat = () => {
    if (!page || creatingPageSession || chatBusy || convenienceBusy) return;
    setCreatingPageSession(true);
    void ensurePageSession(projectId, rootPath, page.meta.path, page.meta.title, true)
      .finally(() => setCreatingPageSession(false));
  };

  const handleCancel = () => {
    if (!sendTaskId) return;
    void cancelChatTask(sendTaskId);
  };

  if (!page) {
    return (
      <div className="flex h-full items-center justify-center px-4 text-center text-[12px] text-[var(--text-muted)]">
        {t("wiki.askAi.empty")}
      </div>
    );
  }

  return (
    <div className="page-chat page-chat-shell flex h-full min-h-0 flex-col">
      <div className="page-chat__head border-b border-[var(--border-subtle)] px-4 py-3">
        <div className="flex items-start gap-2">
          <div className="min-w-0 flex-1">
            <div className="truncate text-[12.5px] font-semibold text-[var(--text-primary)]">
              {page.meta.title}
            </div>
            <div className="mt-1 truncate font-mono text-[10.5px] text-[var(--text-muted)]">
              {page.meta.path}
            </div>
          </div>
          <button
            aria-label={t("chat.sessions.new")}
            className="icon-button shrink-0"
            onClick={handleNewPageChat}
            disabled={creatingPageSession || chatBusy || convenienceBusy}
            title={t("chat.sessions.new")}
            type="button"
          >
            <Plus aria-hidden="true" size={15} />
          </button>
          {onShowRelatedPages ? (
            <button
              aria-label={t("wiki.related.title")}
              className="icon-button shrink-0"
              onClick={onShowRelatedPages}
              title={t("wiki.related.title")}
              type="button"
            >
              <BookOpenText aria-hidden="true" size={15} />
            </button>
          ) : null}
        </div>
        {pinnedCitation ? (
          <div className="mt-2 inline-flex h-[20px] items-center rounded-[var(--radius-sm)] bg-[var(--accent-soft)] px-2 text-[10.5px] font-medium text-[var(--accent-hover)]">
            {t("chat.citations.currentPage")}
          </div>
        ) : null}
      </div>
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
            if (pageSessionId && sendTask && isTerminalStatus(sendTask.status)) {
              const reloaded = await reloadActive(projectId, rootPath);
              if (reloaded) clearSendTask(null);
            }
          } : undefined}
        />
      ) : null}
      <div
        ref={transcriptScroll.ref}
        onScroll={transcriptScroll.onScroll}
        className="page-chat__body app-pane-scrollbar relative min-h-0 flex-1 overflow-y-auto px-3 py-3"
        role="log"
        aria-label={t("chat.thread.transcriptLabel")}
        aria-busy={generating}
      >
        {loadingSession ? (
          <div className="flex h-full flex-col gap-3 px-3 py-3" aria-busy="true">
            <div className="h-3 w-2/5 animate-pulse rounded bg-[var(--surface-muted)]" />
            <div className="h-14 w-4/5 animate-pulse rounded-[var(--radius-md)] bg-[var(--surface-muted)]" />
            <div className="h-3 w-1/3 animate-pulse rounded bg-[var(--surface-muted)]" />
          </div>
        ) : !pageSession ? (
          <div className="flex h-full items-center justify-center text-center text-[12px] text-[var(--text-muted)]">
            {t("wiki.askAi.currentPage")}
          </div>
        ) : (
          <div className="chat-stream w-full">
            {[...pageSession.messages, ...(pendingUserMessage ? [pendingUserMessage] : [])].map((message) => (
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
                  if (citation && onOpenCitation) onOpenCitation(citation.pagePath);
                }}
                onOpenCitation={(path) => {
                  if (onOpenCitation) onOpenCitation(path);
                }}
                onSave={() => {
                  if (!pageSessionId) return;
                  void saveAnswer(projectId, rootPath, pageSessionId, message.id);
                }}
                onKeepConvenience={() => {
                  if (!pageSessionId) return;
                  void resolveConvenienceEdit(projectId, rootPath, pageSessionId, message.id, true);
                }}
                onRollbackConvenience={() => {
                  if (!pageSessionId) return;
                  void resolveConvenienceEdit(projectId, rootPath, pageSessionId, message.id, false);
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
          <button type="button" className="chat-back-latest" onClick={transcriptScroll.backToLatest}>
            <ChevronDown size={14} aria-hidden="true" />
            {t("chat.thread.backToLatest")}
          </button>
        ) : null}
      </div>
      {overwriteRequest ? (
        <ChatOverwritePrompt
          request={overwriteRequest}
          busy={Boolean(saveInFlightMessageId)}
          onConfirm={() => void useChatStore.getState().confirmOverwrite(projectId, rootPath)}
          onCancel={() => void useChatStore.getState().cancelOverwrite()}
          t={t}
        />
      ) : null}
      <ChatComposer
        routePreference="auto"
        lastResolvedRoute={latestAssistant?.route ?? null}
        generating={generating}
        onSend={handleSend}
        onCancel={handleCancel}
        placeholderKey="wiki.askAi.placeholder"
        compact
        draftKey={`${projectId}:page:${page.meta.path}`}
        blocked={chatBusy || saveBlocked || convenienceBusy}
      />
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

function normalizePagePath(path: string): string {
  const normalized = path.replace(/\\/g, "/").trim();
  return typeof navigator !== "undefined" && /windows/i.test(navigator.userAgent)
    ? normalized.toLowerCase()
    : normalized;
}
