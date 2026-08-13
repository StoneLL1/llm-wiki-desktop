import { CheckCircle2, ShieldAlert, Workflow } from "lucide-react";
import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";

import type {
  AgentLintRepairPreparation,
  HealthCheckReport,
  LintIssue,
} from "../../types/lint";

const REPAIR_ERROR_KEYS: Record<string, string> = {
  LINT_REPAIR_SELECTION_INVALID: "lint.repair.errors.selectionInvalid",
  LINT_REPAIR_SELECTION_LIMIT: "lint.repair.errors.selectionLimit",
  LINT_REPAIR_HEALTH_REPORT_REQUIRED: "lint.repair.errors.healthReportRequired",
  LINT_REPAIR_WIKI_ROOT_REQUIRED: "lint.repair.errors.wikiRootRequired",
  LINT_REPAIR_GIT_CLEAN_REQUIRED: "lint.repair.errors.gitCleanRequired",
  LINT_REPAIR_GIT_HEAD_REQUIRED: "lint.repair.errors.gitHeadRequired",
  LINT_REPAIR_PREPARATION_STALE: "lint.repair.errors.preparationStale",
  LINT_REPAIR_CONFIRMATION_MISMATCH: "lint.repair.errors.confirmationMismatch",
  LINT_REPAIR_IDENTITY_CHANGED: "lint.repair.errors.confirmationMismatch",
  CONFIRMATION_EXPIRED: "lint.repair.errors.confirmationExpired",
  CONFIRMATION_NOT_FOUND: "lint.repair.errors.confirmationExpired",
};

export interface AgentLintRepairPanelProps {
  report: HealthCheckReport | null;
  agentRouteConfigured: boolean;
  eligibleFindings: LintIssue[];
  selectedFindingIds: string[];
  preparation: AgentLintRepairPreparation | null;
  pending: boolean;
  errorCode: string | null;
  onPrepare: () => void;
  onConfirm: () => void;
  onCancel: () => void;
}

function errorKey(code: string | null): string | null {
  if (!code) return null;
  return REPAIR_ERROR_KEYS[code] ?? "lint.repair.errors.generic";
}

