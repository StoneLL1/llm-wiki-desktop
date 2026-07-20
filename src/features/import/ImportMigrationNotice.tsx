import { CircleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { ImportFrontendReadiness } from "../../types/importV2Presentation";

export interface ImportMigrationNoticeProps {
  readiness: ImportFrontendReadiness | null;
  unavailable?: boolean;
  onOpenMigration: () => void;
}

export function ImportMigrationNotice({ readiness, unavailable = false, onOpenMigration }: ImportMigrationNoticeProps) {
  const { t } = useTranslation();
  if ((!readiness && !unavailable) || readiness?.active) return null;
  const title = t(unavailable ? "importV2.migration.unavailableTitle" : "importV2.migration.noticeTitle");
  const body = t(unavailable ? "importV2.migration.unavailableBody" : "importV2.migration.noticeBody");
  const review = t("importV2.migration.review");
  return (
    <div className="import-migration-notice" aria-label={title}>
      <button
        type="button"
        className="icon-button import-migration-notice__trigger"
        aria-label={review}
        aria-describedby="import-migration-notice-tooltip"
        title={`${title}: ${body}`}
        onClick={onOpenMigration}
      >
        <CircleAlert size={16} aria-hidden="true" />
      </button>
      <div id="import-migration-notice-tooltip" className="import-migration-notice__tooltip" role="tooltip">
        <strong className="import-migration-notice__title">{title}</strong>
        <span className="import-migration-notice__body">{body}</span>
        <span className="import-migration-notice__action">{review}</span>
      </div>
    </div>
  );
}
