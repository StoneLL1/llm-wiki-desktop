import { ArrowRight, ShieldAlert } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { ImportFrontendReadiness } from "../../types/importV2Presentation";

export interface ImportMigrationNoticeProps {
  readiness: ImportFrontendReadiness | null;
  onOpenMigration: () => void;
}

export function ImportMigrationNotice({ readiness, onOpenMigration }: ImportMigrationNoticeProps) {
  const { t } = useTranslation();
  if (!readiness || readiness.active) return null;
  return (
    <section className="mx-4 mb-3 flex items-start gap-3 rounded-[var(--radius-md)] border border-[var(--warning)] bg-[var(--warning-soft)] px-3 py-3" role="alert" aria-labelledby="import-migration-notice-title">
      <ShieldAlert size={17} className="mt-0.5 shrink-0 text-[var(--warning-text)]" aria-hidden="true" />
      <div className="min-w-0 flex-1">
        <h2 id="import-migration-notice-title" className="m-0 text-[13px] font-semibold">{t("importV2.migration.noticeTitle")}</h2>
        <p className="m-0 mt-1 text-[11px] text-[var(--text-secondary)]">{t("importV2.migration.noticeBody")}</p>
      </div>
      <button type="button" className="btn btn--sm shrink-0" onClick={onOpenMigration}><ArrowRight size={13} className="mr-1 inline" aria-hidden="true" />{t("importV2.migration.review")}</button>
    </section>
  );
}
