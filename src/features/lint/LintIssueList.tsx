import { useMemo, useRef, useState, type RefObject } from "react";
import { useTranslation } from "react-i18next";
import { AlertCircle, AlertTriangle, Info, SearchCheck } from "lucide-react";

import { SEVERITY_ORDER } from "../../types/lint";
import type { LintIssue, LintSeverity } from "../../types/lint";

interface LintIssueListProps {
  scrollRef?: RefObject<HTMLDivElement | null>;
  issues: LintIssue[];
  selectedIssueId: string | null;
  actionsDisabled?: boolean;
  emptyMessage?: string;
  selectionDisabled?: boolean;
  onSelect: (issueId: string) => void;
  onApplyFix: (issue: LintIssue) => void;
  repairSelection?: ReadonlySet<string>;
  repairEligibleIds?: ReadonlySet<string>;
  onToggleRepairSelection?: (issueId: string, selected: boolean) => void;
}

const PAGE_SIZE = 100;

const SEVERITY_ICON: Record<LintSeverity, typeof Info> = {
  error: AlertCircle,
  warning: AlertTriangle,
  info: Info,
};

const SEVERITY_ICON_COLOR: Record<LintSeverity, string> = {
  error: "text-[var(--danger)]",
  warning: "text-[var(--warning)]",
  info: "text-[var(--info)]",
};

function groupLabel(issue: LintIssue, t: (key: string) => string): string {
  const origins = issue.origins ?? [issue.source];
  return `${t(`lint.severity.${issue.severity}`)} · ${origins
    .map((source) => t(`lint.source.${source}`))
    .join(" + ")}`;
}

function subLine(issue: LintIssue): string {
  const parts = [issue.path];
  if (issue.range) parts.push(`L${issue.range.line}`);
  if (issue.target) parts.push(`→ ${issue.target}`);
  return parts.join(" · ");
}

