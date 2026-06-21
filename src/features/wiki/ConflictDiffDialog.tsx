import { useState } from "react";
import { useTranslation } from "react-i18next";
import { AlertTriangle } from "lucide-react";

import type { WikiSaveConflict } from "./wikiStore";

interface ConflictDiffDialogProps {
  conflict: WikiSaveConflict;
  onKeepCurrent: () => void;
  onUseIncoming: () => void;
  onManualMerge: (content: string) => void;
  onCancel: () => void;
}

export function ConflictDiffDialog({
  conflict,
  onKeepCurrent,
  onUseIncoming,
  onManualMerge,
  onCancel,
}: ConflictDiffDialogProps) {
  const { t } = useTranslation();
  const [manual, setManual] = useState(false);
  const [merged, setMerged] = useState(conflict.incomingContent);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 px-4"
      role="dialog"
      aria-modal="true"
      aria-labelledby="wiki-conflict-title"
      onKeyDown={(event) => {
        if (event.key === "Escape") onCancel();
      }}
    >
      <section className="flex max-h-[88vh] w-full max-w-[1080px] flex-col rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-lg">
        <header className="flex min-h-[52px] items-center gap-3 border-b border-[var(--border)] px-4">
          <AlertTriangle size={18} className="text-[var(--warning)]" />
          <div className="min-w-0">
            <h2 id="wiki-conflict-title" className="truncate text-[16px] font-semibold text-[var(--text-primary)]">
              {t("wiki.conflict.title", { path: conflict.path })}
            </h2>
            <p className="mt-1 text-[12px] text-[var(--text-muted)]">
              {t("wiki.conflict.description")}
            </p>
          </div>
        </header>

        <div className="min-h-0 flex-1 overflow-auto p-4">
          <div className="grid min-w-[780px] grid-cols-3 gap-3">
            <DiffColumn label={t("wiki.conflict.baseline")} content={conflict.originalContent} tone="baseline" />
            <DiffColumn label={t("wiki.conflict.current")} content={conflict.currentContent} tone="current" />
            <DiffColumn label={t("wiki.conflict.incoming")} content={conflict.incomingContent} tone="incoming" />
          </div>
          {manual ? (
            <label className="mt-4 block text-[12px] text-[var(--text-secondary)]">
              <span className="mb-1.5 block font-medium">{t("wiki.conflict.manualLabel")}</span>
              <textarea
                aria-label={t("wiki.conflict.manualLabel")}
                value={merged}
                onChange={(event) => setMerged(event.target.value)}
                className="min-h-[220px] w-full rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--background)] p-3 font-mono text-[11.5px] leading-5 text-[var(--text-primary)] outline-none focus:border-[var(--accent)]"
              />
            </label>
          ) : null}
        </div>

        <footer className="flex min-h-[52px] items-center justify-end gap-2 border-t border-[var(--border)] px-4">
          <button type="button" onClick={onCancel} className="h-[28px] rounded-[var(--radius-md)] border border-[var(--border)] px-3 text-[12px]">
            {t("confirmation.cancel")}
          </button>
          <button type="button" onClick={onKeepCurrent} className="h-[28px] rounded-[var(--radius-md)] border border-[var(--border)] px-3 text-[12px]">
            {t("wiki.conflict.keepCurrent")}
          </button>
          <button type="button" onClick={onUseIncoming} className="h-[28px] rounded-[var(--radius-md)] border border-[var(--border)] px-3 text-[12px]">
            {t("wiki.conflict.useIncoming")}
          </button>
          {manual ? (
            <button type="button" onClick={() => onManualMerge(merged)} className="h-[28px] rounded-[var(--radius-md)] bg-[var(--foreground)] px-3 text-[12px] font-medium text-[var(--text-inverse)]">
              {t("wiki.conflict.applyManual")}
            </button>
          ) : (
            <button type="button" onClick={() => setManual(true)} className="h-[28px] rounded-[var(--radius-md)] bg-[var(--foreground)] px-3 text-[12px] font-medium text-[var(--text-inverse)]">
              {t("wiki.conflict.manualMerge")}
            </button>
          )}
        </footer>
      </section>
    </div>
  );
}

function DiffColumn({
  label,
  content,
  tone,
}: {
  label: string;
  content: string;
  tone: "baseline" | "current" | "incoming";
}) {
  return (
    <section className="overflow-hidden rounded-[var(--radius-md)] border border-[var(--border)]">
      <h3 className="border-b border-[var(--border)] bg-[var(--surface)] px-3 py-2 font-mono text-[10.5px] uppercase tracking-[0.08em] text-[var(--text-muted)]">
        {label}
      </h3>
      <pre className={`wiki-diff wiki-diff--${tone}`}>
        {content.split("\n").map((line, index) => (
          <span className="wiki-diff__line" key={`${index}-${line}`}>
            <span className="wiki-diff__number">{index + 1}</span>
            <span>{line || " "}</span>
          </span>
        ))}
      </pre>
    </section>
  );
}
