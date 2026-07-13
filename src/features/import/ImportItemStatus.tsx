import { CheckCircle2, CircleAlert, CircleDashed, CircleSlash2, FileCheck2, GitMerge, KeyRound, LoaderCircle, LogIn, PauseCircle, ScanLine, ShieldCheck, SkipForward, Timer } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ImportItem } from "../../types/importV2";
import { presentImportItem, type ImportItemIcon, type ImportItemPresentation } from "./importStatusPresentation";

const ICONS: Record<ImportItemIcon, typeof CircleDashed> = {
  queue: Timer,
  scan: ScanLine,
  capability: KeyRound,
  login: LogIn,
  shield: ShieldCheck,
  ready: FileCheck2,
  merge: GitMerge,
  commit: LoaderCircle,
  completed: CheckCircle2,
  pause: PauseCircle,
  cancelled: CircleSlash2,
  skipped: SkipForward,
  failed: CircleAlert,
};

export interface ImportItemStatusProps {
  item: ImportItem;
  presentation?: ImportItemPresentation;
}

export function ImportItemStatus({ item, presentation = presentImportItem(item) }: ImportItemStatusProps) {
  const { t } = useTranslation();
  const Icon = ICONS[presentation.icon];
  return (
    <div className={`import-v2-item-status is-${presentation.tone}`} data-testid={`import-status-${item.itemId}`}>
      <span className="flex items-center gap-1.5 text-[12px] font-medium" aria-label={t(presentation.labelKey)}>
        <Icon size={14} className={presentation.icon === "commit" ? "animate-spin" : undefined} />
        {t(presentation.labelKey)}
      </span>
      {presentation.progressMode !== "none" ? (
        <div className="mt-1.5 flex items-center gap-2">
          <div className="h-1.5 min-w-[72px] flex-1 overflow-hidden rounded-full bg-[var(--surface-muted)]" aria-hidden="true">
            <div
              className={`h-full rounded-full bg-current ${presentation.progressMode === "indeterminate" ? "w-1/2 animate-pulse" : ""}`}
              style={presentation.progressValue === null ? undefined : { width: `${presentation.progressValue}%` }}
            />
          </div>
          {presentation.progressValue !== null ? <span className="font-mono text-[10.5px]">{presentation.progressValue}%</span> : null}
        </div>
      ) : null}
      {presentation.progressLabel ? <span className="mt-0.5 block truncate text-[10.5px] text-[var(--text-muted)]">{presentation.progressLabel}</span> : null}
    </div>
  );
}
