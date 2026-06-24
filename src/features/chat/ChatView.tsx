import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";

import { latestAssistantMessage, useChatStore } from "../../stores/chatStore";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import { useNavigationStore } from "../../stores/navigationStore";
import { useWikiStore } from "../../features/wiki/wikiStore";
import { isTerminalStatus } from "../../types/task";
import type { ChatMessage, ChatRoutePreference } from "../../types/chat";
import type { LlmProviderKind } from "../../types/llm";
import { ChatComposer } from "./ChatComposer";
import { ChatSessionList } from "./ChatSessionList";
import { MessageContent } from "./MessageContent";

const SEGMENT_OPTIONS: readonly { value: ChatRoutePreference; key: string }[] = [
  { value: "auto", key: "chat.composer.route.auto" },
  { value: "agent", key: "chat.composer.route.agent" },
  { value: "byok", key: "chat.composer.route.byok" },
];

const PROVIDER_LABEL: Record<LlmProviderKind, string> = {
  open_ai: "OpenAI",
  anthropic: "Anthropic",
  google: "Google",
  ollama: "Ollama",
  custom: "Custom",
};

/** Format an ISO timestamp as HH:MM (locale-independent, 24h). */
function formatTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

export function ChatView() {
  const { t } = useTranslation();
  const currentProject = useProjectStore((state) => state.currentProject);
  const [routePreference, setRoutePreference] = useState<ChatRoutePreference>("auto");

  const sessions = useChatStore((state) => state.sessions);
  const activeSessionId = useChatStore((state) => state.activeSessionId);
  const activeSession = useChatStore((state) => state.activeSession);
  const loadingSessions = useChatStore((state) => state.loadingSessions);
  const sendTaskId = useChatStore((state) => state.sendTaskId);
  const clearSendTask = useChatStore((state) => state.clearSendTask);
  const saveStatus = useChatStore((state) => state.saveStatus);
  const overwriteRequest = useChatStore((state) => state.overwriteRequest);
  const error = useChatStore((state) => state.error);
  const streamingText = useChatStore((state) => state.streamingText);
  const streamingRoute = useChatStore((state) => state.streamingRoute);

  const loadSessions = useChatStore((state) => state.loadSessions);
  const createSession = useChatStore((state) => state.createSession);
  const selectSession = useChatStore((state) => state.selectSession);
  const renameSession = useChatStore((state) => state.renameSession);
  const deleteSession = useChatStore((state) => state.deleteSession);
  const send = useChatStore((state) => state.send);
  const reloadActive = useChatStore((state) => state.reloadActive);
  const saveAnswer = useChatStore((state) => state.saveAnswer);
  const confirmOverwrite = useChatStore((state) => state.confirmOverwrite);
  const cancelOverwrite = useChatStore((state) => state.cancelOverwrite);

  const tasks = useTaskStore((state) => state.tasks);
  const openTaskDrawer = useTaskStore((state) => state.openDrawer);
  const cancelTask = useTaskStore((state) => state.upsertTask);
  const setActiveView = useNavigationStore((state) => state.setActiveView);
  const openWikiPage = useWikiStore((state) => state.openPage);

  const { projectId, rootPath } = currentProject;
  const sendTask = sendTaskId ? tasks.find((task) => task.id === sendTaskId) ?? null : null;
  const generating =
    sendTask?.status === "running" ||
    sendTask?.status === "queued" ||
    sendTask?.status === "cancelling";

  useEffect(() => {
    void loadSessions(projectId, rootPath);
  }, [projectId, rootPath, loadSessions]);

  // When the send task reaches a terminal status, reload the session to surface
  // the persisted message, then clear the in-flight id without discarding a
  // backend failure that the user needs in order to recover.
  useEffect(() => {
    if (!sendTask || !isTerminalStatus(sendTask.status)) return;
    let cancelled = false;
    const terminalError = sendTask.status === "failed"
      ? (sendTask.error?.message ?? sendTask.title)
      : null;
    void reloadActive(projectId, rootPath).finally(() => {
      if (!cancelled) clearSendTask(terminalError);
    });
    return () => {
      cancelled = true;
    };
  }, [sendTask, projectId, rootPath, reloadActive, clearSendTask]);

  const handleSend = (content: string) => {
    if (!activeSessionId) return;
    void send(projectId, rootPath, activeSessionId, content, routePreference);
  };

  const handleCancel = () => {
    if (!sendTaskId) return;
    void invoke("cancel_task", { request: { taskId: sendTaskId } }).then((task) => {
      if (task) cancelTask(task as never);
    });
  };

  const latestAssistant = latestAssistantMessage(activeSession);
  const resolvedRoute = latestAssistant?.route ?? null;

  const openCitation = (path: string) => {
    setActiveView("wiki");
    void openWikiPage(projectId, rootPath, path);
  };

  return (
    <div className="chat-view-layout">
      <div className="border-r border-[var(--border)] bg-[var(--surface)]">
        <ChatSessionList
          sessions={sessions}
          activeSessionId={activeSessionId}
          loading={loadingSessions}
          onSelect={(sessionId) => void selectSession(projectId, rootPath, sessionId)}
          onCreate={() => void createSession(projectId, rootPath)}
          onRename={(sessionId, title) => void renameSession(projectId, rootPath, sessionId, title)}
          onDelete={(sessionId) => void deleteSession(projectId, rootPath, sessionId)}
        />
      </div>

      <div className="flex min-w-0 flex-col">
        {error ? (
          <div className="border-b border-[var(--border-subtle)] bg-[var(--warning-soft)] px-4 py-2 text-[12px] text-[var(--text-primary)]">
            {error}
          </div>
        ) : null}
        <div className="min-h-0 flex-1 overflow-y-auto px-6 py-4" role="log" aria-live="polite">
          {!activeSession ? (
            <div className="flex h-full items-center justify-center text-[13px] text-[var(--text-muted)]">
              {t("chat.thread.empty")}
            </div>
          ) : (
            <>
              <SessionToolbar
                session={activeSession}
                routePreference={routePreference}
                onRouteChange={setRoutePreference}
                t={t}
              />
              <div className="chat-stream mx-auto w-full max-w-[820px] px-4">
              {activeSession.messages.map((message) => (
                <MessageBubble
                  key={message.id}
                  message={message}
                  t={t}
                  generating={generating}
                  saveStatus={saveStatus[message.id] ?? "idle"}
                  onCitationClick={(index) => {
                    const citation = message.citations?.[index - 1];
                    if (citation) openCitation(citation.pagePath);
                  }}
                  onOpenCitation={(path) => openCitation(path)}
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
              {overwriteRequest ? (
                <div className="flex flex-col gap-2 rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] p-3">
                  <div className="text-[12px] font-medium">{t("chat.thread.overwriteTitle")}</div>
                  <p className="m-0 text-[11.5px] text-[var(--text-secondary)]">
                    {t("chat.thread.overwriteBody", { path: overwriteRequest.path })}
                  </p>
                  <div className="flex gap-2">
                    <button
                      type="button"
                      onClick={() => void confirmOverwrite(projectId, rootPath)}
                    className="h-[28px] rounded-[var(--radius-md)] bg-[var(--foreground)] px-3 text-[12px] font-medium text-[var(--text-inverse)] hover:bg-[var(--primary-hover)]"
                    >
                      {t("chat.thread.overwrite")}
                    </button>
                    <button
                      type="button"
                      onClick={cancelOverwrite}
                      className="h-[28px] rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] hover:bg-[var(--surface-muted)]"
                    >
                      {t("chat.thread.cancel")}
                    </button>
                  </div>
                </div>
              ) : null}
            </div>
            </>
          )}
        </div>
        <ChatComposer
          routePreference={routePreference}
          lastResolvedRoute={resolvedRoute}
          generating={generating}
          onSend={handleSend}
          onCancel={handleCancel}
        />
      </div>
    </div>
  );
}

interface MessageBubbleProps {
  message: ChatMessage;
  t: (k: string, opts?: Record<string, unknown>) => string;
  generating: boolean;
  saveStatus: "idle" | "saving" | "saved" | "exists" | "error";
  onCitationClick: (index: number) => void;
  onOpenCitation: (path: string) => void;
  onSave: () => void;
}

function MessageBubble({
  message,
  t,
  generating,
  saveStatus,
  onCitationClick,
  onOpenCitation,
  onSave,
}: MessageBubbleProps) {
  const isUser = message.role === "user";
  const citations = message.citations ?? [];
  const time = formatTime(message.createdAt);
  const routeLabel = message.route ? t(`chat.composer.route.${message.route}`) : null;
  const providerLabel = message.provider ? PROVIDER_LABEL[message.provider] : null;

  return (
    <div className={`msg ${isUser ? "msg--user" : ""}`}>
      <div className={`msg__avatar ${isUser ? "msg__avatar--user" : "msg__avatar--ai"}`}>
        {isUser ? "YOU" : "AI"}
      </div>
      <div className="msg__body">
        <div className="msg__head">
          <span className="msg__name">{isUser ? t("chat.thread.you") : "AI"}</span>
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
            <MessageContent
              content={message.content}
              citationCount={citations.length}
              onCitationClick={onCitationClick}
            />
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
                    <span className="msg__citation-idx">{index + 1}</span>
                    <span className="msg__citation-title">{citation.title}</span>
                    <span className="msg__citation-path">{citation.pagePath}</span>
                  </button>
                ))}
              </div>
            ) : null}
            {!isUser ? (
              <div className="mt-2 flex items-center gap-2">
                <SaveAnswerButton status={saveStatus} disabled={generating} onSave={onSave} />
              </div>
            ) : null}
          </>
        )}
      </div>
    </div>
  );
}

