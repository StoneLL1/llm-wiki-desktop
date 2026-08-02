import { AlertTriangle, FolderOpen, ShieldCheck } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { ProjectOpenAssessment } from "../../types/project";

export function ProjectAssessmentPanel({
  assessment,
  onBack,
  onOpen,
}: {
  assessment: ProjectOpenAssessment;
  onBack: () => void;
  onOpen?: () => void;
}) {
  const { t } = useTranslation();
  const facts = [
    ["format", t(`projectAssessment.format.${assessment.format}`)],
    ["trust", t(`projectAssessment.trust.${assessment.trust}`)],
    ["filesystem", t(`projectAssessment.filesystem.${assessment.filesystemAccess}`)],
    ["health", t(`projectAssessment.health.${assessment.health}`)],
    ["layout", t("projectAssessment.markdownRoots", { count: assessment.layout.markdownRoots.length })],
    ["capabilities", t("projectAssessment.capabilities", { count: assessment.capabilities.length })],
    ["git", t(assessment.git.isRepository ? "projectAssessment.git.repository" : "projectAssessment.git.none")],
  ] as const;

  return (
    <section
      aria-labelledby="project-assessment-title"
      className="mx-auto mt-5 w-full max-w-[760px] border border-[var(--border)] bg-[var(--surface)]"
    >
      <header className="flex min-h-11 items-start gap-3 border-b border-[var(--border)] px-[var(--sp-4)] py-[var(--sp-3)]">
        <span className="mt-0.5 text-[var(--accent)]">
          {assessment.health === "healthy" ? (
            <ShieldCheck aria-hidden="true" size={16} />
          ) : (
            <AlertTriangle aria-hidden="true" size={16} />
          )}
        </span>
        <div className="min-w-0">
          <h2 id="project-assessment-title" className="m-0 text-[13px] font-semibold">
            {t("projectAssessment.title")}
          </h2>
          <p className="m-0 mt-1 truncate font-mono text-[11px] text-[var(--text-muted)]" title={assessment.canonicalRootPath}>
            {assessment.canonicalRootPath}
          </p>
        </div>
      </header>

      <dl className="grid grid-cols-2 gap-px bg-[var(--border)] max-[680px]:grid-cols-1">
        {facts.map(([key, value]) => (
          <div className="flex min-h-10 items-center justify-between gap-4 bg-[var(--surface)] px-[var(--sp-4)] py-[var(--sp-2)]" key={key}>
            <dt className="text-[11px] text-[var(--text-muted)]">{t(`projectAssessment.dimension.${key}`)}</dt>
            <dd className="m-0 text-right text-[12px] text-[var(--text-secondary)]">{value}</dd>
          </div>
        ))}
      </dl>

      {assessment.warnings.length > 0 || assessment.layoutWarnings.length > 0 ? (
        <div className="border-t border-[var(--border)] px-[var(--sp-4)] py-[var(--sp-3)]" role="status">
          {[...assessment.warnings, ...assessment.layoutWarnings].map((warning) => (
            <p className="m-0 text-[12px] leading-5 text-[var(--text-secondary)]" key={`${warning.code}:${warning.path ?? ""}`}>
              {t(`projectAssessment.warning.${warning.code}`, { defaultValue: warning.message })}
            </p>
          ))}
        </div>
      ) : null}

      <footer className="flex items-center justify-end gap-2 border-t border-[var(--border)] px-[var(--sp-4)] py-[var(--sp-3)]">
        <button className="btn btn--secondary" onClick={onBack} type="button">
          {t("projectAssessment.back")}
        </button>
        {onOpen ? (
          <button className="btn btn--primary" onClick={onOpen} type="button">
            <FolderOpen aria-hidden="true" size={14} />
            {t("projectAssessment.open")}
          </button>
        ) : null}
      </footer>
    </section>
  );
}
