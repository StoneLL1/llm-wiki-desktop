import { Check, Download, LoaderCircle, Package, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import { ActionableErrorNotice } from "../../components/app/ActionableErrorNotice";
import { useModalDialog } from "../../hooks/useModalDialog";
import { normalizeBackendError, type NormalizedBackendError } from "../../lib/backendError";
import { type AppCapabilityDialogIntent, useAppCapabilityStore } from "../../stores/appCapabilityStore";
import { cancelTaskRequest, selectTaskById, useTaskStore } from "../../stores/taskStore";
import type { AppCapabilityView } from "../../types/appCapability";
import type { ImportCapabilityRequirement } from "../../types/importV2Presentation";
import type { BackendTask } from "../../types/task";
import { capabilityDisplayName, capabilityPurpose } from "./importCapabilityPresentation";

interface SharedProps {
  open: boolean;
  onCancel: () => void;
}

export interface ImportLinkedCapabilityDialogProps extends SharedProps {
  origin?: "import";
  requirement: ImportCapabilityRequirement | null;
  sessionId?: string | null;
  itemId?: string | null;
  onInstall: (capabilityId: string) => Promise<BackendTask | null | void> | BackendTask | null | void;
}

export interface ManagementCapabilityDialogProps extends SharedProps {
  origin: "management";
  capability: AppCapabilityView | null;
  intent: AppCapabilityDialogIntent;
}

export type ImportCapabilityDialogProps = ImportLinkedCapabilityDialogProps | ManagementCapabilityDialogProps;

function bytes(value: number | null | undefined): string {
  if (value === null || value === undefined) return "—";
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KiB`;
  if (value < 1024 * 1024 * 1024) return `${(value / (1024 * 1024)).toFixed(1)} MiB`;
  return `${(value / (1024 * 1024 * 1024)).toFixed(1)} GiB`;
}

function isBusy(task: BackendTask | null): boolean {
  return task !== null && ["queued", "running", "cancelling"].includes(task.status);
}

export function ImportCapabilityDialog(props: ImportCapabilityDialogProps) {
  const { t } = useTranslation();
  const dialogRef = useModalDialog({ open: props.open, onClose: props.onCancel });
  const management = props.origin === "management";
  const requirement = management ? null : props.requirement;
  const capabilityId = management
    ? props.capability?.capabilityId ?? null
    : requirement?.requirement.capabilityId ?? null;
  const globalCapability = useAppCapabilityStore((state) =>
    state.capabilities.find((candidate) => candidate.capabilityId === capabilityId) ?? null);
  const capability = management ? props.capability : globalCapability;
  const actionError = useAppCapabilityStore((state) => state.actionErrorCapabilityId === capabilityId ? state.actionError : null);
  const actionErrorOperation = useAppCapabilityStore((state) => state.actionErrorCapabilityId === capabilityId ? state.actionErrorOperation : null);
  const confirmManagementInstall = useAppCapabilityStore((state) => state.confirmInstall);
  const continueInstall = useAppCapabilityStore((state) => state.continueInstall);
  const cancelInstall = useAppCapabilityStore((state) => state.cancelInstall);
  const refreshCapabilities = useAppCapabilityStore((state) => state.refresh);
  const taskFacts = useTaskStore((state) => state.taskById);
  const visibleTasks = useTaskStore((state) => state.tasks);
  const [acknowledged, setAcknowledged] = useState(false);
  const [starting, setStarting] = useState(false);
  const [startedTaskId, setStartedTaskId] = useState<string | null>(null);
  const [installError, setInstallError] = useState<NormalizedBackendError | null>(null);

  const capabilityTasks = useMemo(() => {
    if (!capabilityId) return [];
    const byId = new Map<string, BackendTask>();
    for (const task of [...Object.values(taskFacts), ...visibleTasks]) {
      if (task.operation?.kind === "app_capability_install" && task.operation.capabilityId === capabilityId) {
        byId.set(task.id, task);
      }
    }
    return [...byId.values()].sort((left, right) => right.updatedAt.localeCompare(left.updatedAt));
  }, [capabilityId, taskFacts, visibleTasks]);

  const task = selectTaskById(useTaskStore.getState(), startedTaskId)
    ?? selectTaskById(useTaskStore.getState(), capability?.activeTaskId ?? capability?.operation.taskId)
    ?? capabilityTasks[0]
    ?? null;
  const busy = starting || isBusy(task);
  const paused = task?.status === "interrupted" || capability?.operation.state === "paused";
  const failed = task?.status === "failed" || capability?.operation.state === "failed";
  const installed = capability?.installation.state === "healthy" || (!management && requirement?.available === true);
  const mutationIntent = management ? props.intent !== "details" : true;
  const installable = capability?.installAllowed ?? requirement?.installable ?? false;
  const canConfirm = mutationIntent && installable && acknowledged && !busy && !paused;

  useEffect(() => {
    setAcknowledged(false);
    setStarting(false);
    setStartedTaskId(null);
    setInstallError(null);
  }, [props.open, capabilityId, management ? props.intent : requirement?.requirementRevision]);

  if (!props.open || !capabilityId || (!management && !requirement)) return null;

  const taskError = task?.error ? normalizeBackendError(task.error, {
    defaultSummaryKey: "backendError.summary.importCapabilityUnavailable",
    defaultActionKind: "retry",
    defaultRecoverable: true,
  }) : null;
  const version = capability?.targetVersion ?? requirement?.requirement.minimumVersion ?? null;
  const target = capability?.targetTriple ?? requirement?.requirement.targetTriple ?? "—";
  const license = capability?.licenseExpression ?? requirement?.license ?? requirement?.requirement.acceptedLicenseExpressions.join(", ") ?? "—";
  const packageBytes = capability?.compressedBytes ?? requirement?.compressedBytes;
  const installedBytes = capability?.installedBytes ?? requirement?.installedBytes;
  const modelBytes = capability?.modelBytes ?? requirement?.modelBytes;
  const purpose = capability ? t(capability.purposeKey) : capabilityPurpose(requirement!.route, t);
  const name = capability ? t(capability.nameKey) : capabilityDisplayName(capabilityId, t);
  const waitingCount = management ? capability?.currentProjectWaitingCount ?? 0 : Math.max(1, capability?.currentProjectWaitingCount ?? 0);

  async function install() {
    if (!canConfirm) return;
    setStarting(true);
    setInstallError(null);
    try {
      const started = management
        ? await confirmManagementInstall(capabilityId!)
        : await props.onInstall(capabilityId!);
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

  async function continuePaused() {
    if (!capabilityId || busy) return;
    setStarting(true);
    try {
      const resumed = await continueInstall(capabilityId);
      if (resumed) setStartedTaskId(resumed.id);
    } catch (error) {
      setInstallError(normalizeBackendError(error, { defaultSummaryKey: "backendError.summary.importCapabilityUnavailable", defaultActionKind: "retry", defaultRecoverable: true }));
    } finally {
      setStarting(false);
    }
  }

  async function cancelActive() {
    if (!capabilityId || !task) return;
    try {
      if (management || capability) await cancelInstall(capabilityId);
      else await cancelTaskRequest(task.id);
    } catch (error) {
      setInstallError(normalizeBackendError(error, {
        defaultSummaryKey: "backendError.summary.importCapabilityUnavailable",
        actionKindOverride: null,
        defaultRecoverable: true,
      }));
    }
  }

  const progressCurrent = task?.progress?.current ?? capability?.operation.progressCurrent;
  const progressTotal = task?.progress?.total ?? capability?.operation.progressTotal;
  const progressState = capability?.operation.state ?? (paused ? "paused" : failed ? "failed" : busy ? "downloading" : null);
  const downloading = progressState === "downloading";
  const installResult = task?.result?.reference?.type === "app_capability_install"
    ? task.result.reference
    : null;
  const reviewContinuationCount = installResult
    ? installResult.deferredContinuations + installResult.failedContinuations
    : 0;
  const stableErrorCode = taskError?.code
    ?? capability?.operation.errorCode
    ?? capability?.errorCode
    ?? capability?.distribution.errorCode
    ?? capability?.installBlockedReasonCode
    ?? null;

  async function retryVisibleError() {
    if (["APP_CAPABILITY_TASK_REVISION_STALE", "APP_CAPABILITY_VERSION_STALE", "APP_CAPABILITY_ACKNOWLEDGEMENT_STALE"].includes(actionError?.code ?? "")) {
      await refreshCapabilities(true);
      return;
    }
    if (actionErrorOperation === "cancel") {
      await cancelActive();
      return;
    }
    if (actionErrorOperation === "continue" || paused) {
      await continuePaused();
      return;
    }
    await install();
  }

  return (
    <div ref={dialogRef} tabIndex={-1} className="dialog-overlay" role="dialog" aria-modal="true" aria-labelledby="import-capability-title">
      <section className="import-capability-dialog">
        <header>
          <Package size={17} className="text-[var(--accent)]" aria-hidden="true" />
          <div className="min-w-0 flex-1"><h2 id="import-capability-title">{management ? t("importV2.capabilityManagement.confirmTitle") : t("importV2.capability.title")}</h2><p>{name}</p></div>
          <button type="button" className="icon-button" aria-label={t("importV2.capability.cancel")} title={t("importV2.capability.cancel")} onClick={props.onCancel}><X size={16} aria-hidden="true" /></button>
        </header>
        <div className="import-capability-dialog__body">
          <dl className="import-capability-dialog__facts">
            <div><dt>{t("importV2.capability.purpose")}</dt><dd>{purpose}</dd></div>
            <div><dt>{t("importV2.capability.version")}</dt><dd>{version ?? "—"}</dd></div>
            <div><dt>{t("importV2.capability.platform")}</dt><dd className="font-mono">{target}</dd></div>
            <div><dt>{t("importV2.capabilityManagement.publisherKey")}</dt><dd className="font-mono">{capability?.publisherKeyId ?? "—"}</dd></div>
            <div><dt>{t("importV2.capabilityManagement.sourceDomain")}</dt><dd className="font-mono">{capability?.sourceDomain ?? "—"}</dd></div>
            <div><dt>{t("importV2.capability.compressed")}</dt><dd>{bytes(packageBytes)}</dd></div>
            <div><dt>{t("importV2.capability.model")}</dt><dd>{bytes(modelBytes)}</dd></div>
            <div><dt>{t("importV2.capability.installed")}</dt><dd>{bytes(installedBytes)}</dd></div>
            <div><dt>{t("importV2.capability.license")}</dt><dd>{license}</dd></div>
            <div><dt>{t("importV2.capabilityManagement.continuations")}</dt><dd>{management ? t("importV2.capabilityManagement.continuationManagement", { count: waitingCount }) : t("importV2.capabilityManagement.continuationImport", { count: waitingCount })}</dd></div>
          </dl>

          <section className="import-capability-dialog__permissions" aria-labelledby="import-capability-permissions">
            <h3 id="import-capability-permissions">{t("importV2.capabilityManagement.permissions")}</h3>
            <ul>
              <li>{t(capability?.runtimeNetwork ? "importV2.capabilityManagement.permission.network" : "importV2.capabilityManagement.permission.noNetwork")}</li>
              <li>{t(capability?.runtimeSubprocess ? "importV2.capabilityManagement.permission.subprocess" : "importV2.capabilityManagement.permission.noSubprocess")}</li>
              <li>{t("importV2.capabilityManagement.permission.filesystem", { scopes: capability?.runtimeFilesystem.join(", ") || "—" })}</li>
            </ul>
          </section>

          <p className="import-capability-dialog__safety">{t("importV2.capabilityManagement.activationSafety")}</p>
          {!installable && (mutationIntent || capability?.distribution.state !== "published") ? <p className="import-capability-dialog__warning" role="alert">{t(capability?.distribution.state === "source_catalog_empty" ? "importV2.capability.state.catalog_unavailable" : capability?.distribution.state === "unsupported" ? "backendError.summary.appCapabilityUnsupported" : "importV2.capability.unavailable")}</p> : null}
          {installed && !failed && !busy && !paused ? <p className="import-capability-dialog__success" role="status"><Check size={14} aria-hidden="true" />{t("importV2.capability.installedState")}</p> : null}
          {installResult && installResult.resumedContinuations > 0 ? <p className="import-capability-dialog__success" role="status"><Check size={14} aria-hidden="true" />{t("importV2.capabilityManagement.continuationResumed", { count: installResult.resumedContinuations })}</p> : null}
          {reviewContinuationCount > 0 ? <p className="import-capability-dialog__warning" role="alert">{t("importV2.capabilityManagement.continuationReview", { count: reviewContinuationCount })}</p> : null}

          {progressState && (busy || paused || failed) ? <div className="import-capability-dialog__progress" role="status" aria-live="polite">
            <div><span>{t(`importV2.capabilityManagement.state.${progressState}`)}</span>{downloading && progressTotal ? <span className="font-mono">{bytes(progressCurrent)} / {bytes(progressTotal)}</span> : null}</div>
            {downloading && progressTotal ? <progress max={progressTotal} value={progressCurrent ?? 0} /> : null}
          </div> : null}

          {mutationIntent && !paused ? <label className="import-capability-dialog__ack"><input type="checkbox" checked={acknowledged} onChange={(event) => setAcknowledged(event.target.checked)} disabled={busy || !installable} /><span>{t("importV2.capabilityManagement.acknowledgement", { version: version ?? "—", license })}</span></label> : null}
          {installError ?? actionError ?? taskError ? <ActionableErrorNotice className="mt-3" error={(installError ?? actionError ?? taskError)!} onAction={async (kind) => { if (kind === "retry") await retryVisibleError(); }} /> : null}
          <details className="import-v2-technical-details mt-3"><summary>{t("importV2.preview.technicalDetails")}</summary><dl><dt>{t("importV2.capability.identifier")}</dt><dd>{capabilityId}</dd><dt>{t("importV2.capabilityManagement.routes")}</dt><dd>{capability?.routes.join(", ") ?? requirement?.route ?? "—"}</dd>{stableErrorCode ? <><dt>{t("importV2.capabilityManagement.errorCode")}</dt><dd>{stableErrorCode}</dd></> : null}</dl></details>
        </div>
        <footer>
          {busy ? <button type="button" className="btn btn--sm" onClick={() => void cancelActive()}>{t("importV2.capabilityManagement.action.cancel")}</button> : <button type="button" className="btn btn--sm" onClick={props.onCancel}>{t("importV2.capability.close")}</button>}
          {paused ? <button type="button" className="btn btn--sm btn--primary" onClick={() => void continuePaused()} disabled={starting}>{starting ? <LoaderCircle size={13} className="animate-spin" aria-hidden="true" /> : null}{t("importV2.capabilityManagement.action.continue")}</button> : null}
          {mutationIntent && !paused ? <button type="button" className="btn btn--sm btn--primary" onClick={() => void install()} disabled={!canConfirm}><Download size={13} aria-hidden="true" />{starting ? <LoaderCircle size={13} className="animate-spin" aria-hidden="true" /> : null}{t(management && props.intent === "update" ? "importV2.capabilityManagement.action.update" : management && props.intent === "retry" ? "importV2.capabilityManagement.action.retry" : "importV2.capabilityManagement.action.install")}</button> : null}
        </footer>
      </section>
    </div>
  );
}
