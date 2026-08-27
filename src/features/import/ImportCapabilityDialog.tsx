import { Check, Download, LoaderCircle, Package, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import { ActionableErrorNotice } from "../../components/app/ActionableErrorNotice";
import { useModalDialog } from "../../hooks/useModalDialog";
import {
  normalizeBackendError,
  type NormalizedBackendError,
} from "../../lib/backendError";
import type { ImportCapabilityRequirement } from "../../types/importV2Presentation";
import type { BackendTask } from "../../types/task";
import {
  cancelTaskRequest,
  selectProjectTaskById,
  selectTaskIdsForProject,
  useTaskStore,
} from "../../stores/taskStore";
import { capabilityInstallState } from "./capabilityInstallState";
import { capabilityDisplayName, capabilityPurpose } from "./importCapabilityPresentation";

export interface ImportCapabilityDialogProps {
  open: boolean;
  requirement: ImportCapabilityRequirement | null;
  sessionId?: string | null;
  itemId?: string | null;
  onInstall: (capabilityId: string) => Promise<BackendTask | null | void> | BackendTask | null | void;
  onCancel: () => void;
}

function bytes(value: number | null): string {
  if (value === null) return "—";
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KiB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MiB`;
}

export function ImportCapabilityDialog({ open, requirement, sessionId, itemId, onInstall, onCancel }: ImportCapabilityDialogProps) {
  const { t } = useTranslation();
  const dialogRef = useModalDialog({ open, onClose: onCancel });
  const [acknowledged, setAcknowledged] = useState(false);
  const [starting, setStarting] = useState(false);
  const [startedTaskId, setStartedTaskId] = useState<string | null>(null);
  const activeProjectId = useTaskStore((state) => state.activeProjectId);
  const projectTaskIds = useTaskStore((state) => selectTaskIdsForProject(state, activeProjectId));
  const capabilityTasks = useMemo(() => {
    const state = useTaskStore.getState();
    return projectTaskIds
      .map((taskId) => selectProjectTaskById(state, activeProjectId, taskId))
      .filter((task): task is BackendTask => task !== null)
      .filter((task) => {
        const operation = task.operation;
        return operation?.kind === "capability_install"
          && operation.sessionId === sessionId
          && operation.itemId === itemId
          && operation.capabilityId === requirement?.requirement.capabilityId
          && operation.requirementRevision === requirement?.requirementRevision;
      });
  }, [activeProjectId, itemId, projectTaskIds, requirement, sessionId]);
  const [installError, setInstallError] = useState<NormalizedBackendError | null>(null);
  useEffect(() => {
    setAcknowledged(false);
    setStarting(false);
    setStartedTaskId(null);
    setInstallError(null);
  }, [open, requirement?.requirementRevision]);
  if (!open || !requirement) return null;
  const task = capabilityTasks.find((candidate) => candidate.id === startedTaskId)
    ?? [...capabilityTasks].sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))[0]
    ?? null;
  const state = capabilityInstallState(task, requirement.installable, requirement.available);
  const taskError = task?.error ? normalizeBackendError(task.error, {
    defaultSummaryKey: "backendError.summary.importCapabilityUnavailable",
    defaultActionKind: "retry",
    defaultRecoverable: true,
  }) : null;
  const busy = starting || (task !== null && ["queued", "running", "cancelling"].includes(task.status));
  const canInstall = ["not_installed", "paused", "health_check_failed"].includes(state.kind)
    && requirement.installable && acknowledged && !busy;
  async function install() {
    if (!canInstall) return;
    setStarting(true);
    setInstallError(null);
    try {
      const started = await onInstall(requirement!.requirement.capabilityId);
      if (started) setStartedTaskId(started.id);
    } catch (error) {
      setInstallError(normalizeBackendError(error, {
        defaultSummaryKey: "backendError.summary.importCapabilityUnavailable",
        actionKindOverride: "retry",
        defaultRecoverable: true,
      }));
    } finally {
      setStarting(false);
    }
  }
  async function cancelInstall() {
    if (!task || !busy) return;
    await cancelTaskRequest(task.id);
  }
  return (
    <div ref={dialogRef} tabIndex={-1} className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 px-4" role="dialog" aria-modal="true" aria-labelledby="import-capability-title">
      <section className="flex max-h-[84vh] w-full max-w-[680px] flex-col rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-lg">
        <header className="flex min-h-[52px] items-center gap-3 border-b border-[var(--border)] px-4">
          <Package size={17} className="text-[var(--accent)]" aria-hidden="true" />
          <h2 id="import-capability-title" className="m-0 flex-1 text-[15px] font-semibold">{t("importV2.capability.title")}</h2>
          <button type="button" className="icon-button" aria-label={t("importV2.capability.cancel")} title={t("importV2.capability.cancel")} onClick={onCancel}><X size={16} aria-hidden="true" /></button>
        </header>
        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4 text-[12px]">
          <dl className="grid grid-cols-[150px_1fr] gap-x-4 gap-y-1.5">
            <dt className="text-[var(--text-muted)]">{t("importV2.capability.name")}</dt><dd className="m-0">{capabilityDisplayName(requirement.requirement.capabilityId, t)}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.capability.purpose")}</dt><dd className="m-0">{capabilityPurpose(requirement.route, t)}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.capability.version")}</dt><dd className="m-0">{requirement.requirement.minimumVersion ?? "—"}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.capability.compressed")}</dt><dd className="m-0">{bytes(requirement.compressedBytes)}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.capability.installed")}</dt><dd className="m-0">{bytes(requirement.installedBytes)}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.capability.model")}</dt><dd className="m-0">{bytes(requirement.modelBytes)}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.capability.license")}</dt><dd className="m-0">{requirement.license ?? requirement.requirement.acceptedLicenseExpressions.join(", ")}</dd>
          </dl>
          <details className="import-v2-technical-details mt-3">
            <summary>{t("importV2.preview.technicalDetails")}</summary>
            <dl>
              <dt>{t("importV2.capability.identifier")}</dt><dd>{requirement.requirement.capabilityId}</dd>
              <dt>{t("importV2.inspector.route")}</dt><dd>{requirement.route}</dd>
              <dt>{t("importV2.capability.protocol")}</dt><dd>{requirement.requirement.protocolVersion}</dd>
              <dt>{t("importV2.capability.platform")}</dt><dd>{requirement.requirement.targetTriple}</dd>
            </dl>
          </details>
          {requirement.fallback ? <p className="mt-3 rounded-[var(--radius-md)] border border-[var(--border-subtle)] px-3 py-2 text-[11px] text-[var(--text-muted)]"><strong>{t("importV2.capability.fallback")}:</strong> {requirement.fallback}</p> : null}
          {state.kind === "installed" ? <p className="mt-3 flex items-center gap-1.5 text-[var(--success-text)]" role="status"><Check size={14} aria-hidden="true" />{t("importV2.capability.installedState")}</p> : null}
          {!requirement.installable ? <p className="mt-3 text-[11px] text-[var(--warning-text)]" role="alert">{t("importV2.capability.unavailable")}</p> : null}
          {!["installed", "signed_release_unavailable"].includes(state.kind) ? <label className="mt-3 flex items-start gap-2 rounded-[var(--radius-md)] border border-[var(--border-subtle)] px-3 py-2"><input type="checkbox" aria-label={t("importV2.capability.installAck")} checked={acknowledged} onChange={(event) => setAcknowledged(event.target.checked)} disabled={busy} /><span>{t("importV2.capability.installAck")}</span></label> : null}
          {!["not_installed", "signed_release_unavailable", "installed"].includes(state.kind) ? (
            <div className="mt-3" role="status">
              <div className="flex items-center justify-between gap-3 text-[11px]">
                <span>{t(`importV2.capability.state.${state.kind}`)}</span>
                {state.totalBytes ? <span className="font-mono text-[var(--text-muted)]">{bytes(state.downloadedBytes)} / {bytes(state.totalBytes)}</span> : null}
              </div>
              {state.totalBytes ? <progress className="mt-1 w-full" max={state.totalBytes} value={state.downloadedBytes ?? 0} /> : null}
            </div>
          ) : null}
          {installError ?? taskError ? (
            <ActionableErrorNotice className="mt-3" error={(installError ?? taskError)!} onAction={() => install()} />
          ) : null}
        </div>
        <footer className="flex items-center justify-end gap-2 border-t border-[var(--border)] px-4 py-3">
          {busy ? <button type="button" className="btn btn--sm" onClick={() => void cancelInstall()}>{t("importV2.capability.cancelDownload")}</button> : <button type="button" className="btn btn--sm" onClick={onCancel}>{t("importV2.capability.close")}</button>}
          {state.kind !== "installed" ? <button type="button" className="btn btn--sm btn--primary" onClick={() => void install()} disabled={!canInstall} title={!requirement.installable ? t("importV2.capability.unavailable") : undefined}><Download size={13} className="mr-1 inline" aria-hidden="true" />{busy ? <LoaderCircle size={13} className="animate-spin" aria-label={t("importV2.common.loading")} /> : state.kind === "paused" ? t("importV2.capability.resume") : state.kind === "health_check_failed" ? t("importV2.capability.reinstall") : t("importV2.capability.install")}</button> : null}
        </footer>
      </section>
    </div>
  );
}
