import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { BookOpenText, Plus } from "lucide-react";

import { latestAssistantMessage, useChatStore } from "../../stores/chatStore";
import { useTaskStore } from "../../stores/taskStore";
import type { ChatMessage } from "../../types/chat";
import { isTerminalStatus } from "../../types/task";
import type { WikiPageContent } from "../../types/wiki";
import { ChatComposer } from "./ChatComposer";
import { MessageBubble, StreamingBubble } from "./ChatView";

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
  const activeSessionId = useChatStore((state) => state.activeSessionId);
  const activeSession = useChatStore((state) => state.activeSession);
  const sendTaskId = useChatStore((state) => state.sendTaskId);
  const clearSendTask = useChatStore((state) => state.clearSendTask);
  const error = useChatStore((state) => state.error);
  const streamingText = useChatStore((state) => state.streamingText);
  const streamingRoute = useChatStore((state) => state.streamingRoute);
  const createSession = useChatStore((state) => state.createSession);
  const ensurePageSession = useChatStore((state) => state.ensurePageSession);
  const send = useChatStore((state) => state.send);
  const saveStatus = useChatStore((state) => state.saveStatus);
  const saveAnswer = useChatStore((state) => state.saveAnswer);
  const reloadActive = useChatStore((state) => state.reloadActive);

  const tasks = useTaskStore((state) => state.tasks);
  const upsertTask = useTaskStore((state) => state.upsertTask);
  const openTaskDrawer = useTaskStore((state) => state.openDrawer);

  const sendTask = sendTaskId ? tasks.find((task) => task.id === sendTaskId) ?? null : null;
  const generating =
    sendTask?.status === "running" ||
    sendTask?.status === "queued" ||
    sendTask?.status === "cancelling";
  const latestAssistant = latestAssistantMessage(activeSession);
  const pinnedCitation = latestAssistant?.citations?.find(
    (citation) => citation.isPinned && citation.pagePath === page?.meta.path,
  );

  useEffect(() => {
    if (!sendTask || !isTerminalStatus(sendTask.status)) return;
    let cancelled = false;
    const terminalError =
      sendTask.status === "failed" ? (sendTask.error?.message ?? sendTask.title) : null;
    void reloadActive(projectId, rootPath).finally(() => {
      if (!cancelled) clearSendTask(terminalError);
    });
    return () => {
      cancelled = true;
    };
  }, [sendTask, projectId, rootPath, reloadActive, clearSendTask]);

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
    if (!page) return false;
    let sessionId = activeSessionId;
    if (!sessionId) {
      const created = await createSession(
        projectId,
        rootPath,
        `Ask: ${page.meta.title}`,
        page.meta.path,
      );
      sessionId = created?.id ?? useChatStore.getState().activeSessionId;
    }
    if (!sessionId) return false;
    const taskId = await send(projectId, rootPath, sessionId, content, "auto", {
      pinnedPagePath: page.meta.path,
    });
    return Boolean(taskId);
  };

  const handleNewPageChat = () => {
    if (!page) return;
    void ensurePageSession(projectId, rootPath, page.meta.path, page.meta.title, true);
  };

  const handleCancel = () => {
    if (!sendTaskId) return;
    void invoke("cancel_task", { request: { taskId: sendTaskId } }).then((task) => {
      if (task) upsertTask(task as never);
    });
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
        <div className="border-b border-[var(--border-subtle)] bg-[var(--warning-soft)] px-4 py-2 text-[12px] text-[var(--text-primary)]">
          {error}
        </div>
      ) : null}
      <div className="page-chat__body min-h-0 flex-1 overflow-y-auto px-3 py-3" role="log" aria-live="polite">
        {!activeSession ? (
          <div className="flex h-full items-center justify-center text-center text-[12px] text-[var(--text-muted)]">
            {t("wiki.askAi.currentPage")}
          </div>
        ) : (
          <div className="chat-stream w-full">
            {activeSession.messages.map((message) => (
              <MessageBubble
                key={message.id}
                message={message}
                t={t}
                generating={generating}
                saveStatus={saveStatus[message.id] ?? "idle"}
                onCitationClick={(ref) => {
                  const citation = resolveCitationRef(message.citations ?? [], ref);
                  if (citation && onOpenCitation) onOpenCitation(citation.pagePath);
                }}
                onOpenCitation={(path) => {
                  if (onOpenCitation) onOpenCitation(path);
                }}
                onSave={() => {
                  if (!activeSessionId) return;
                  void saveAnswer(projectId, rootPath, activeSessionId, message.id);
                }}
              />
            ))}
            {generating ? (
              <StreamingBubble
                text={streamingText}
                routeLabel={streamingRoute ? t(`chat.composer.route.${streamingRoute}`) : null}
                placeholder={t("chat.thread.generating")}
                onOpenLogs={() => openTaskDrawer(sendTaskId ?? undefined)}
                openLogsLabel={t("chat.thread.openLogs")}
              />
            ) : null}
          </div>
        )}
      </div>
      <ChatComposer
        routePreference="auto"
        lastResolvedRoute={latestAssistant?.route ?? null}
        generating={generating}
        onSend={handleSend}
        onCancel={handleCancel}
        placeholderKey="wiki.askAi.placeholder"
        compact
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
