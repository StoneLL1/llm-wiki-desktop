import { CircleAlert, LoaderCircle, Package, RefreshCw, Search } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { ActionableErrorNotice } from "../../components/app/ActionableErrorNotice";
import {
  appCapabilityPrimaryAction,
  matchesAppCapabilityStatus,
  type AppCapabilityCategoryFilter,
  type AppCapabilityStatusFilter,
  useAppCapabilityStore,
} from "../../stores/appCapabilityStore";
import type { AppCapabilityView } from "../../types/appCapability";

function formatBytes(value: number | undefined): string {
  if (value === undefined) return "—";
  const units = ["B", "KiB", "MiB", "GiB"];
  let amount = value;
  let unit = 0;
  while (amount >= 1024 && unit < units.length - 1) {
    amount /= 1024;
    unit += 1;
  }
  return `${amount >= 10 || unit === 0 ? amount.toFixed(0) : amount.toFixed(1)} ${units[unit]}`;
}

const ACTIVE_OPERATION_STATES = new Set([
  "queued", "downloading", "paused", "verifying", "installing",
  "health_checking", "activating", "recovering",
]);

function statusKey(capability: AppCapabilityView): string {
  if (capability.operation.state && ACTIVE_OPERATION_STATES.has(capability.operation.state)) {
    return `importV2.capabilityManagement.state.${capability.operation.state}`;
  }
  return `importV2.capabilityManagement.state.${capability.displayState}`;
}

function progressLabel(capability: AppCapabilityView): string | null {
  const current = capability.operation.progressCurrent;
  const total = capability.operation.progressTotal;
  if (capability.operation.state !== "downloading" || current === undefined || total === undefined || total <= 0) return null;
  return `${Math.min(100, Math.round((current / total) * 100))}%`;
}

function statusTone(capability: AppCapabilityView): "ok" | "busy" | "warn" | "neutral" {
  const action = appCapabilityPrimaryAction(capability);
  if (action === "cancel" || action === "continue") return "busy";
  if (action === "retry" || capability.installation.state === "unhealthy") return "warn";
  if (capability.installation.state === "healthy") return "ok";
  return "neutral";
}

const CATEGORY_FILTERS: AppCapabilityCategoryFilter[] = ["all", "documents", "web", "ocr", "media_asr"];
const STATUS_FILTERS: AppCapabilityStatusFilter[] = ["all", "installed", "available", "updating", "active", "attention", "unpublished"];

