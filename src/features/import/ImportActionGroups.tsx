import { KeyRound, LoaderCircle, LogIn, Mic2, PackagePlus, Play, ScanText } from "lucide-react";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";

import type { ImportItem } from "../../types/importV2";
import { capabilityDisplayName } from "./importCapabilityPresentation";
import { importPlatformForLocator } from "./importLocator";
import { presentImportItem, type ImportItemAction } from "./importStatusPresentation";

export type ImportActionGroupKind = "login" | "ocr" | "asr" | "capability" | "resume";

export interface ImportActionGroup {
  groupKey: string;
  kind: ImportActionGroupKind;
  subjectId: string | null;
  itemIds: readonly string[];
  action: ImportItemAction;
}

function capabilityIdForItem(item: ImportItem): string {
  const actions = item.issue?.recoveryActions ?? [];
  if (actions.includes("install_browser_capability")) return "browser-runtime";
  if (actions.includes("install_media_capability")) return "asr-sensevoice-small";
  if (actions.includes("install_ocr_capability")) return "ocr-cjk-accurate";
  return "document-standard";
}

export function buildImportActionGroups(items: readonly ImportItem[]): ImportActionGroup[] {
  const groups = new Map<string, ImportActionGroup>();
  for (const item of items) {
    const presentation = presentImportItem(item);
    const group = (() => {
      if (item.status === "paused") {
        return { kind: "resume" as const, action: "retry" as const, subjectId: null };
      }
      switch (presentation.primaryAction) {
        case "begin_login": {
          const locator = item.input.normalizedLocator ?? item.input.locator;
          return {
            kind: "login" as const,
            action: "begin_login" as const,
            subjectId: importPlatformForLocator(locator),
          };
        }
        case "enable_ocr":
          return { kind: "ocr" as const, action: "enable_ocr" as const, subjectId: null };
        case "authorize_local_asr":
          return { kind: "asr" as const, action: "authorize_local_asr" as const, subjectId: null };
        case "view_capability":
          return {
            kind: "capability" as const,
            action: "view_capability" as const,
            subjectId: capabilityIdForItem(item),
          };
        default:
          return null;
      }
    })();
    if (!group) continue;
    const groupKey = `${group.kind}:${group.subjectId ?? "all"}`;
    const current = groups.get(groupKey);
    groups.set(groupKey, {
      ...group,
      groupKey,
      itemIds: [...(current?.itemIds ?? []), item.itemId],
    });
  }
  return [...groups.values()];
}

const GROUP_ICONS = {
  login: LogIn,
  ocr: ScanText,
  asr: Mic2,
  capability: PackagePlus,
  resume: Play,
} satisfies Record<ImportActionGroupKind, typeof KeyRound>;

export interface ImportActionGroupsProps {
  items: readonly ImportItem[];
  pendingItemIds?: ReadonlySet<string>;
  onRun: (group: ImportActionGroup) => void;
}

export function ImportActionGroups({
  items,
  pendingItemIds = new Set<string>(),
  onRun,
}: ImportActionGroupsProps) {
  const { t } = useTranslation();
  const groups = useMemo(() => buildImportActionGroups(items), [items]);
  if (groups.length === 0) return null;

  return (
    <section className="import-v2-action-groups" aria-labelledby="import-action-groups-title">
      <div className="import-v2-action-groups__header">
        <h2 id="import-action-groups-title">{t("importV2.actionGroups.title")}</h2>
        <span>{t("importV2.actionGroups.summary", { count: groups.length })}</span>
      </div>
      <div className="import-v2-action-groups__list">
        {groups.map((group) => {
          const Icon = GROUP_ICONS[group.kind];
          const pending = group.itemIds.some((itemId) => pendingItemIds.has(itemId));
          const subject = group.kind === "login" && group.subjectId
            ? t(`importV2.platform.${group.subjectId}`, { defaultValue: group.subjectId })
            : group.kind === "capability" && group.subjectId
              ? capabilityDisplayName(group.subjectId, t)
              : null;
          return (
            <article key={group.groupKey} className="import-v2-action-group">
              <Icon size={15} aria-hidden="true" />
              <div className="min-w-0 flex-1">
                <strong>
                  {subject ? `${subject} · ` : ""}
                  {t(`importV2.actionGroups.${group.kind}.title`, { count: group.itemIds.length })}
                </strong>
                <span>{t(`importV2.actionGroups.${group.kind}.description`)}</span>
              </div>
              <button
                type="button"
                className="btn btn--sm"
                disabled={pending}
                onClick={() => onRun(group)}
              >
                {pending ? <LoaderCircle size={13} className="animate-spin" aria-hidden="true" /> : null}
                {t(`importV2.actionGroups.${group.kind}.action`)}
              </button>
            </article>
          );
        })}
      </div>
    </section>
  );
}