export function LintIssueList({
  scrollRef,
  issues,
  selectedIssueId,
  actionsDisabled = false,
  selectionDisabled = false,
  emptyMessage,
  onSelect,
  onApplyFix,
  repairSelection = new Set<string>(),
  repairEligibleIds = new Set<string>(),
  onToggleRepairSelection,
}: LintIssueListProps) {
  const { t } = useTranslation();

  const localScrollRef = useRef<HTMLDivElement>(null);
  const listScrollRef = scrollRef ?? localScrollRef;
  const [pagination, setPagination] = useState({ issues, page: 0 });
  // A new report or filter starts at the beginning; selection updates retain
  // the current page. The list owns no copy of selection or repair state.
  const page = pagination.issues === issues ? pagination.page : 0;
  const start = page * PAGE_SIZE;
  const end = Math.min(start + PAGE_SIZE, issues.length);

  const grouped = useMemo(() => {
    const groups = new Map<string, LintIssue[]>();
    for (const issue of issues) {
      const key = `${issue.severity}:${(issue.origins ?? [issue.source]).join(":")}`;
      const bucket = groups.get(key);
      if (bucket) bucket.push(issue);
      else groups.set(key, [issue]);
    }
    // Sort the few severity/origin groups, not every finding. Insertion order
    // within each group is the same stable order as the previous full sort.
    return [...groups.entries()].sort(([, a], [, b]) => {
      const first = a[0]!;
      const second = b[0]!;
      return SEVERITY_ORDER[first.severity] - SEVERITY_ORDER[second.severity]
        || (first.origins ?? [first.source]).join(":")
          .localeCompare((second.origins ?? [second.source]).join(":"));
    });
  }, [issues]);

  const visibleGroups = useMemo(() => {
    let offset = 0;
    return grouped.flatMap(([key, group]) => {
      const from = Math.max(0, start - offset);
      const to = Math.min(group.length, end - offset);
      offset += group.length;
      return to > from ? [{ key, total: group.length, issues: group.slice(from, to) }] : [];
    });
  }, [grouped, start, end]);

  function changePage(nextPage: number) {
    setPagination({ issues, page: nextPage });
    if (listScrollRef.current) listScrollRef.current.scrollTop = 0;
  }

  if (issues.length === 0) {
    return (
      <div className="lint-empty">
        <SearchCheck size={22} aria-hidden="true" />
        <p>{emptyMessage ?? t("lint.list.empty")}</p>
      </div>
    );
  }

  return (
    <>
      <div ref={listScrollRef} className="flex min-h-0 flex-1 flex-col overflow-y-auto" data-testid="lint-issue-list-scroll">
        {visibleGroups.map(({ key, total, issues: group }) => {
          const first = group[0]!;
          return (
            <div key={key}>
              <div className="px-5 pt-3 pb-1 text-[10.5px] uppercase tracking-[0.08em] text-[var(--text-muted)]">
                {groupLabel(first, t)} · {total}
              </div>
              {group.map((issue) => {
                const Icon = SEVERITY_ICON[issue.severity];
                const active = issue.id === selectedIssueId;
                // A persisted finding without its scan baseline is stale. Keep
                // it selectable for details/rescan, but never expose a Fix
                // action that the backend must reject with a hash error.
                const fixable = issue.fixability !== "none" && Boolean(issue.scanHash);
                return (
                  <div
                    key={issue.id}
                    className={`issue-card-shell relative ${active ? "is-selected" : ""}`}
                  >
                    {repairEligibleIds.has(issue.id) ? (
                      <span
                        className="absolute left-2 top-3 z-[1] flex items-start"
                        onClick={(event) => event.stopPropagation()}
                      >
                        <input
                          aria-label={t("lint.repair.selectFinding", { path: issue.path })}
                          checked={repairSelection.has(issue.id)}
                          disabled={actionsDisabled}
                          onChange={(event) => onToggleRepairSelection?.(issue.id, event.target.checked)}
                          type="checkbox"
                        />
                      </span>
                    ) : null}
                    <button
                      type="button"
                      aria-pressed={active}
                      disabled={selectionDisabled}
                      data-lint-issue-id={issue.id}
                      onClick={() => onSelect(issue.id)}
                      className={`issue-card ${repairEligibleIds.has(issue.id) ? "pl-10" : ""}`}
                    >
                      <span className={`issue-card__icon ${SEVERITY_ICON_COLOR[issue.severity]}`}>
                        <Icon size={16} aria-hidden="true" />
                      </span>
                      <div className="min-w-0">
                        <div className="issue-card__title">
                          {t(`lint.issueType.${issue.issueType}`)}
                        </div>
                        <div className="issue-card__sub" title={subLine(issue)}>{subLine(issue)}</div>

                      </div>
                    </button>
                    <div className="issue-card__actions">
                      {fixable ? (
                        <button
                          type="button"
                          onClick={(event) => {
                            event.stopPropagation();
                            if (actionsDisabled) return;
                            onSelect(issue.id);
                            if (issue.fixability === "safe") onApplyFix(issue);
                          }}
                          disabled={actionsDisabled}
                          className="btn btn--secondary btn--sm"
                        >
                          {issue.fixability === "high_risk"
                            ? t("lint.card.details")
                            : t("lint.card.fix")}
                        </button>
                      ) : null}
                    </div>
                  </div>
                );
              })}
            </div>
          );
        })}
      </div>
      {issues.length > PAGE_SIZE ? (
        <div className="flex shrink-0 items-center justify-between gap-3 border-t border-[var(--border)] px-5 py-2 text-[12px]">
          <span className="text-[var(--text-muted)]" role="status">
            {start + 1}–{end} / {issues.length}
          </span>
          <div className="flex items-center gap-2">
            <button
              type="button"
              disabled={page === 0}
              onClick={() => changePage(page - 1)}
              className="h-[26px] rounded-[var(--radius-sm)] border border-[var(--border)] px-2 hover:bg-[var(--surface-muted)] disabled:opacity-40"
            >
              {t("workflows.update.previous")}
            </button>
            <button
              type="button"
              disabled={end === issues.length}
              onClick={() => changePage(page + 1)}
              className="h-[26px] rounded-[var(--radius-sm)] border border-[var(--border)] px-2 hover:bg-[var(--surface-muted)] disabled:opacity-40"
            >
              {t("workflows.update.next")}
            </button>
          </div>
        </div>
      ) : null}
    </>
  );
}
