import { useState } from "react";
import { useTranslation } from "react-i18next";

import type { ChatRoute, ChatRoutePreference } from "../../types/chat";

interface ChatComposerProps {
  routePreference: ChatRoutePreference;
  lastResolvedRoute: ChatRoute | null;
  generating: boolean;
  onSend: (content: string) => void;
  onCancel: () => void;
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

export function ChatComposer({
  routePreference,
  lastResolvedRoute,
  generating,
  onSend,
  onCancel,
}: ChatComposerProps) {
  const { t } = useTranslation();
  const [value, setValue] = useState("");

  const submit = () => {
    const trimmed = value.trim();
    if (!trimmed || generating) return;
    onSend(trimmed);
    setValue("");
  };

  const badgeKey = lastResolvedRoute ? ROUTE_LABEL[lastResolvedRoute] : PREFERENCE_LABEL[routePreference];

  return (
    <div className="border-t border-[var(--border-subtle)] p-3">
      <div className="mb-2 flex items-center gap-2">
        <span className="inline-flex h-[18px] items-center rounded-[var(--radius-pill)] bg-[var(--accent-soft)] px-2 text-[10.5px] font-medium text-[var(--accent-hover)]">
          {t(badgeKey)}
        </span>
        {generating ? (
          <button
            type="button"
            onClick={onCancel}
            className="h-[26px] rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] font-medium text-[var(--danger)] hover:bg-[var(--surface-muted)]"
          >
            {t("chat.composer.cancel")}
          </button>
        ) : null}
      </div>
      <div className="flex items-end gap-2">
        <textarea
          value={value}
          onChange={(event) => setValue(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              submit();
            }
          }}
          placeholder={t("chat.composer.placeholder")}
          rows={2}
          className="min-h-[44px] flex-1 resize-none rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--background)] px-3 py-2 text-[13px] leading-5 text-[var(--text-primary)] focus:outline-none focus:ring-1 focus:ring-[var(--accent)]"
        />
        <button
          type="button"
          onClick={submit}
          disabled={generating || !value.trim()}
          className="h-[44px] rounded-[var(--radius-md)] bg-[var(--foreground)] px-4 text-[13px] font-medium text-[var(--text-inverse)] hover:bg-[#1a1a1a] disabled:opacity-40"
        >
          {t("chat.composer.send")}
        </button>
      </div>
    </div>
  );
}
