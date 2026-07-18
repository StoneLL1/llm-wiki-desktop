import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Pencil, Plus, Search, Trash2 } from "lucide-react";

import type { ChatSessionSummary } from "../../types/chat";

interface ChatSessionListProps {
  sessions: ChatSessionSummary[];
  activeSessionId: string | null;
  loading: boolean;
  onSelect: (sessionId: string) => void;
  onCreate: () => void;
  onRename: (sessionId: string, title: string) => void;
  onDelete: (sessionId: string) => void;
  deleteDisabled?: boolean;
  deleteDisabledSessionId?: string | null;
  createDisabled?: boolean;
}

/** Format an ISO timestamp as HH:MM (locale-independent, 24h). Mirrors the
 *  ChatView helper; duplicated here to avoid a circular import. */
function formatTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

export function ChatSessionList({
  sessions,
  activeSessionId,
  loading,
  onSelect,
  onCreate,
  onRename,
  onDelete,
  deleteDisabled = false,
  deleteDisabledSessionId = null,
  createDisabled = false,
}: ChatSessionListProps) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
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

  const trimmedQuery = query.trim().toLowerCase();
  const filtered = trimmedQuery
    ? sessions.filter((session) => session.title.toLowerCase().includes(trimmedQuery))
    : sessions;

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-[44px] items-center gap-1.5 border-b border-[var(--border-subtle)] px-2">
        <div className="flex h-[28px] min-w-0 flex-1 items-center gap-1.5 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--background)] px-2">
          <Search aria-hidden="true" size={13} className="shrink-0 text-[var(--text-muted)]" />
          <input
            type="text"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("chat.sessions.search")}
            aria-label={t("chat.sessions.search")}
            className="min-w-0 flex-1 bg-transparent text-[12px] text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none"
          />
        </div>
        <button
          type="button"
          onClick={onCreate}
          disabled={createDisabled}
          className="icon-button shrink-0"
          aria-label={t("chat.sessions.new")}
          title={t("chat.sessions.new")}
        >
          <Plus aria-hidden="true" size={15} />
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto py-1">
        {loading ? (
          <p className="m-0 px-3 py-2 text-[11px] text-[var(--text-muted)]">{t("chat.sessions.loading")}</p>
        ) : sessions.length === 0 ? (
          <p className="m-0 px-3 py-2 text-[11px] text-[var(--text-muted)]">{t("chat.sessions.empty")}</p>
        ) : filtered.length === 0 ? (
          <p className="m-0 px-3 py-2 text-[11px] text-[var(--text-muted)]">{t("chat.sessions.noSearchResults")}</p>
        ) : (
          filtered.map((session) => {
            const isActive = session.id === activeSessionId;
            const isEditing = editingId === session.id;
            const meta = `${formatTime(session.updatedAt)} · ${t("chat.sessions.messageCount", {
              count: session.messageCount,
            })}`;
            return (
              <div
                key={session.id}
                className={`group flex items-center gap-1 px-2 py-1.5 text-[13px] ${
                  isActive
                    ? "bg-[var(--accent-soft)] text-[var(--accent-hover)]"
                    : "text-[var(--text-secondary)] hover:bg-[var(--surface-muted)]"
                }`}
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
                      aria-current={isActive ? "page" : undefined}
                      className="min-w-0 flex-1 truncate text-left"
                      title={session.title}
                    >
                      <div className="truncate text-[12.5px] font-medium leading-tight">{session.title}</div>
                      <div className="chat-session__meta">{meta}</div>
                    </button>
                    <button
                      type="button"
                      onClick={() => startEdit(session)}
                      className="flex h-[24px] w-[24px] shrink-0 items-center justify-center rounded-[var(--radius-sm)] text-[var(--text-muted)] opacity-0 pointer-events-none transition-opacity hover:bg-[var(--surface-muted)] hover:text-[var(--text-primary)] group-hover:pointer-events-auto group-hover:opacity-100 group-focus-within:pointer-events-auto group-focus-within:opacity-100"
                      aria-label={t("chat.sessions.rename")}
                      title={t("chat.sessions.rename")}
                    >
                      <Pencil aria-hidden="true" size={13} />
                    </button>
                    <button
                      type="button"
                      onClick={() => onDelete(session.id)}
                      disabled={deleteDisabled || deleteDisabledSessionId === session.id}
                      className="flex h-[24px] w-[24px] shrink-0 items-center justify-center rounded-[var(--radius-sm)] text-[var(--text-muted)] opacity-0 pointer-events-none transition-opacity hover:bg-[var(--surface-muted)] hover:text-[var(--danger)] group-hover:pointer-events-auto group-hover:opacity-100 group-focus-within:pointer-events-auto group-focus-within:opacity-100 disabled:cursor-not-allowed disabled:opacity-30"
                      aria-label={t("chat.sessions.delete")}
                      title={t("chat.sessions.delete")}
                    >
                      <Trash2 aria-hidden="true" size={13} />
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
