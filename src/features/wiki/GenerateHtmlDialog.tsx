import { useState } from "react";
import { useTranslation } from "react-i18next";
import { FileOutput, LayoutGrid, Network, Newspaper } from "lucide-react";

import type { ExportType } from "../../types/export";

interface GenerateHtmlDialogProps {
  pagePath: string;
  onCancel: () => void;
  onGenerate: (type: ExportType) => void;
}

const templates = [
  { type: "beautiful_read", icon: Newspaper, skill: "html-beautiful-read" },
  { type: "knowledge_card", icon: LayoutGrid, skill: "html-knowledge-card" },
  { type: "concept_map", icon: Network, skill: "html-concept-map" },
  { type: "project_report", icon: FileOutput, skill: "html-project-report" },
] satisfies Array<{ type: ExportType; icon: typeof Newspaper; skill: string }>;

export function GenerateHtmlDialog({
  pagePath,
  onCancel,
  onGenerate,
}: GenerateHtmlDialogProps) {
  const { t } = useTranslation();
  const [selected, setSelected] = useState<ExportType>("beautiful_read");

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 px-4" role="dialog" aria-modal="true" aria-labelledby="generate-html-title">
      <section className="w-full max-w-[820px] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-lg">
        <header className="flex min-h-[52px] items-center border-b border-[var(--border)] px-4">
          <h2 id="generate-html-title" className="text-[16px] font-semibold text-[var(--text-primary)]">
            {t("wiki.html.generateTitle", { path: pagePath })}
          </h2>
        </header>
        <div className="p-4">
          <p className="mb-3 text-[12px] text-[var(--text-muted)]">
            {t("wiki.html.templateHint")}
          </p>
          <div className="tmpl-grid">
            {templates.map(({ type, icon: Icon, skill }) => {
              const active = selected === type;
              const label = t(`wiki.html.template.${type}.title`);
              return (
                <button
                  key={type}
                  type="button"
                  aria-label={label}
                  aria-pressed={active}
                  onClick={() => setSelected(type)}
                  className={`tmpl-card ${active ? "is-selected" : ""}`}
                >
                  <span className="tmpl-card__thumb"><Icon size={30} strokeWidth={1.2} /></span>
                  <span className="tmpl-card__title">{label}</span>
                  <span className="tmpl-card__desc">{t(`wiki.html.template.${type}.desc`)}</span>
                  <span className="tmpl-card__skill">{skill} →</span>
                </button>
              );
            })}
          </div>
          <div className="mt-4 rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--surface)] p-3 font-mono text-[11px] text-[var(--text-secondary)]">
            {t("wiki.html.outputHint", { path: pagePath })}
          </div>
        </div>
        <footer className="flex min-h-[52px] items-center justify-end gap-2 border-t border-[var(--border)] px-4">
          <button type="button" onClick={onCancel} className="h-[28px] rounded-[var(--radius-md)] border border-[var(--border)] px-3 text-[12px]">
            {t("confirmation.cancel")}
          </button>
          <button type="button" onClick={() => onGenerate(selected)} className="h-[28px] rounded-[var(--radius-md)] bg-[var(--foreground)] px-3 text-[12px] font-medium text-[var(--text-inverse)]">
            {t("wiki.html.generateAndPreview")}
          </button>
        </footer>
      </section>
    </div>
  );
}
