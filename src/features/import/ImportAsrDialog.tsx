import { Cpu, HardDriveDownload, LoaderCircle, Mic2, ShieldCheck, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { ActionableErrorNotice } from "../../components/app/ActionableErrorNotice";
import { useModalDialog } from "../../hooks/useModalDialog";
import {
  normalizeBackendError,
  type NormalizedBackendError,
} from "../../lib/backendError";
import { cancelTaskRequest, useTaskStore } from "../../stores/taskStore";
import type { ImportAsrProfile } from "../../types/importV2";
import type { ImportAsrEnablementPlan, ImportAsrProfilePlan } from "../../types/importV2Presentation";
import type { BackendTask } from "../../types/task";
import type { AsrAuthorizationOptions } from "./importWorkflow";
import { readAsrPreference, writeAsrPreference } from "./asrPreferences";
import { capabilityInstallState } from "./capabilityInstallState";

export interface ImportAsrDialogProps {
  open: boolean;
  plan: ImportAsrEnablementPlan | null;
  loading: boolean;
  sessionId?: string | null;
  itemId?: string | null;
  onConfirm: (options: AsrAuthorizationOptions) => Promise<void> | void;
  onInstall: (capabilityId: string, options: AsrAuthorizationOptions) => Promise<BackendTask | null | void> | BackendTask | null | void;
  onCancel: () => void;
}

function canChoose(plan: ImportAsrProfilePlan): boolean {
  return plan.available || plan.installable;
}

function initialProfile(
  plan: ImportAsrEnablementPlan | null,
  stored: ImportAsrProfile | undefined,
): ImportAsrProfile {
  if (stored && plan?.profiles.some((entry) => entry.profile === stored && canChoose(entry))) {
    return stored;
  }
  return plan?.recommendedProfile ?? "balanced";
}

function formatBytes(value: number | null, locale: string, unknown: string): string {
  if (value === null) return unknown;
  const units = ["B", "KB", "MB", "GB", "TB"];
  let amount = value;
  let unit = 0;
  while (amount >= 1024 && unit < units.length - 1) {
    amount /= 1024;
    unit += 1;
  }
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: unit === 0 ? 0 : 1 }).format(amount)} ${units[unit]}`;
}

function formatDuration(value: number | null, locale: string, unknown: string): string {
  if (value === null) return unknown;
  if (value < 60) return `${new Intl.NumberFormat(locale).format(value)} s`;
  const minutes = Math.ceil(value / 60);
  return `${new Intl.NumberFormat(locale).format(minutes)} min`;
}

export function ImportAsrDialog({
  open,
  plan,
  loading,
  sessionId,
  itemId,
  onConfirm,
  onInstall,
  onCancel,
}: ImportAsrDialogProps) {
  const { t, i18n } = useTranslation();
  const initialFocusRef = useRef<HTMLInputElement>(null);
  const dialogRef = useModalDialog({ open, onClose: onCancel, initialFocusRef });
  const [profile, setProfile] = useState<ImportAsrProfile>("balanced");
  const [language, setLanguage] = useState("auto");
  const [remember, setRemember] = useState(false);
  const [busy, setBusy] = useState(false);
  const [startedTaskId, setStartedTaskId] = useState<string | null>(null);
  const [installError, setInstallError] = useState<NormalizedBackendError | null>(null);
  const tasks = useTaskStore((state) => state.tasks);

  useEffect(() => {
    if (!open) return;
    const stored = readAsrPreference();
    setProfile(initialProfile(plan, stored?.profile));
    setLanguage(stored?.language ?? "auto");
    setRemember(Boolean(stored));
    setBusy(false);
    setStartedTaskId(null);
    setInstallError(null);
  }, [open, plan]);

  const selected = useMemo(
    () => plan?.profiles.find((entry) => entry.profile === profile) ?? null,
    [plan, profile],
  );
  const capabilityTasks = useMemo(() => tasks.filter((task) => {
    const operation = task.operation;
    return operation?.kind === "capability_install"
      && operation.sessionId === sessionId
      && operation.itemId === itemId
      && operation.capabilityId === selected?.capabilityId
      && operation.requirementRevision === plan?.requirementRevision;
  }), [itemId, plan?.requirementRevision, selected?.capabilityId, sessionId, tasks]);
  const task = capabilityTasks.find((candidate) => candidate.id === startedTaskId)
    ?? capabilityTasks.toSorted((left, right) => right.updatedAt.localeCompare(left.updatedAt))[0]
    ?? null;
  const installState = capabilityInstallState(
    task,
    selected?.installable ?? false,
    selected?.available ?? false,
  );
  const taskBusy = task !== null && ["queued", "running", "cancelling"].includes(task.status);
  const blocked = busy || taskBusy;
  const taskError = task?.error ? normalizeBackendError(task.error, {
    defaultSummaryKey: "backendError.summary.importCapabilityUnavailable",
    defaultActionKind: "retry",
    defaultRecoverable: true,
  }) : null;

  if (!open) return null;

  async function submit() {
    if (blocked || !selected || !canChoose(selected)) return;
    setBusy(true);
    setInstallError(null);
    const options = { profile, language: language === "auto" ? null : language };
    try {
      writeAsrPreference(remember ? options : null);
      if (!selected.available && selected.installable) {
        const started = await onInstall(selected.capabilityId, options);
        if (started) setStartedTaskId(started.id);
      } else {
        await onConfirm(options);
      }
    } catch (error) {
      setInstallError(normalizeBackendError(error, {
        defaultSummaryKey: "backendError.summary.importCapabilityUnavailable",
        actionKindOverride: "retry",
        defaultRecoverable: true,
      }));
    } finally {
      setBusy(false);
    }
  }

  const unknown = t("importV2.asr.unknown");
  const ctaKey = selected?.available
    ? "importV2.asr.enable"
    : selected?.installable
      ? "importV2.asr.downloadAndEnable"
      : "importV2.asr.unavailable";

  return (
    <div ref={dialogRef} tabIndex={-1} className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 px-4" role="dialog" aria-modal="true" aria-labelledby="import-asr-title">
      <section className="flex max-h-[84vh] w-full max-w-[680px] flex-col rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-lg">
        <header className="flex min-h-[52px] items-center gap-3 border-b border-[var(--border)] px-4">
          <Mic2 size={17} className="text-[var(--accent)]" aria-hidden="true" />
          <h2 id="import-asr-title" className="m-0 flex-1 text-[15px] font-semibold">{t("importV2.asr.title")}</h2>
          <button type="button" className="icon-button" aria-label={t("importV2.asr.cancel")} title={t("importV2.asr.cancel")} onClick={onCancel}><X size={16} aria-hidden="true" /></button>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4 text-[12px]">
          <p className="m-0 max-w-[68ch] text-[var(--text-secondary)]">{t("importV2.asr.description")}</p>
          {loading ? (
            <p className="mt-4 flex items-center gap-2 text-[var(--text-muted)]">
              <LoaderCircle size={14} className="animate-spin" aria-hidden="true" />
              {t("importV2.asr.loadingPlan")}
            </p>
          ) : null}

          <fieldset className="mt-4 border-0 p-0" disabled={loading || blocked}>
            <legend className="mb-2 text-[12px] font-semibold">{t("importV2.asr.profile")}</legend>
            <div className="grid gap-2">
              {(plan?.profiles ?? []).map((entry, index) => (
                <label key={entry.profile} className="flex cursor-pointer items-start gap-2 rounded-[var(--radius-md)] border border-[var(--border-subtle)] px-3 py-2 has-[:checked]:border-[var(--accent)] has-[:disabled]:cursor-not-allowed has-[:disabled]:opacity-60">
                  <input
                    ref={index === 0 ? initialFocusRef : undefined}
                    type="radio"
                    name="asr-profile"
                    value={entry.profile}
                    checked={profile === entry.profile}
                    onChange={() => setProfile(entry.profile)}
                    disabled={!canChoose(entry)}
                  />
                  <span className="min-w-0 flex-1">
                    <span className="flex flex-wrap items-baseline gap-x-1 font-medium">
                      {t(`importV2.asr.profile.${entry.profile}`)}
                      {plan?.recommendedProfile === entry.profile ? <span className="text-[var(--accent)]">{t("importV2.asr.recommended")}</span> : null}
                      {!canChoose(entry) ? <span className="text-[var(--text-muted)]">{t("importV2.asr.profileUnavailable")}</span> : null}
                    </span>
                    <span className="mt-0.5 block text-[11px] text-[var(--text-muted)]">
                      {entry.engineName} · {entry.modelName}
                    </span>
                  </span>
                </label>
              ))}
            </div>
          </fieldset>

          <label className="mt-4 block">
            <span className="mb-1.5 block text-[12px] font-semibold">{t("importV2.asr.language")}</span>
            <select className="h-[30px] w-full rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface)] px-2 text-[12px]" value={language} onChange={(event) => setLanguage(event.target.value)} disabled={blocked || loading}>
              <option value="auto">{t("importV2.asr.language.auto")}</option>
              <option value="zh">{t("importV2.asr.language.zh")}</option>
              <option value="en">{t("importV2.asr.language.en")}</option>
              <option value="ja">{t("importV2.asr.language.ja")}</option>
              <option value="ko">{t("importV2.asr.language.ko")}</option>
            </select>
          </label>

          <dl className="mt-4 grid grid-cols-[132px_minmax(0,1fr)] gap-x-4 gap-y-1.5 rounded-[var(--radius-md)] border border-[var(--border-subtle)] px-3 py-2">
            <dt className="text-[var(--text-muted)]">{t("importV2.asr.download")}</dt>
            <dd className="m-0">{formatBytes(selected?.downloadBytes ?? null, i18n.language, unknown)}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.asr.disk")}</dt>
            <dd className="m-0">{formatBytes(selected?.installedBytes ?? null, i18n.language, unknown)}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.asr.device")}</dt>
            <dd className="m-0 flex items-center gap-1.5"><Cpu size={13} aria-hidden="true" />{selected?.device.toUpperCase() ?? unknown}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.asr.estimate")}</dt>
            <dd className="m-0">{formatDuration(selected?.estimatedSeconds ?? null, i18n.language, unknown)}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.asr.availableMemory")}</dt>
            <dd className="m-0">{formatBytes(plan?.availableMemoryBytes ?? null, i18n.language, unknown)}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.asr.availableDisk")}</dt>
            <dd className="m-0">{formatBytes(plan?.availableDiskBytes ?? null, i18n.language, unknown)}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.asr.mediaDuration")}</dt>
            <dd className="m-0">{formatDuration(plan?.mediaDurationSeconds ?? null, i18n.language, unknown)}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.asr.installLocation")}</dt>
            <dd className="m-0 break-all font-mono text-[11px]">{plan?.installLocation ?? unknown}</dd>
          </dl>

          {selected ? (
            <section className="mt-3 rounded-[var(--radius-md)] border border-[var(--border-subtle)] px-3 py-2" aria-labelledby="import-asr-dependencies">
              <h3 id="import-asr-dependencies" className="m-0 flex items-center gap-1.5 text-[12px] font-semibold">
                <HardDriveDownload size={14} aria-hidden="true" />
                {t("importV2.asr.dependencies")}
              </h3>
              <ol className="mt-2 grid list-decimal gap-1 pl-5 text-[11px] text-[var(--text-secondary)]">
                {selected.dependencies.map((dependency) => (
                  <li key={dependency.kind}>
                    <span className="font-medium text-[var(--text-primary)]">{dependency.name}</span>
                    {" · "}
                    {dependency.available ? t("importV2.asr.dependencyReady") : t("importV2.asr.dependencyPending")}
                    {" · "}
                    {dependency.license}
                    <span className="block break-all font-mono text-[10.5px] text-[var(--text-muted)]">
                      {dependency.source}
                    </span>
                  </li>
                ))}
              </ol>
            </section>
          ) : null}

          <p className="mt-3 flex items-start gap-2 rounded-[var(--radius-md)] border border-[var(--border-subtle)] px-3 py-2 text-[11px] text-[var(--text-secondary)]">
            <ShieldCheck size={14} className="mt-0.5 shrink-0 text-[var(--success-text)]" aria-hidden="true" />
            {t("importV2.asr.localOnly")}
          </p>
          <label className="mt-3 flex items-center gap-2">
            <input type="checkbox" checked={remember} onChange={(event) => setRemember(event.target.checked)} disabled={blocked || loading} />
            <span>{t("importV2.asr.remember")}</span>
          </label>
          {!selected?.available && !["not_installed", "signed_release_unavailable"].includes(installState.kind) ? (
            <div className="mt-3" role="status">
              <div className="flex items-center justify-between gap-3 text-[11px]">
                <span>{t(`importV2.capability.state.${installState.kind}`)}</span>
                {installState.totalBytes ? <span className="font-mono text-[var(--text-muted)]">{formatBytes(installState.downloadedBytes, i18n.language, unknown)} / {formatBytes(installState.totalBytes, i18n.language, unknown)}</span> : null}
              </div>
              {installState.totalBytes ? <progress className="mt-1 w-full" max={installState.totalBytes} value={installState.downloadedBytes ?? 0} /> : null}
            </div>
          ) : null}
          {installError ?? taskError ? (
            <ActionableErrorNotice className="mt-3" error={(installError ?? taskError)!} onAction={() => submit()} />
          ) : null}
        </div>

        <footer className="flex items-center justify-end gap-2 border-t border-[var(--border)] px-4 py-3">
          {taskBusy && task ? <button type="button" className="btn btn--sm" onClick={() => void cancelTaskRequest(task.id)}>{t("importV2.capability.cancelDownload")}</button> : <button type="button" className="btn btn--sm" onClick={onCancel}>{t("importV2.asr.cancel")}</button>}
          <button type="button" className="btn btn--sm btn--primary" onClick={() => void submit()} disabled={blocked || loading || !selected || !canChoose(selected)}>
            {blocked ? <LoaderCircle size={13} className="mr-1 inline animate-spin" aria-label={t("importV2.common.loading")} /> : null}
            {t(ctaKey)}
          </button>
        </footer>
      </section>
    </div>
  );
}