interface SessionToolbarProps {
  session: { id: string; title: string; messages: unknown[]; updatedAt: string };
  routePreference: ChatRoutePreference;
  onRouteChange: (value: ChatRoutePreference) => void;
  t: (k: string, opts?: Record<string, unknown>) => string;
}

function SessionToolbar({ session, routePreference, onRouteChange, t }: SessionToolbarProps) {
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
      <div className="seg">
        {SEGMENT_OPTIONS.map((opt) => (
          <button
            key={opt.value}
            type="button"
            className={`seg__btn${routePreference === opt.value ? " is-active" : ""}`}
            onClick={() => onRouteChange(opt.value)}
          >
            {t(opt.key)}
          </button>
        ))}
      </div>
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
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={handleEdit}
          className="h-[26px] rounded-[var(--radius-sm)] px-2 text-[11px] text-[var(--text-secondary)] hover:bg-[var(--surface-muted)]"
        >
          {t("chat.sessions.rename")}
        </button>
        <button
          type="button"
          onClick={() => {
            void del(projectId, rootPath, session.id);
          }}
          className="h-[26px] rounded-[var(--radius-sm)] px-2 text-[11px] text-[var(--danger)] hover:bg-[var(--surface-muted)]"
        >
          {t("chat.sessions.delete")}
        </button>
      </div>
    </div>
  );
}

