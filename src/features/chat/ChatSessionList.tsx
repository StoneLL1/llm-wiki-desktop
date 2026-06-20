import { useState } from "react";
import { useTranslation } from "react-i18next";

import type { ChatSessionSummary } from "../../types/chat";

interface ChatSessionListProps {
  sessions: ChatSessionSummary[];
  activeSessionId: string | null;
  loading: boolean;
  onSelect: (sessionId: string) => void;
  onCreate: () => void;
  onRename: (sessionId: string, title: string) => void;
  onDelete: (sessionId: string) => void;
}

export function ChatSessionList({
  sessions,
  activeSessionId,
  loading,
  onSelect,
  onCreate,
  onRename,
  onDelete,
}: ChatSessionListProps) {
  const { t } = useTranslation();
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");

  const startEdit = (session: ChatSessionSummary) => {
    setEditingId(session.id);
    setDraft(session.title);
  };

  const commitEdit = () => {
    if (editingId && draft.trim()) {
      onRename(editingId, draft.trim());
    }
    setEditingId(null);
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-[44px] items-center justify-between border-b border-[var(--border-subtle)] px-3">
        <span className="text-[10.5px] font-semibold uppercase tracking-[0.08em] text-[var(--text-muted)]">
          {t("chat.sessions.title")}
        </span>
        <button
          type="button"
          onClick={onCreate}
          className="h-[24px] rounded-[var(--radius-sm)] px-2 text-[12px] font-medium text-[var(--accent-hover)] hover:bg-[var(--surface-muted)]"
        >
          {t("chat.sessions.new")}
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto py-1">
        {loading ? (
          <p className="m-0 px-3 py-2 text-[11px] text-[var(--text-muted)]">{t("chat.sessions.loading")}</p>
        ) : sessions.length === 0 ? (
          <p className="m-0 px-3 py-2 text-[11px] text-[var(--text-muted)]">{t("chat.sessions.empty")}</p>
        ) : (
          sessions.map((session) => {
            const isActive = session.id === activeSessionId;
            const isEditing = editingId === session.id;
            return (
              <div
                key={session.id}
                className={`group flex h-[30px] items-center gap-1 px-2 text-[13px] ${
                  isActive
                    ? "bg-[var(--accent-soft)] text-[var(--accent-hover)]"
                    : "text-[var(--text-secondary)] hover:bg-[var(--surface-muted)]"
                }`}
                aria-current={isActive ? "true" : undefined}
              >
                {isEditing ? (
                  <input
                    autoFocus
                    value={draft}
                    onChange={(event) => setDraft(event.target.value)}
                    onBlur={commitEdit}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") commitEdit();
                      if (event.key === "Escape") setEditingId(null);
                    }}
                    className="h-[22px] min-w-0 flex-1 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--background)] px-1 text-[12px]"
                  />
                ) : (
                  <>
                    <button
                      type="button"
                      onClick={() => onSelect(session.id)}
                      onDoubleClick={() => startEdit(session)}
                      className="min-w-0 flex-1 truncate text-left"
                      title={session.title}
                    >
                      {session.title}
                    </button>
                    <button
                      type="button"
                      onClick={() => startEdit(session)}
                      className="hidden h-[18px] w-[18px] items-center justify-center text-[var(--text-muted)] hover:text-[var(--text-primary)] group-hover:flex"
                      aria-label={t("chat.sessions.rename")}
                    >
                      ✎
                    </button>
                    <button
                      type="button"
                      onClick={() => onDelete(session.id)}
                      className="hidden h-[18px] w-[18px] items-center justify-center text-[var(--text-muted)] hover:text-[var(--danger)] group-hover:flex"
                      aria-label={t("chat.sessions.delete")}
                    >
                      ×
                    </button>
                  </>
                )}
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
