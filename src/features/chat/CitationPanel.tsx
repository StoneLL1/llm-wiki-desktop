import { useTranslation } from "react-i18next";

import type { ChatCitation } from "../../types/chat";

interface CitationPanelProps {
  citations: ChatCitation[];
  onOpenPage: (path: string) => void;
}

export function CitationPanel({ citations, onOpenPage }: CitationPanelProps) {
  const { t } = useTranslation();
  return (
    <div className="px-4 py-3">
      <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">
        {t("chat.citations.title")}
      </h4>
      {citations.length === 0 ? (
        <p className="m-0 text-[11px] text-[var(--text-muted)]">{t("chat.citations.empty")}</p>
      ) : (
        <ul className="m-0 flex flex-col gap-2 p-0" style={{ listStyle: "none" }}>
          {citations.map((citation) => (
            <li key={citation.pagePath} className="flex flex-col gap-1 rounded-[var(--radius-sm)] border border-[var(--border-subtle)] p-2">
              <button
                type="button"
                onClick={() => onOpenPage(citation.pagePath)}
                className="text-left text-[12px] font-medium text-[var(--accent-hover)] hover:underline"
                title={t("chat.citations.openPage")}
              >
                {citation.title}
              </button>
              <span className="font-mono text-[10.5px] text-[var(--text-muted)]">{citation.pagePath}</span>
              {citation.snippet ? (
                <p className="m-0 line-clamp-3 text-[11.5px] leading-5 text-[var(--text-secondary)]">{citation.snippet}</p>
              ) : null}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
