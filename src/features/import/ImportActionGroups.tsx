import { GitMerge, KeyRound, LoaderCircle, LogIn, Mic2, PackagePlus, Play, ScanText } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { ImportSessionActionGroup, ImportSessionActionGroupKind } from "../../types/importV2";
import { capabilityDisplayName } from "./importCapabilityPresentation";
import type { ImportItemAction } from "./importStatusPresentation";

export type ImportActionGroupKind = ImportSessionActionGroupKind;

export interface ImportActionGroup extends ImportSessionActionGroup {
  action: ImportItemAction;
}

function actionForGroup(kind: ImportActionGroupKind): ImportItemAction {
  if (kind === "login") return "begin_login";
  if (kind === "ocr") return "enable_ocr";
  if (kind === "asr") return "authorize_local_asr";
  if (kind === "capability") return "view_capability";
  if (kind === "conflict") return "resolve_merge";
  return "retry";
}

const GROUP_ICONS = {
  login: LogIn,
  ocr: ScanText,
  asr: Mic2,
  capability: PackagePlus,
  conflict: GitMerge,
  resume: Play,
} satisfies Record<ImportActionGroupKind, typeof KeyRound>;

export interface ImportActionGroupsProps {
  groups: readonly ImportSessionActionGroup[];
  pendingItemIds?: ReadonlySet<string>;
  onRun: (group: ImportActionGroup) => void;
}

export function ImportActionGroups({
  groups: overviewGroups,
  pendingItemIds = new Set<string>(),
  onRun,
}: ImportActionGroupsProps) {
  const { t } = useTranslation();
  const groups = overviewGroups.map((group) => ({ ...group, action: actionForGroup(group.kind) }));
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
                  {t(`importV2.actionGroups.${group.kind}.title`, { count: group.itemCount })}
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
