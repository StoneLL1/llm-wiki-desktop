import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import {
  normalizeBackendError,
  redactBackendErrorDetails,
  type BackendErrorActionKind,
  type NormalizedBackendError,
} from "../../lib/backendError";

export interface ActionableErrorNoticeProps {
  error: unknown | NormalizedBackendError;
  onAction?: (kind: Exclude<BackendErrorActionKind, null>) => Promise<void> | void;
  role?: "alert" | "status";
  className?: string;
}

const ACTION_LABELS: Record<Exclude<BackendErrorActionKind, null>, string> = {
  retry: "backendError.action.retry",
  reauthorize: "backendError.action.reauthorize",
  repair: "backendError.action.repair",
  open_settings: "backendError.action.openSettings",
  restart: "backendError.action.restart",
  copy_details: "backendError.action.copyDetails",
};

export function ActionableErrorNotice({
  error,
  onAction,
  role = "alert",
  className = "",
}: ActionableErrorNoticeProps) {
  const { t } = useTranslation();
  const normalized = useMemo(() => normalizeBackendError(error), [error]);
  const [busy, setBusy] = useState(false);
  const [copyState, setCopyState] = useState<"idle" | "copied" | "failed">("idle");
  const [actionFailure, setActionFailure] = useState<NormalizedBackendError | null>(null);
  const actionGeneration = useRef(0);

  useEffect(() => {
    actionGeneration.current += 1;
    setBusy(false);
    setCopyState("idle");
    setActionFailure(null);
  }, [error]);

  const copyDetails = async () => {
    if (!normalized.technicalDetails || busy) return;
    const generation = actionGeneration.current + 1;
    actionGeneration.current = generation;
    setBusy(true);
    try {
      await navigator.clipboard.writeText(redactBackendErrorDetails(normalized.technicalDetails));
      if (actionGeneration.current === generation) setCopyState("copied");
    } catch {
      if (actionGeneration.current === generation) setCopyState("failed");
    } finally {
      if (actionGeneration.current === generation) setBusy(false);
    }
  };

  const runAction = async (kind: Exclude<BackendErrorActionKind, null>) => {
    if (busy) return;
    if (kind === "copy_details") {
      await copyDetails();
      return;
    }
    if (!onAction) return;
    const generation = actionGeneration.current + 1;
    actionGeneration.current = generation;
    setBusy(true);
    setActionFailure(null);
    try {
      await onAction(kind);
    } catch (reason) {
      if (actionGeneration.current === generation) {
        setActionFailure(normalizeBackendError(reason, {
          defaultSummaryKey: "backendError.summary.actionFailed",
          defaultActionKind: kind,
          defaultRecoverable: true,
        }));
      }
    } finally {
      if (actionGeneration.current === generation) setBusy(false);
    }
  };

  const primaryAction = normalized.actionKind;
  const showPrimaryAction = primaryAction === "copy_details"
    ? Boolean(normalized.technicalDetails)
    : Boolean(primaryAction && onAction);

  return (
    <div
      className={`actionable-error-notice rounded-[var(--radius-md)] border border-[var(--danger)]/20 bg-[var(--danger)]/10 px-3 py-2 text-[12px] text-[var(--text-primary)] ${className}`.trim()}
      role={role}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="m-0 font-medium">{t(normalized.summaryKey, normalized.summaryParams)}</p>
          {actionFailure ? (
            <p className="m-0 mt-1 text-[11px] text-[var(--danger)]">
              {t(actionFailure.summaryKey, actionFailure.summaryParams)}
            </p>
          ) : null}
        </div>
        {showPrimaryAction && primaryAction ? (
          <button
            className="btn btn--secondary btn--sm shrink-0"
            disabled={busy}
            onClick={() => void runAction(primaryAction)}
            type="button"
          >
            {busy ? t("backendError.action.working") : t(ACTION_LABELS[primaryAction])}
          </button>
        ) : null}
      </div>
      {normalized.technicalDetails ? (
        <details className="mt-2 text-[11px]">
          <summary className="cursor-pointer text-[var(--text-muted)]">
            {t("backendError.technicalDetails")}
          </summary>
          <pre className="app-pane-scrollbar mt-2 max-h-40 overflow-auto whitespace-pre-wrap break-all rounded-[var(--radius-sm)] bg-[var(--surface-muted)] p-2 font-mono text-[11px] text-[var(--text-secondary)]">
            {normalized.technicalDetails}
          </pre>
          {primaryAction !== "copy_details" ? (
            <button
              className="btn btn--ghost btn--sm mt-2"
              disabled={busy}
              onClick={() => void copyDetails()}
              type="button"
            >
              {copyState === "copied"
                ? t("backendError.copy.copied")
                : copyState === "failed"
                  ? t("backendError.copy.failed")
                  : t("backendError.action.copyDetails")}
            </button>
          ) : null}
        </details>
      ) : null}
    </div>
  );
}