interface StreamingBubbleProps {
  text: string;
  routeLabel: string | null;
  placeholder: string;
  onOpenLogs: () => void;
  openLogsLabel: string;
}

function StreamingBubble({ text, routeLabel, placeholder, onOpenLogs, openLogsLabel }: StreamingBubbleProps) {
  return (
    <div className="msg">
      <div className="msg__avatar msg__avatar--ai">AI</div>
      <div className="msg__body">
        <div className="msg__head">
          <span className="msg__name">AI</span>
          <span className="msg__route-badge msg__route-badge--busy">
            <span className="dotstatus dotstatus--busy" aria-hidden="true" style={{ width: 6, height: 6 }} />
            {routeLabel}
          </span>
          <button
            type="button"
            onClick={onOpenLogs}
            className="text-[11px] text-[var(--accent-hover)] hover:underline"
          >
            {openLogsLabel}
          </button>
        </div>
        {text ? (
          <div className="chat-prose">
            <MessageContent content={text} citationCount={0} onCitationClick={() => {}} />
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

interface SaveAnswerButtonProps {
  status: "idle" | "saving" | "saved" | "exists" | "error";
  disabled: boolean;
  onSave: () => void;
}

function SaveAnswerButton({ status, disabled, onSave }: SaveAnswerButtonProps) {
  const { t } = useTranslation();
  if (status === "saved") {
    return <span className="text-[10.5px] text-[var(--text-muted)]">{t("chat.thread.saveDone")}</span>;
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
