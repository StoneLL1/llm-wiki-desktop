import { CheckCircle2, CircleAlert, Package, Route } from "lucide-react";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";

import type { ImportCapabilityReadiness } from "../../types/importV2Presentation";

export interface ImportCapabilitiesPanelProps {
  capabilities: readonly ImportCapabilityReadiness[];
}

interface CapabilityGroup {
  id: string;
  routes: ImportCapabilityReadiness[];
}

function groupStatus(group: CapabilityGroup): "ready" | "partial" | "missing" {
  const available = group.routes.filter((route) => route.available).length;
  return available === group.routes.length ? "ready" : available > 0 ? "partial" : "missing";
}

export function ImportCapabilitiesPanel({ capabilities }: ImportCapabilitiesPanelProps) {
  const { t } = useTranslation();
  const groups = useMemo(() => {
    const byId = new Map<string, CapabilityGroup>();
    for (const capability of capabilities) {
      const current = byId.get(capability.capabilityId) ?? {
        id: capability.capabilityId,
        routes: [],
      };
      const routeIndex = current.routes.findIndex((route) => route.route === capability.route);
      if (routeIndex >= 0) current.routes[routeIndex] = capability;
      else current.routes.push(capability);
      byId.set(capability.capabilityId, current);
    }
    return [...byId.values()].sort((left, right) =>
      Number(groupStatus(right) === "ready") - Number(groupStatus(left) === "ready")
      || left.id.localeCompare(right.id));
  }, [capabilities]);

  return (
    <section className="import-v2-capabilities" aria-labelledby="import-capabilities-title">
      <header className="import-v2-section-header">
        <div>
          <p className="import-v2-section-header__eyebrow">{t("importV2.capabilities.eyebrow")}</p>
          <h2 id="import-capabilities-title">{t("importV2.capabilities.title")}</h2>
          <p>{t("importV2.capabilities.description")}</p>
        </div>
        <span className="import-v2-header__stat">
          {t("importV2.capabilities.summary", {
            available: groups.filter((group) => groupStatus(group) === "ready").length,
            total: groups.length,
          })}
        </span>
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
                  <h3>{group.id}</h3>
                  <div className="import-v2-capability-row__routes">
                    <Route size={12} aria-hidden="true" />
                    <span>
                      {group.routes.map((route) => (
                        <span
                          key={route.route}
                          className={route.available ? "is-ready" : "is-missing"}
                          title={route.reasonCode ?? undefined}
                        >
                          <span>{route.route}</span>
                          <span aria-hidden="true">
                            {route.available ? " ✓" : ` — ${route.reasonCode ?? t("importV2.capabilities.missing")}`}
                          </span>
                        </span>
                      ))}
                    </span>
                  </div>
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
