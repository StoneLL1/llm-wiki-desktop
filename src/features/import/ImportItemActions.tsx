import { Bot, CheckCircle2, Eye, GitCompareArrows, KeyRound, LogIn, RotateCcw, ScanEye, Trash2, Upload, X, Zap } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ImportItem } from "../../types/importV2";
import { presentImportItem, type ImportItemAction, type ImportItemPresentation } from "./importStatusPresentation";

const ACTION_ICONS: Record<ImportItemAction, typeof Eye> = {
  inspect: ScanEye,
  start: Zap,
  retry: RotateCcw,
  cancel: X,
  preview_markdown: Eye,
  begin_login: LogIn,
  authorize_private_target: KeyRound,
  view_capability: Upload,
  invoke_local_agent: Bot,
  request_byok: Bot,
  compare_candidate: GitCompareArrows,
  discard_candidate: Trash2,
  resolve_merge: GitCompareArrows,
  open_result: CheckCircle2,
};

export interface ImportItemActionsProps {
  item: ImportItem;
  presentation?: ImportItemPresentation;
  onAction: (action: ImportItemAction, itemId: string) => void;
}

export function ImportItemActions({ item, presentation = presentImportItem(item), onAction }: ImportItemActionsProps) {
  const { t } = useTranslation();
  return (
    <div className="flex shrink-0 items-center gap-0.5" aria-label={t("importV2.queue.actions") }>
      {presentation.actions.map((action) => {
        const Icon = ACTION_ICONS[action];
        const label = t(`importV2.action.${action}`);
        return (
          <button
            key={action}
            type="button"
            className="btn btn--ghost btn--icon btn--sm"
            aria-label={`${label} ${item.input.displayName}`}
            title={label}
            onClick={(event) => {
              event.stopPropagation();
              onAction(action, item.itemId);
            }}
          >
            <Icon size={14} />
          </button>
        );
      })}
    </div>
  );
}
