import { useState } from "react";
import { useTranslation } from "react-i18next";

import { latestAssistantMessage, useChatStore } from "../../../stores/chatStore";
import { useNavigationStore } from "../../../stores/navigationStore";
import { useTaskStore } from "../../../stores/taskStore";
import { useWikiStore } from "../../../features/wiki/wikiStore";
import { RightPanelHeader } from "../RightPanelHeader";
import type { RightPanelHostProps } from "./types";

export function ChatRightPanelHost({ currentProject }: RightPanelHostProps) {
  const { t } = useTranslation();
  const setActiveView = useNavigationStore((state) => state.setActiveView);
  const tasks = useTaskStore((state) => state.tasks);
  const openWikiPage = useWikiStore((state) => state.openPage);
  const chatSession = useChatStore((state) => state.activeSession);
  const chatLoadingSession = useChatStore((state) => state.loadingSession);
  const chatSendTaskId = useChatStore((state) => state.sendTaskId);
  const chatSendSessionId = useChatStore((state) => state.sendSessionId);
  const chatSendStarting = useChatStore((state) => state.sendStarting);
  const chatSaveStatus = useChatStore((state) => state.saveStatus);
  const chatSaveInFlightMessageId = useChatStore((state) => state.saveInFlightMessageId);
  const chatConvenienceMutationKey = useChatStore((state) => state.convenienceMutationKey);
  const chatSaveAnswer = useChatStore((state) => state.saveAnswer);
  const chatOverwriteRequest = useChatStore((state) => state.overwriteRequest);
  const [chatCopied, setChatCopied] = useState(false);
  const chatSendTask = chatSendTaskId
    ? tasks.find((task) => task.id === chatSendTaskId) ?? null
    : null;
  const chatGenerating = chatLoadingSession || chatSendStarting || Boolean(
    chatSession?.id
      && chatSendSessionId === chatSession.id
      && chatSendTask
      && ["queued", "running", "cancelling"].includes(chatSendTask.status),
  );
  const latestAssistant = chatGenerating ? null : latestAssistantMessage(chatSession);
  const citations = latestAssistant?.citations ?? [];
  const diagnostics = latestAssistant?.retrievalDiagnostics ?? null;
  const route = latestAssistant?.route ?? null;
  const provider = latestAssistant?.provider ?? null;
  const saveStatus = latestAssistant ? (chatSaveStatus[latestAssistant.id] ?? "idle") : "idle";
  const { projectId, rootPath } = currentProject;
  const providerLabel = provider ? t(`provider.name.${provider}`) : null;
  const routeLabel = route
    ? route === "agent"
      ? t("chat.composer.route.agent")
      : providerLabel
        ? t("rightpanel.route.byokLabel", { provider: providerLabel })
        : t("chat.composer.route.byok")
    : null;

  const handleSave = () => {
    if (!chatSession?.id || !latestAssistant) return;
    void chatSaveAnswer(projectId, rootPath, chatSession.id, latestAssistant.id);
  };
  const handleCopyMarkdown = () => {
    if (!latestAssistant || !navigator.clipboard) return;
    void navigator.clipboard.writeText(latestAssistant.content).then(() => {
      setChatCopied(true);
      window.setTimeout(() => setChatCopied(false), 1600);
    }).catch(() => setChatCopied(false));
  };

  return (
    <aside id="right-context-panel" aria-label={t("chat.citations.title")} className="right-panel">
      <RightPanelHeader title={t("chat.citations.title")} />
      <div className="app-pane-scrollbar min-h-0 flex-1 overflow-y-auto">
        <div className="px-4 py-3">
          <div className="border-b border-[var(--border-subtle)] py-3">
            <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">
              {t("chat.citations.title")}
              {citations.length > 0 ? <span className="ml-1 font-normal normal-case text-[var(--text-muted)]">{citations.length}</span> : null}
            </h4>
            {citations.length === 0 ? (
              <p className="m-0 text-[11px] text-[var(--text-muted)]">
                {chatGenerating ? t("chat.citations.updating") : t("chat.citations.empty")}
              </p>
            ) : (
              <div className="flex flex-col gap-1.5">
                {citations.map((citation, index) => (
                  <button
                    key={citation.sourceId ?? citation.pagePath}
                    type="button"
                    onClick={() => {
                      setActiveView("wiki");
                      void openWikiPage(projectId, rootPath, citation.pagePath);
                    }}
                    className="flex w-full items-center gap-2 rounded-[var(--radius-sm)] border border-[var(--border-subtle)] p-2 text-left hover:bg-[var(--surface-muted)]"
                  >
                    <span className="flex h-[18px] w-[18px] shrink-0 items-center justify-center rounded-full bg-[var(--accent-soft)] font-mono text-[10.5px] font-semibold text-[var(--accent-hover)]">
                      {citation.sourceId ?? `S${index + 1}`}
                    </span>
                    <div className="min-w-0 flex-1">
                      <div className="flex min-w-0 items-center gap-1.5">
                        <span className="truncate text-[12px] font-medium text-[var(--text-primary)]">{citation.title}</span>
                        {citation.isPinned ? <span className="shrink-0 rounded-[var(--radius-sm)] bg-[var(--accent-soft)] px-1.5 py-0.5 text-[10px] font-medium text-[var(--accent-hover)]">{t("chat.citations.currentPage")}</span> : null}
                      </div>
                      <div className="font-mono text-[10.5px] text-[var(--text-muted)]">{citation.pagePath}</div>
                    </div>
                  </button>
                ))}
              </div>
            )}
            {diagnostics && (diagnostics.invalidCitationIds?.length || diagnostics.hasUnverified) ? (
              <div className="mt-2 flex flex-col gap-1 rounded-[var(--radius-sm)] border border-[var(--warning)] bg-[var(--warning-soft)] px-2.5 py-2 text-[11px] text-[var(--text-secondary)]" role="status">
                {diagnostics.invalidCitationIds?.length ? <span>{t("chat.trust.invalidCitations", { ids: diagnostics.invalidCitationIds.join(", ") })}</span> : null}
                {diagnostics.hasUnverified ? <span>{t("chat.trust.unverified")}</span> : null}
              </div>
            ) : null}
          </div>

          <div className="border-b border-[var(--border-subtle)] py-3">
            <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">{t("chat.citations.route")}</h4>
            <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-2 text-[12px]">
              {routeLabel ? <><dt className="font-medium text-[var(--text-muted)]">{t("chat.citations.routePath")}</dt><dd className="m-0 text-[var(--accent-hover)]">{routeLabel}</dd></> : null}
              <dt className="font-medium text-[var(--text-muted)]">{t("rightpanel.index.pages")}</dt>
              <dd className="m-0 font-mono text-[11.5px] text-[var(--text-primary)]">{citations.length} / {currentProject.wikiPageCount}</dd>
            </dl>
          </div>

          <div className="py-3">
            <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">{t("chat.citations.actions")}</h4>
            <div className="flex flex-col gap-1.5">
              <button
                type="button"
                onClick={handleSave}
                disabled={!latestAssistant || saveStatus === "saving" || saveStatus === "saved" || Boolean(chatSaveInFlightMessageId || chatOverwriteRequest || chatConvenienceMutationKey)}
                className="flex h-[28px] items-center gap-2 rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] hover:bg-[var(--surface-muted)] disabled:opacity-40"
              >
                {saveStatus === "saved" ? t("chat.thread.saveDone") : t("chat.thread.saveAnswer")}
              </button>
              <button type="button" onClick={handleCopyMarkdown} disabled={!latestAssistant} className="flex h-[28px] items-center gap-2 rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] hover:bg-[var(--surface-muted)] disabled:opacity-40">
                {chatCopied ? t("chat.citations.copied") : t("chat.citations.copyMd")}
              </button>
              <button type="button" onClick={() => setActiveView("exports")} disabled={!latestAssistant} className="flex h-[28px] items-center gap-2 rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] hover:bg-[var(--surface-muted)] disabled:opacity-40">
                {t("chat.citations.generateCard")}
              </button>
            </div>
          </div>
        </div>
      </div>
    </aside>
  );
}
