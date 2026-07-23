import { Clock3, FileOutput, Package } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { ImportSession } from "../../types/importV2";
import type { BackendTask } from "../../types/task";
import type { ImportSessionProgress } from "./importViewModel";

export interface ImportV2HeaderProps {
  session: ImportSession | null;
  progress?: ImportSessionProgress;
  discoveryTask?: BackendTask | null;
  syncing?: boolean;
  activeSection?: ImportV2Section;
  onSectionChange?: (section: ImportV2Section) => void;
}

export type ImportV2Section = "workbench" | "history" | "capabilities";

export function ImportV2Header({ session, progress, discoveryTask, syncing = false, activeSection = "workbench", onSectionChange }: ImportV2HeaderProps) {
  const { t } = useTranslation();
  const total = session?.items.length ?? 0;
  const completed = session?.items.filter((item) => item.status === "completed").length ?? 0;
  const processed = progress?.processed ?? completed;
  const active = progress?.active ?? 0;
  const failed = progress?.failed ?? 0;
  const needsAction = progress?.needsAction ?? 0;
  const discoveryActive = discoveryTask?.status === "queued" || discoveryTask?.status === "running" || discoveryTask?.status === "cancelling";
  const discoveryCount = discoveryTask?.progress?.current ?? 0;
  return (
    <header className="import-v2-header">
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <FileOutput size={18} className="shrink-0 text-[var(--accent)]" aria-hidden="true" />
          <h1 className="m-0 text-[20px] font-semibold tracking-[-0.02em]">{t("importV2.header.title")}</h1>
        </div>
      </div>
      <div className="import-v2-header__tools">
        <nav className="import-v2-header__nav" aria-label={t("importV2.header.sections")}>
          <button type="button" className={activeSection === "workbench" ? "is-active" : ""} aria-current={activeSection === "workbench" ? "page" : undefined} onClick={() => onSectionChange?.("workbench")}><FileOutput size={14} />{t("importV2.header.workbench")}</button>
          <button type="button" className={activeSection === "history" ? "is-active" : ""} aria-current={activeSection === "history" ? "page" : undefined} onClick={() => onSectionChange?.("history")}><Clock3 size={14} />{t("importV2.header.history")}</button>
          <button type="button" className={activeSection === "capabilities" ? "is-active" : ""} aria-current={activeSection === "capabilities" ? "page" : undefined} onClick={() => onSectionChange?.("capabilities")}><Package size={14} />{t("importV2.header.capabilities")}</button>
        </nav>
        <span className="import-v2-header__stat" aria-live="polite">{discoveryActive ? t("importV2.header.discovery", { count: discoveryCount }) : syncing ? t("importV2.header.syncing") : t("importV2.header.stats", { completed, total, processed, active, failed, needsAction })}</span>
      </div>
    </header>
  );
}