export function AgentLintRepairPanel({
  report,
  agentRouteConfigured,
  eligibleFindings,
  selectedFindingIds,
  preparation,
  pending,
  errorCode,
  onPrepare,
  onConfirm,
  onCancel,
}: AgentLintRepairPanelProps) {
  const { t } = useTranslation();
  const prepareButtonRef = useRef<HTMLButtonElement>(null);
  const confirmButtonRef = useRef<HTMLButtonElement>(null);
  const hadPreparation = useRef(false);
  const hadPending = useRef(false);
  useEffect(() => {
    if (preparation?.pendingAction) confirmButtonRef.current?.focus();
    else if (!pending && (hadPreparation.current || hadPending.current)) prepareButtonRef.current?.focus();
    hadPreparation.current = Boolean(preparation);
    hadPending.current = pending;
  }, [pending, preparation]);
  if (!report) return null;

  const hasAgentFindings = report.issues.some((issue) => issue.source === "agent");
  if (report.route.kind !== "agent" && !hasAgentFindings) return null;

  const unavailable = report.route.kind !== "agent"
    ? "lint.repair.unavailable.agentRoute"
    : !agentRouteConfigured
      ? "lint.repair.unavailable.agentNotConfigured"
      : !report.persistent
        ? "lint.repair.unavailable.notPersistent"
        : eligibleFindings.length === 0
          ? "lint.repair.unavailable.noEligibleFindings"
          : null;
  const pendingAction = preparation?.pendingAction;
  const selectedCount = selectedFindingIds.length;
  const selectionOverLimit = selectedCount > 100;

  return (
    <section
      aria-labelledby="lint-agent-repair-title"
      className="border-b border-[var(--border)] bg-[var(--surface-muted)] px-4 py-3"
    >
      <div className="flex items-start gap-2">
        <Workflow aria-hidden="true" className="mt-0.5 shrink-0 text-[var(--accent)]" size={16} />
        <div className="min-w-0 flex-1">
          <h2 id="lint-agent-repair-title" className="text-[13px] font-semibold text-[var(--text-primary)]">
            {t("lint.repair.title")}
          </h2>
          <p className="mt-1 text-[11.5px] leading-5 text-[var(--text-secondary)]">
            {t("lint.repair.description")}
          </p>
        </div>
      </div>

      {unavailable ? (
        <div className="mt-2 flex items-start gap-2 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-raised)] px-2.5 py-2 text-[11.5px] leading-5 text-[var(--text-secondary)]" role="status">
          <ShieldAlert aria-hidden="true" className="mt-0.5 shrink-0 text-[var(--warning)]" size={14} />
          <span>{t(unavailable)}</span>
        </div>
      ) : preparation && pendingAction ? (
        <div className="mt-2 rounded-[var(--radius-sm)] border border-[var(--accent-border)] bg-[var(--surface-raised)] px-2.5 py-2.5">
          <div className="flex items-center gap-2 text-[12px] font-medium text-[var(--text-primary)]">
            <ShieldAlert aria-hidden="true" className="text-[var(--warning)]" size={14} />
            {t("lint.repair.confirmTitle")}
          </div>
          <dl className="mt-2 grid grid-cols-2 gap-x-3 gap-y-1 text-[11px]">
            <div><dt className="text-[var(--text-muted)]">{t("lint.repair.selected")}</dt><dd>{selectedCount}</dd></div>
            <div><dt className="text-[var(--text-muted)]">{t("lint.repair.rounds")}</dt><dd>3</dd></div>
            <div><dt className="text-[var(--text-muted)]">{t("lint.repair.skill")}</dt><dd className="font-mono">{preparation.skill.version}</dd></div>
            <div><dt className="text-[var(--text-muted)]">{t("lint.repair.checkpoint")}</dt><dd>{t("lint.repair.required")}</dd></div>
          </dl>
          <p className="mt-2 text-[11px] leading-5 text-[var(--text-secondary)]">
            {t("lint.repair.confirmDescription")}
          </p>
          {preparation.authorizedPaths.length > 0 ? (
            <div className="mt-2">
              <div className="text-[10.5px] uppercase tracking-[0.08em] text-[var(--text-muted)]">{t("lint.repair.paths")}</div>
              <ul className="mt-1 max-h-20 overflow-y-auto font-mono text-[11px] text-[var(--text-secondary)]">
                {preparation.authorizedPaths.map((path) => <li key={path} className="truncate" title={path}>{path}</li>)}
              </ul>
            </div>
          ) : null}
          <div className="mt-2 flex gap-2">
            <button
              ref={confirmButtonRef}
              type="button"
              className="h-[28px] rounded-[var(--radius-md)] bg-[var(--foreground)] px-3 text-[12px] font-medium text-[var(--text-inverse)] hover:bg-[var(--primary-hover)] disabled:opacity-40"
              disabled={pending}
              onClick={onConfirm}
            >
              {pending ? t("lint.repair.confirming") : t("lint.repair.confirm")}
            </button>
            <button
              type="button"
              className="h-[28px] rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] hover:bg-[var(--surface-muted)] disabled:opacity-40"
              disabled={pending}
              onClick={onCancel}
            >
              {t("lint.repair.cancel")}
            </button>
          </div>
        </div>
      ) : (
        <>
          <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-[var(--text-secondary)]">
            <span>{t("lint.repair.eligible", { count: eligibleFindings.length })}</span>
            <span>{t("lint.repair.selectedCount", { count: selectedCount })}</span>
            <span>{t("lint.repair.maxRounds")}</span>
          </div>
          <p className="mt-1 text-[11px] leading-5 text-[var(--text-muted)]">
            {t("lint.repair.semanticReview")}
          </p>
          <div className="mt-2 flex gap-2">
            <button
              ref={prepareButtonRef}
              type="button"
              className="h-[28px] rounded-[var(--radius-md)] bg-[var(--foreground)] px-3 text-[12px] font-medium text-[var(--text-inverse)] hover:bg-[var(--primary-hover)] disabled:opacity-40"
              disabled={pending || selectedCount === 0 || selectionOverLimit}
              onClick={onPrepare}
            >
              {pending ? t("lint.repair.preparing") : t("lint.repair.prepare", { count: selectedCount })}
            </button>
            {pending ? (
              <button
                type="button"
                className="h-[28px] rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] hover:bg-[var(--surface-muted)]"
                onClick={onCancel}
              >
                {t("lint.repair.cancel")}
              </button>
            ) : null}
          </div>
        </>
      )}

      {errorCode ? (
        <p className="mt-2 text-[11.5px] leading-5 text-[var(--danger)]" role="alert">
          {t(errorKey(errorCode) ?? "lint.repair.errors.generic")}
        </p>
      ) : null}

      {preparation && !pendingAction ? (
        <p className="mt-2 flex items-center gap-1 text-[11.5px] text-[var(--text-secondary)]" role="status">
          <CheckCircle2 aria-hidden="true" size={13} />
          {t("lint.repair.prepared")}
        </p>
      ) : null}
    </section>
  );
}
