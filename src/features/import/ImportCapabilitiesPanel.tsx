import { CheckCircle2, CircleAlert, Package } from "lucide-react";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";

import type { ImportItem } from "../../types/importV2";
import type { ImportCapabilityReadiness } from "../../types/importV2Presentation";
import type { ImportItemAction } from "./importStatusPresentation";
import { capabilityDisplayName } from "./importCapabilityPresentation";

export interface ImportCapabilitiesPanelProps {
  capabilities: readonly ImportCapabilityReadiness[];
  items?: readonly ImportItem[];
  onAction?: (action: ImportItemAction, itemId: string) => void;
}

interface CapabilityGroup {
  id: string;
  entries: ImportCapabilityReadiness[];
}

function groupStatus(group: CapabilityGroup): "ready" | "partial" | "missing" {
  const available = group.entries.filter((entry) => entry.available).length;
  return available === group.entries.length ? "ready" : available > 0 ? "partial" : "missing";
}

export function ImportCapabilitiesPanel({
  capabilities,
  items = [],
  onAction,
}: ImportCapabilitiesPanelProps) {
  const { t } = useTranslation();
  const groups = useMemo(() => {
    const byId = new Map<string, CapabilityGroup>();
    for (const capability of capabilities) {
      const current = byId.get(capability.capabilityId) ?? {
        id: capability.capabilityId,
        entries: [],
      };
      const entryIndex = current.entries.findIndex((entry) => entry.route === capability.route);
      if (entryIndex >= 0) current.entries[entryIndex] = capability;
      else current.entries.push(capability);
      byId.set(capability.capabilityId, current);
    }
    return [...byId.values()].sort((left, right) =>
      Number(groupStatus(right) === "ready") - Number(groupStatus(left) === "ready")
      || left.id.localeCompare(right.id));
  }, [capabilities]);
  const blocker = items.find((item) => item.status === "waiting_capability") ?? null;

  return (
    <section className="import-v2-capabilities" aria-labelledby="import-capabilities-title">
      <header className="import-v2-section-header">
        <div>
          <p className="import-v2-section-header__eyebrow">{t("importV2.capabilities.eyebrow")}</p>
          <h2 id="import-capabilities-title">{t("importV2.capabilities.title")}</h2>
          <p>{t("importV2.capabilities.description")}</p>
        </div>
        <div className="flex items-center gap-2">
          <span className="import-v2-header__stat">
            {t("importV2.capabilities.summary", {
              available: groups.filter((group) => groupStatus(group) === "ready").length,
              total: groups.length,
            })}
          </span>
          {blocker && onAction ? (
            <button
              type="button"
              className="btn btn--sm"
              onClick={() => onAction("view_capability", blocker.itemId)}
            >
              {t("importV2.capabilities.manage")}
            </button>
          ) : null}
        </div>
      </header>
      {groups.length === 0 ? (
        <div className="import-v2-state">{t("importV2.capabilities.empty")}</div>
      ) : (
        <div className="import-v2-capabilities__list">
          {groups.map((group) => {
            const status = groupStatus(group);
            return (
              <article key={group.id} className="import-v2-capability-row">
                <Package size={16} className="text-[var(--text-muted)]" aria-hidden="true" />
                <div className="min-w-0">
                  <h3>{capabilityDisplayName(group.id, t)}</h3>
                  <p className="m-0 mt-1 text-[10.5px] text-[var(--text-muted)]">
                    {t("importV2.capabilities.supportCount", { count: group.entries.length })}
                  </p>
                </div>
                <span className={`import-v2-capability-row__status is-${status}`}>
                  {status === "ready"
                    ? <CheckCircle2 size={14} aria-hidden="true" />
                    : <CircleAlert size={14} aria-hidden="true" />}
                  {t(`importV2.capabilities.${status}`)}
                </span>
              </article>
            );
          })}
        </div>
      )}
      <p className="import-v2-capabilities__note">{t("importV2.capabilities.installNote")}</p>
    </section>
  );
}