export function ImportCapabilitiesPanel() {
  const { t } = useTranslation();
  const capabilities = useAppCapabilityStore((state) => state.capabilities);
  const initialized = useAppCapabilityStore((state) => state.initialized);
  const loading = useAppCapabilityStore((state) => state.loading);
  const error = useAppCapabilityStore((state) => state.error);
  const actionError = useAppCapabilityStore((state) => state.actionError);
  const actionErrorCapabilityId = useAppCapabilityStore((state) => state.actionErrorCapabilityId);
  const actionErrorOperation = useAppCapabilityStore((state) => state.actionErrorOperation);
  const search = useAppCapabilityStore((state) => state.search);
  const categoryFilter = useAppCapabilityStore((state) => state.categoryFilter);
  const stateFilter = useAppCapabilityStore((state) => state.statusFilter);
  const refresh = useAppCapabilityStore((state) => state.refresh);
  const openDialog = useAppCapabilityStore((state) => state.openDialog);
  const setSearch = useAppCapabilityStore((state) => state.setSearch);
  const setCategoryFilter = useAppCapabilityStore((state) => state.setCategoryFilter);
  const setStatusFilter = useAppCapabilityStore((state) => state.setStatusFilter);
  const continueInstall = useAppCapabilityStore((state) => state.continueInstall);
  const cancelInstall = useAppCapabilityStore((state) => state.cancelInstall);
  const confirmInstall = useAppCapabilityStore((state) => state.confirmInstall);
  const [activeAnnouncement, setActiveAnnouncement] = useState("");
  const announcedStates = useRef(new Map<string, string>());

  const summary = useMemo(() => ({
    installed: capabilities.filter((capability) => capability.installation.state === "healthy").length,
    available: capabilities.filter((capability) => appCapabilityPrimaryAction(capability) === "install").length,
    updates: capabilities.filter((capability) => capability.update.state === "available").length,
    active: capabilities.filter((capability) => ["cancel", "continue"].includes(appCapabilityPrimaryAction(capability))).length,
    attention: capabilities.filter((capability) => matchesAppCapabilityStatus(capability, "attention")).length,
    unpublished: capabilities.filter((capability) => capability.distribution.state === "not_published_for_target").length,
  }), [capabilities]);

  const filtered = useMemo(() => {
    const query = search.trim().toLocaleLowerCase();
    return capabilities.filter((capability) => {
      const categoryMatches = categoryFilter === "all" || capability.category === categoryFilter;
      const statusMatches = matchesAppCapabilityStatus(capability, stateFilter);
      const searchable = [t(capability.nameKey), t(capability.purposeKey), capability.capabilityId, ...capability.routes, ...capability.formats].join(" ").toLocaleLowerCase();
      return categoryMatches && statusMatches && (!query || searchable.includes(query));
    });
  }, [capabilities, categoryFilter, search, stateFilter, t]);

  useEffect(() => {
    const changes: string[] = [];
    for (const capability of capabilities) {
      const state = capability.operation.state;
      if (!state) continue;
      const progress = progressLabel(capability);
      const signature = `${statusKey(capability)}:${progress ?? ""}`;
      const previous = announcedStates.current.get(capability.capabilityId);
      announcedStates.current.set(capability.capabilityId, signature);
      if (previous === signature || (previous === undefined && !ACTIVE_OPERATION_STATES.has(state))) continue;
      changes.push(`${t(capability.nameKey)}: ${t(statusKey(capability))}${progress ? ` ${progress}` : ""}`);
    }
    if (changes.length > 0) setActiveAnnouncement(changes.slice(0, 2).join(". "));
  }, [capabilities, t]);

  const runPrimaryAction = (capability: AppCapabilityView) => {
    const action = appCapabilityPrimaryAction(capability);
    if (action === "continue") {
      void continueInstall(capability.capabilityId).catch(() => undefined);
      return;
    }
    if (action === "cancel") {
      void cancelInstall(capability.capabilityId).catch(() => undefined);
      return;
    }
    openDialog(capability.capabilityId, action);
  };

  const retryActionFailure = async () => {
    if (!actionErrorCapabilityId || !actionErrorOperation) return;
    if (["APP_CAPABILITY_TASK_REVISION_STALE", "APP_CAPABILITY_VERSION_STALE", "APP_CAPABILITY_ACKNOWLEDGEMENT_STALE"].includes(actionError?.code ?? "")) {
      await refresh(true);
      return;
    }
    if (actionErrorOperation === "install") await confirmInstall(actionErrorCapabilityId);
    else if (actionErrorOperation === "continue") await continueInstall(actionErrorCapabilityId);
    else await cancelInstall(actionErrorCapabilityId);
  };

  return (
    <section className="import-v2-capabilities" aria-labelledby="import-capabilities-title">
      <header className="import-v2-section-header import-v2-capabilities__header">
        <div>
          <p className="import-v2-section-header__eyebrow">{t("importV2.capabilities.eyebrow")}</p>
          <h2 id="import-capabilities-title">{t("importV2.capabilityManagement.title")}</h2>
          <p>{t("importV2.capabilityManagement.description")}</p>
        </div>
        <button className="icon-button" type="button" onClick={() => void refresh(true).catch(() => undefined)} disabled={loading} aria-label={t("importV2.capabilityManagement.refresh")} title={t("importV2.capabilityManagement.refresh")}>
          <RefreshCw size={14} className={loading ? "animate-spin" : ""} aria-hidden="true" />
        </button>
      </header>

      <div className="import-v2-capability-summary" aria-label={t("importV2.capabilityManagement.summaryLabel")}>
        {(["installed", "available", "updates", "active", "attention", "unpublished"] as const).map((key) => (
          <div key={key}><strong>{summary[key]}</strong><span>{t(`importV2.capabilityManagement.summary.${key}`)}</span></div>
        ))}
      </div>

      <div className="import-v2-capability-filters">
        <label className="import-v2-capability-search">
          <Search size={13} aria-hidden="true" />
          <span className="sr-only">{t("importV2.capabilityManagement.search")}</span>
          <input value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t("importV2.capabilityManagement.searchPlaceholder")} />
        </label>
        <label>
          <span>{t("importV2.capabilityManagement.category")}</span>
          <select value={categoryFilter} onChange={(event) => setCategoryFilter(event.target.value as AppCapabilityCategoryFilter)}>
            {CATEGORY_FILTERS.map((filter) => <option key={filter} value={filter}>{t(`importV2.capabilityManagement.category.${filter}`)}</option>)}
          </select>
        </label>
        <label>
          <span>{t("importV2.capabilityManagement.status")}</span>
          <select value={stateFilter} onChange={(event) => setStatusFilter(event.target.value as AppCapabilityStatusFilter)}>
            {STATUS_FILTERS.map((filter) => <option key={filter} value={filter}>{t(`importV2.capabilityManagement.filter.${filter}`)}</option>)}
          </select>
        </label>
      </div>

      {error || actionError ? <ActionableErrorNotice error={(actionError ?? error)!} onAction={async (kind) => {
        if (kind !== "retry") return;
        if (error) await refresh(true);
        else await retryActionFailure();
      }} /> : null}

      {!initialized || (loading && capabilities.length === 0) ? (
        <div className="import-v2-state" role="status"><LoaderCircle size={15} className="animate-spin" aria-hidden="true" />{t("importV2.common.loading")}</div>
      ) : filtered.length === 0 ? (
        <div className="import-v2-state"><CircleAlert size={15} aria-hidden="true" />{t(capabilities.length === 0 ? "importV2.capabilityManagement.emptyCatalog" : "importV2.capabilityManagement.noMatches")}</div>
      ) : (
        <div className="import-v2-capability-table-wrap">
          <table className="import-v2-capability-table">
            <thead><tr>
              <th scope="col">{t("importV2.capabilityManagement.column.name")}</th>
              <th scope="col">{t("importV2.capabilityManagement.column.coverage")}</th>
              <th scope="col">{t("importV2.capabilityManagement.column.state")}</th>
              <th scope="col">{t("importV2.capabilityManagement.column.size")}</th>
              <th scope="col">{t("importV2.capabilityManagement.column.action")}</th>
            </tr></thead>
            <tbody>
              {filtered.map((capability) => {
                const action = appCapabilityPrimaryAction(capability);
                const progress = progressLabel(capability);
                const coverage = [...capability.formats, ...capability.platformContentTypes, ...capability.routes];
                return (
                  <tr key={capability.capabilityId}>
                    <th scope="row">
                      <span className="import-v2-capability-table__name"><Package size={14} aria-hidden="true" />{t(capability.nameKey)}</span>
                      <span>{t(capability.purposeKey)}</span>
                    </th>
                    <td><span className="import-v2-capability-table__coverage" title={coverage.join(" · ")} aria-label={coverage.join(" · ") || "—"}>{coverage.slice(0, 5).join(" · ") || "—"}{coverage.length > 5 ? ` +${coverage.length - 5}` : ""}</span></td>
                    <td>
                      <span className={`import-v2-capability-state is-${statusTone(capability)}`}>
                        <span className={`dotstatus${statusTone(capability) === "neutral" ? "" : ` dotstatus--${statusTone(capability) === "ok" ? "ok" : statusTone(capability) === "busy" ? "busy" : "err"}`}`} aria-hidden="true" />
                        {t(statusKey(capability), { version: capability.targetVersion ?? capability.installation.healthyVersion ?? "—" })}{progress ? ` ${progress}` : ""}
                      </span>
                      <span className="import-v2-capability-table__meta font-mono">{t("importV2.capabilityManagement.versionFacts", {
                        installed: capability.installation.healthyVersion ?? "—",
                        target: capability.targetVersion ?? "—",
                      })}</span>
                      <span className="import-v2-capability-table__meta">{t("importV2.capabilityManagement.stateFacts", {
                        distribution: t(`importV2.capabilityManagement.distribution.${capability.distribution.state}`),
                        installation: t(`importV2.capabilityManagement.installation.${capability.installation.state}`),
                        update: t(`importV2.capabilityManagement.update.${capability.update.state}`),
                      })}</span>
                      {capability.operation.state === "downloading" && capability.operation.progressTotal ? <progress max={capability.operation.progressTotal} value={capability.operation.progressCurrent ?? 0} aria-label={t("importV2.capabilityManagement.downloadProgress", { name: t(capability.nameKey), current: formatBytes(capability.operation.progressCurrent), total: formatBytes(capability.operation.progressTotal) })} /> : null}
                    </td>
                    <td className="font-mono text-[11px]" title={t("importV2.capabilityManagement.sizeTitle", { package: formatBytes(capability.compressedBytes), model: formatBytes(capability.modelBytes), installed: formatBytes(capability.installedBytes) })}>
                      <span>{formatBytes(capability.compressedBytes ?? capability.installedBytes)}</span>
                      <span className="import-v2-capability-table__meta">{t("importV2.capabilityManagement.sizeMeta", { model: formatBytes(capability.modelBytes), installed: formatBytes(capability.installedBytes) })}</span>
                      <span className="import-v2-capability-table__meta">{capability.licenseExpression}</span>
                    </td>
                    <td><button type="button" className={`btn btn--sm${["install", "update", "retry", "continue"].includes(action) ? " btn--primary" : ""}`} onClick={() => runPrimaryAction(capability)}>{t(`importV2.capabilityManagement.action.${action}`)}</button></td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      <p className="sr-only" role="status" aria-live="polite" aria-atomic="true">{activeAnnouncement}</p>
    </section>
  );
}
