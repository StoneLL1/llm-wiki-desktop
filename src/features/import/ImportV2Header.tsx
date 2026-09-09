import { Clock3, FileOutput, Package } from "lucide-react";
import { useTranslation } from "react-i18next";

export interface ImportV2HeaderProps {
  activeSection?: ImportV2Section;
  onSectionChange?: (section: ImportV2Section) => void;
}

export type ImportV2Section = "workbench" | "history" | "capabilities";

export function ImportV2Header({ activeSection = "workbench", onSectionChange }: ImportV2HeaderProps) {
  const { t } = useTranslation();
  return (
    <header className="import-v2-header">
      <h1 className="sr-only">{t("importV2.header.title")}</h1>
      <div className="import-v2-header__tools">
        <nav className="import-v2-header__nav" aria-label={t("importV2.header.sections")}>
          <button type="button" className={activeSection === "workbench" ? "is-active" : ""} aria-current={activeSection === "workbench" ? "page" : undefined} onClick={() => onSectionChange?.("workbench")}><FileOutput size={14} />{t("importV2.header.workbench")}</button>
          <button type="button" className={activeSection === "history" ? "is-active" : ""} aria-current={activeSection === "history" ? "page" : undefined} onClick={() => onSectionChange?.("history")}><Clock3 size={14} />{t("importV2.header.history")}</button>
          <button type="button" className={activeSection === "capabilities" ? "is-active" : ""} aria-current={activeSection === "capabilities" ? "page" : undefined} onClick={() => onSectionChange?.("capabilities")}><Package size={14} />{t("importV2.header.capabilities")}</button>
        </nav>
      </div>
    </header>
  );
}
