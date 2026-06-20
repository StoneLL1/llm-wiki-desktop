import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";

import { latestAssistantMessage, useChatStore } from "../../stores/chatStore";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import { isTerminalStatus } from "../../types/task";
import type { ChatRoutePreference } from "../../types/chat";
import { ChatComposer } from "./ChatComposer";
import { ChatSessionList } from "./ChatSessionList";

const ROUTE_PREFERENCE: ChatRoutePreference = "auto";

export function ChatView() {
  const { t } = useTranslation();
  const currentProject = useProjectStore((state) => state.currentProject);

  const sessions = useChatStore((state) => state.sessions);
  const activeSessionId = useChatStore((state) => state.activeSessionId);
  const activeSession = useChatStore((state) => state.activeSession);
  const loadingSessions = useChatStore((state) => state.loadingSessions);
  const sendTaskId = useChatStore((state) => state.sendTaskId);
  const clearSendTask = useChatStore((state) => state.clearSendTask);
  const saveStatus = useChatStore((state) => state.saveStatus);
  const overwriteRequest = useChatStore((state) => state.overwriteRequest);
  const error = useChatStore((state) => state.error);

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
  // the persisted assistant message (and clear the in-flight id).
  useEffect(() => {
    if (sendTask && isTerminalStatus(sendTask.status)) {
      void reloadActive(projectId, rootPath);
      clearSendTask();
    }
  }, [sendTask, projectId, rootPath, reloadActive, clearSendTask]);

  const handleSend = (content: string) => {
    if (!activeSessionId) return;
    void send(projectId, rootPath, activeSessionId, content, ROUTE_PREFERENCE);
  };

  const handleCancel = () => {
    if (!sendTaskId) return;
    void invoke("cancel_task", { request: { taskId: sendTaskId } }).then((task) => {
      if (task) cancelTask(task as never);
    });
  };

  const latestAssistant = latestAssistantMessage(activeSession);
  const resolvedRoute = latestAssistant?.route ?? null;

  return (
    <div className="grid h-full grid-cols-[220px_minmax(0,1fr)]">
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
        <div className="min-h-0 flex-1 overflow-y-auto px-6 py-4">
          {!activeSession ? (
            <div className="flex h-full items-center justify-center text-[13px] text-[var(--text-muted)]">
              {t("chat.thread.empty")}
            </div>
          ) : (
            <div className="mx-auto flex max-w-[720px] flex-col gap-4">
              {activeSession.messages.map((message) => (
                <div
                  key={message.id}
                  className={`flex flex-col gap-1 ${message.role === "user" ? "items-end" : "items-start"}`}
                >
                  <div
                    className={`max-w-[85%] whitespace-pre-wrap rounded-[var(--radius-md)] px-3 py-2 text-[13px] leading-6 ${
                      message.role === "user"
                        ? "bg-[var(--accent-soft)] text-[var(--text-primary)]"
                        : "border border-[var(--border-subtle)] bg-[var(--surface-raised)] text-[var(--text-primary)]"
                    }`}
                  >
                    {message.content}
                  </div>
                  {message.role === "assistant" ? (
                    <div className="flex flex-wrap items-center gap-2">
                      {message.route ? (
                        <span className="text-[10.5px] uppercase tracking-[0.06em] text-[var(--text-muted)]">
                          {t(`chat.composer.route.${message.route}`)}
                        </span>
                      ) : null}
                      {message.citations && message.citations.length > 0 ? (
                        <span className="text-[10.5px] text-[var(--text-muted)]">
                          {t("chat.thread.sources", { count: message.citations.length })}
                        </span>
                      ) : null}
                      <SaveAnswerButton
                        messageId={message.id}
                        status={saveStatus[message.id] ?? "idle"}
                        disabled={generating}
                        onSave={() => {
                          if (!activeSessionId) return;
                          void saveAnswer(projectId, rootPath, activeSessionId, message.id);
                        }}
                      />
                    </div>
                  ) : null}
                </div>
              ))}
              {generating ? (
                <div className="flex items-center gap-2 text-[12px] text-[var(--text-muted)]">
                  <span className="inline-block h-2 w-2 animate-pulse rounded-full bg-[var(--accent)]" />
                  {t("chat.thread.generating")}
                  <button
                    type="button"
                    onClick={() => openTaskDrawer(sendTaskId ?? undefined)}
                    className="text-[var(--accent-hover)] hover:underline"
                  >
                    {t("chat.thread.openLogs")}
                  </button>
                </div>
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
                      className="h-[28px] rounded-[var(--radius-md)] bg-[var(--foreground)] px-3 text-[12px] font-medium text-[var(--text-inverse)] hover:bg-[#1a1a1a]"
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
          )}
        </div>
        <ChatComposer
          routePreference={ROUTE_PREFERENCE}
          lastResolvedRoute={resolvedRoute}
          generating={generating}
          onSend={handleSend}
          onCancel={handleCancel}
        />
      </div>
    </div>
  );
}

interface SaveAnswerButtonProps {
  messageId: string;
  status: "idle" | "saving" | "saved" | "exists" | "error";
  disabled: boolean;
  onSave: () => void;
}

function SaveAnswerButton({ messageId, status, disabled, onSave }: SaveAnswerButtonProps) {
  const { t } = useTranslation();
  void messageId;
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
