import {
  Bot,
  CheckCircle2,
  Copy,
  Download,
  Eye,
  FileText,
  GitCompareArrows,
  KeyRound,
  ListChecks,
  LoaderCircle,
  LogIn,
  MoreHorizontal,
  RotateCcw,
  ScanEye,
  SkipForward,
  Trash2,
  Upload,
  X,
  Zap,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import type { ImportItem } from "../../types/importV2";
import {
  presentImportItem,
  type ImportItemAction,
  type ImportItemPresentation,
} from "./importStatusPresentation";

const ACTION_ICONS: Record<ImportItemAction, typeof Eye> = {
  inspect: ScanEye,
  start: Zap,
  retry: RotateCcw,
  retry_route: RotateCcw,
  switch_route: RotateCcw,
  switch_parser: ScanEye,
  enable_ocr: ScanEye,
  skip: SkipForward,
  authorize_local_asr: Upload,
  select_subtitle: ListChecks,
  cancel: X,
  preview_markdown: Eye,
  preserve_remote_media: Download,
  begin_login: LogIn,
  authorize_private_target: KeyRound,
  view_capability: Upload,
  invoke_local_agent: Bot,
  view_log: FileText,
  compare_candidate: GitCompareArrows,
  discard_candidate: Trash2,
  resolve_merge: GitCompareArrows,
  open_result: CheckCircle2,
};

export interface ImportItemActionsProps {
  item: ImportItem;
  presentation?: ImportItemPresentation;
  pending?: boolean;
  onAction: (action: ImportItemAction, itemId: string) => void;
  onCopyLocator?: (locator: string) => void | Promise<void>;
}

export function ImportItemActions({
  item,
  presentation = presentImportItem(item),
  pending = false,
  onAction,
  onCopyLocator,
}: ImportItemActionsProps) {
  const { t } = useTranslation();
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menuOpen) return;
    const closeOutside = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) setMenuOpen(false);
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setMenuOpen(false);
        menuRef.current?.querySelector<HTMLButtonElement>("[aria-haspopup='menu']")?.focus();
      }
    };
    document.addEventListener("pointerdown", closeOutside);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("pointerdown", closeOutside);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [menuOpen]);

  if (pending) {
    return (
      <span
        className="flex shrink-0 items-center gap-1.5 text-[11px] text-[var(--text-muted)]"
        role="status"
        aria-label={t("importV2.status.updating")}
      >
        <LoaderCircle size={14} className="animate-spin" aria-hidden="true" />
        {t("importV2.status.updating")}
      </span>
    );
  }

  const primaryAction = presentation.primaryAction;
  const secondaryActions = presentation.secondaryActions;
  const PrimaryIcon = primaryAction ? ACTION_ICONS[primaryAction] : null;
  const hasMenu = secondaryActions.length > 0 || Boolean(onCopyLocator);

  return (
    <div
      className="import-v2-item-actions relative flex shrink-0 items-center justify-end gap-1"
      aria-label={t("importV2.queue.actions")}
      ref={menuRef}
    >
      {primaryAction && PrimaryIcon ? (
        <button
          type="button"
          className="btn btn--sm"
          onClick={(event) => {
            event.stopPropagation();
            onAction(primaryAction, item.itemId);
          }}
        >
          <PrimaryIcon size={13} aria-hidden="true" />
          {t(`importV2.action.${primaryAction}`)}
        </button>
      ) : null}
      {hasMenu ? (
        <>
          <button
            type="button"
            className="btn btn--ghost btn--icon btn--sm"
            aria-haspopup="menu"
            aria-expanded={menuOpen}
            aria-label={t("importV2.queue.moreActions", { name: item.input.displayName })}
            title={t("importV2.queue.more")}
            onClick={(event) => {
              event.stopPropagation();
              setMenuOpen((open) => !open);
            }}
          >
            <MoreHorizontal size={15} aria-hidden="true" />
          </button>
          {menuOpen ? (
            <div
              className="import-v2-action-menu"
              role="menu"
              onClick={(event) => event.stopPropagation()}
            >
              {secondaryActions.map((action) => {
                const Icon = ACTION_ICONS[action];
                return (
                  <button
                    key={action}
                    type="button"
                    role="menuitem"
                    onClick={() => {
                      setMenuOpen(false);
                      onAction(action, item.itemId);
                    }}
                  >
                    <Icon size={13} aria-hidden="true" />
                    {t(`importV2.action.${action}`)}
                  </button>
                );
              })}
              {onCopyLocator ? (
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    setMenuOpen(false);
                    void onCopyLocator(item.input.locator);
                  }}
                >
                  <Copy size={13} aria-hidden="true" />
                  {t("importV2.queue.copy")}
                </button>
              ) : null}
            </div>
          ) : null}
        </>
      ) : null}
    </div>
  );
}
