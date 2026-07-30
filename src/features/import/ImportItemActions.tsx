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
import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
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

const ACTION_MENU_GUTTER = 8;
const ACTION_MENU_OFFSET = 4;

interface ActionMenuPosition {
  top: number;
  left: number;
  maxHeight: number;
  side: "top" | "bottom";
}

function actionMenuPosition(
  triggerRect: DOMRect,
  menuSize: { width: number; height: number },
): ActionMenuPosition {
  const spaceBelow = Math.max(
    0,
    window.innerHeight - triggerRect.bottom - ACTION_MENU_OFFSET - ACTION_MENU_GUTTER,
  );
  const spaceAbove = Math.max(
    0,
    triggerRect.top - ACTION_MENU_OFFSET - ACTION_MENU_GUTTER,
  );
  const side = menuSize.height <= spaceBelow || spaceBelow >= spaceAbove
    ? "bottom"
    : "top";
  const maxHeight = side === "bottom" ? spaceBelow : spaceAbove;
  const visibleHeight = Math.min(menuSize.height, maxHeight);
  const top = side === "bottom"
    ? triggerRect.bottom + ACTION_MENU_OFFSET
    : triggerRect.top - ACTION_MENU_OFFSET - visibleHeight;
  const maxLeft = Math.max(
    ACTION_MENU_GUTTER,
    window.innerWidth - ACTION_MENU_GUTTER - menuSize.width,
  );
  const left = Math.min(
    Math.max(ACTION_MENU_GUTTER, triggerRect.right - menuSize.width),
    maxLeft,
  );

  return { top, left, maxHeight, side };
}

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
  const [menuPosition, setMenuPosition] = useState<ActionMenuPosition | null>(null);
  const menuId = useId();
  const actionsRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menuOpen) return;
    const closeOutside = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) return;
      if (
        !actionsRef.current?.contains(target)
        && !menuRef.current?.contains(target)
      ) {
        setMenuOpen(false);
      }
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setMenuOpen(false);
        triggerRef.current?.focus();
      }
    };
    document.addEventListener("pointerdown", closeOutside);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("pointerdown", closeOutside);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [menuOpen]);

  useLayoutEffect(() => {
    if (!menuOpen) return;
    let frame = 0;
    const updatePosition = () => {
      const trigger = triggerRef.current;
      const menu = menuRef.current;
      if (!trigger || !menu) return;
      const menuRect = menu.getBoundingClientRect();
      setMenuPosition(actionMenuPosition(
        trigger.getBoundingClientRect(),
        {
          width: menuRect.width,
          height: Math.max(menuRect.height, menu.scrollHeight),
        },
      ));
    };
    const schedulePositionUpdate = () => {
      window.cancelAnimationFrame(frame);
      frame = window.requestAnimationFrame(updatePosition);
    };

    updatePosition();
    menuRef.current?.querySelector<HTMLButtonElement>("[role='menuitem']")?.focus();
    window.addEventListener("resize", schedulePositionUpdate);
    document.addEventListener("scroll", schedulePositionUpdate, true);
    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener("resize", schedulePositionUpdate);
      document.removeEventListener("scroll", schedulePositionUpdate, true);
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
      ref={actionsRef}
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
            ref={triggerRef}
            type="button"
            className="btn btn--ghost btn--icon btn--sm"
            aria-haspopup="menu"
            aria-expanded={menuOpen}
            aria-controls={menuOpen ? menuId : undefined}
            aria-label={t("importV2.queue.moreActions", { name: item.input.displayName })}
            title={t("importV2.queue.more")}
            onClick={(event) => {
              event.stopPropagation();
              setMenuOpen((open) => !open);
            }}
          >
            <MoreHorizontal size={15} aria-hidden="true" />
          </button>
          {menuOpen && typeof document !== "undefined" ? createPortal((
            <div
              id={menuId}
              ref={menuRef}
              className="import-v2-action-menu"
              role="menu"
              aria-label={t("importV2.queue.moreActions", { name: item.input.displayName })}
              data-side={menuPosition?.side}
              style={{
                top: menuPosition?.top ?? 0,
                left: menuPosition?.left ?? 0,
                maxHeight: menuPosition?.maxHeight,
                visibility: menuPosition ? "visible" : "hidden",
              }}
              onClick={(event) => event.stopPropagation()}
              onBlur={(event) => {
                const nextTarget = event.relatedTarget;
                if (
                  !(nextTarget instanceof Node)
                  || (
                    !actionsRef.current?.contains(nextTarget)
                    && !menuRef.current?.contains(nextTarget)
                  )
                ) {
                  setMenuOpen(false);
                }
              }}
              onKeyDown={(event) => {
                if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
                const items = Array.from(
                  menuRef.current?.querySelectorAll<HTMLButtonElement>("[role='menuitem']")
                  ?? [],
                );
                if (items.length === 0) return;
                event.preventDefault();
                const currentIndex = items.indexOf(document.activeElement as HTMLButtonElement);
                const nextIndex = event.key === "Home"
                  ? 0
                  : event.key === "End"
                    ? items.length - 1
                    : event.key === "ArrowUp"
                      ? (currentIndex <= 0 ? items.length - 1 : currentIndex - 1)
                      : (currentIndex + 1) % items.length;
                items[nextIndex]?.focus();
              }}
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
          ), document.body) : null}
        </>
      ) : null}
    </div>
  );
}
