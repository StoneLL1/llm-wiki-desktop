import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { CircleStop, SendHorizontal } from "lucide-react";

import type { ChatRoute, ChatRoutePreference } from "../../types/chat";

interface ChatComposerProps {
  routePreference: ChatRoutePreference;
  lastResolvedRoute: ChatRoute | null;
  generating: boolean;
  /** Returns whether the parent accepted the send. The composer only clears
   *  the draft when the send actually landed (a session existed and the backend
   *  returned a task id), so a failed/no-op send preserves the user's text. */
  onSend: (content: string) => boolean | Promise<boolean>;
  onCancel: () => void;
  placeholderKey?: string;
  compact?: boolean;
  /** Clears a draft when the composer changes its session/page owner. */
  draftKey?: string | null;
  /** Blocks a second Chat run while another session owns the global task slot. */
  blocked?: boolean;
}

const ROUTE_LABEL: Record<ChatRoute, string> = {
  agent: "chat.composer.route.agent",
  byok: "chat.composer.route.byok",
};

const PREFERENCE_LABEL: Record<ChatRoutePreference, string> = {
  auto: "chat.composer.route.auto",
  agent: "chat.composer.route.agent",
  byok: "chat.composer.route.byok",
};

const MAX_CHAT_CONTENT_CHARS = 32_000;

export function ChatComposer({
  routePreference,
  lastResolvedRoute,
  generating,
  onSend,
  onCancel,
  placeholderKey = "chat.composer.placeholder",
  compact = false,
  draftKey = null,
  blocked = false,
}: ChatComposerProps) {
  const { t } = useTranslation();
  const [value, setValue] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const drafts = useRef<Record<string, string>>({});
  const draftRevisions = useRef<Record<string, number>>({});
  const migratedFrom = useRef<Record<string, string | undefined>>({});
  const scopeKey = draftKey ? `scope:${draftKey}` : "scope:global";
  const previousScopeKey = useRef(scopeKey);

  useEffect(() => {
    const previousDraft = drafts.current[previousScopeKey.current];
    if (
      previousScopeKey.current.endsWith(":chat:new") &&
      previousDraft &&
      drafts.current[scopeKey] === undefined
    ) {
      drafts.current[scopeKey] = previousDraft;
      draftRevisions.current[scopeKey] = draftRevisions.current[previousScopeKey.current] ?? 0;
      migratedFrom.current[scopeKey] = previousScopeKey.current;
    }
    draftRevisions.current[scopeKey] ??= 0;
    setValue(drafts.current[scopeKey] ?? "");
    previousScopeKey.current = scopeKey;
  }, [scopeKey]);

  const submit = async () => {
    const trimmed = value.trim();
    if (!trimmed || generating || submitting || blocked) return;
    const submissionKey = scopeKey;
    const submissionRevision = draftRevisions.current[submissionKey] ?? 0;
    setSubmitting(true);
    try {
      const accepted = await onSend(trimmed);
      if (accepted) {
        const currentKey = previousScopeKey.current;
        const migratedDraft = drafts.current[currentKey];
        const ownsMigratedDraft =
          currentKey === submissionKey || migratedFrom.current[currentKey] === submissionKey;
        const draftIsUnchanged =
          (draftRevisions.current[currentKey] ?? 0) === submissionRevision &&
          migratedDraft?.trim() === trimmed;
        const submissionDraftIsUnchanged =
          (draftRevisions.current[submissionKey] ?? 0) === submissionRevision &&
          drafts.current[submissionKey]?.trim() === trimmed;
        if (submissionDraftIsUnchanged) {
          delete drafts.current[submissionKey];
          delete draftRevisions.current[submissionKey];
        }
        if (ownsMigratedDraft && draftIsUnchanged) {
          delete drafts.current[currentKey];
          delete draftRevisions.current[currentKey];
          delete migratedFrom.current[currentKey];
          setValue("");
        }
      }
    } finally {
      setSubmitting(false);
    }
  };

  const badgeKey = lastResolvedRoute ? ROUTE_LABEL[lastResolvedRoute] : PREFERENCE_LABEL[routePreference];

  return (
    <div className={`border-t border-[var(--border-subtle)] ${compact ? "p-2" : "p-3"}`}>
      <div className="mb-2 flex items-center gap-2">
        <span className="inline-flex h-[18px] items-center rounded-[var(--radius-pill)] bg-[var(--accent-soft)] px-2 text-[10.5px] font-medium text-[var(--accent-hover)]">
          {t(badgeKey)}
        </span>
        {generating ? (
          <button
            type="button"
            onClick={onCancel}
            className="inline-flex h-[26px] items-center gap-1.5 rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] font-medium text-[var(--danger)] hover:bg-[var(--surface-muted)]"
          >
            <CircleStop aria-hidden="true" size={13} />
            {t("chat.composer.cancel")}
          </button>
        ) : null}
        {blocked && !generating ? (
          <span className="text-[11px] text-[var(--text-muted)]">
            {t("chat.composer.busy")}
          </span>
        ) : null}
      </div>
      <div className="flex items-end gap-2">
        <textarea
          value={value}
          onChange={(event) => {
            const next = event.target.value;
            drafts.current[scopeKey] = next;
            draftRevisions.current[scopeKey] = (draftRevisions.current[scopeKey] ?? 0) + 1;
            setValue(next);
          }}
          onKeyDown={(event) => {
            // Guard IME composition (CJK input): Enter confirms the candidate,
            // it must not also send the message mid-composition.
            if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
              event.preventDefault();
              void submit();
            }
          }}
          placeholder={t(placeholderKey)}
          maxLength={MAX_CHAT_CONTENT_CHARS}
          aria-label={t("chat.composer.inputLabel")}
          rows={compact ? 1 : 2}
          className="min-h-[44px] flex-1 resize-none rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--background)] px-3 py-2 text-[13px] leading-5 text-[var(--text-primary)] focus:outline-none focus:ring-1 focus:ring-[var(--accent)]"
        />
        <button
          type="button"
          onClick={() => void submit()}
          disabled={generating || blocked || submitting || !value.trim()}
          className="inline-flex h-[44px] items-center gap-1.5 rounded-[var(--radius-md)] bg-[var(--foreground)] px-4 text-[13px] font-medium text-[var(--text-inverse)] hover:bg-[var(--primary-hover)] disabled:opacity-40"
        >
          <SendHorizontal aria-hidden="true" size={14} />
          {t("chat.composer.send")}
        </button>
      </div>
    </div>
  );
}
