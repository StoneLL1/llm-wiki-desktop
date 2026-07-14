import { FileOutput } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { ImportSession } from "../../types/importV2";

export interface ImportV2HeaderProps {
  session: ImportSession | null;
}

export function ImportV2Header({ session }: ImportV2HeaderProps) {
  const { t } = useTranslation();
  const total = session?.items.length ?? 0;
  const completed = session?.items.filter((item) => item.status === "completed").length ?? 0;
  return (
    <header className="import-v2-header">
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <FileOutput size={18} className="shrink-0 text-[var(--accent)]" aria-hidden="true" />
          <h1 className="m-0 text-[20px] font-semibold tracking-[-0.02em]">{t("importV2.header.title")}</h1>
        </div>
        <p className="m-0 mt-1 max-w-[60ch] text-[13px] text-[var(--text-secondary)]">{t("importV2.header.subtitle")}</p>
      </div>
      <span className="import-v2-header__stat" aria-live="polite">{t("importV2.header.stats", { completed, total })}</span>
    </header>
  );
}
