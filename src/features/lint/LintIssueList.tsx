import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { AlertCircle, AlertTriangle, Info } from "lucide-react";

import { SEVERITY_ORDER } from "../../types/lint";
import type { LintIssue, LintSeverity } from "../../types/lint";

interface LintIssueListProps {
  issues: LintIssue[];
  selectedIssueId: string | null;
  onSelect: (issueId: string) => void;
  onApplyFix: (issue: LintIssue) => void;
}

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

const SEVERITY_BADGE: Record<LintSeverity, string> = {
  error: "badge badge--danger",
  warning: "badge badge--warn",
  info: "badge badge--info",
};

function groupLabel(issue: LintIssue, t: (key: string) => string): string {
  return `${t(`lint.severity.${issue.severity}`)} · ${t(`lint.source.${issue.source}`)}`;
}

function subLine(issue: LintIssue): string {
  const parts = [issue.path];
  if (issue.range) parts.push(`L${issue.range.line}`);
  if (issue.target) parts.push(`→ ${issue.target}`);
  return parts.join(" · ");
}

export function LintIssueList({
  issues,
  selectedIssueId,
  onSelect,
  onApplyFix,
}: LintIssueListProps) {
  const { t } = useTranslation();

  const grouped = useMemo(() => {
    const sorted = [...issues].sort((a, b) => {
      const bySeverity = SEVERITY_ORDER[a.severity] - SEVERITY_ORDER[b.severity];
      if (bySeverity !== 0) return bySeverity;
      return a.source.localeCompare(b.source);
    });
    const groups = new Map<string, LintIssue[]>();
    for (const issue of sorted) {
      const key = `${issue.severity}:${issue.source}`;
      const bucket = groups.get(key);
      if (bucket) bucket.push(issue);
      else groups.set(key, [issue]);
    }
    return [...groups.entries()];
  }, [issues]);

  if (issues.length === 0) {
    return (
      <div className="flex h-full items-center justify-center px-6 py-8 text-center text-[12px] text-[var(--text-muted)]">
        {t("lint.list.empty")}
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col overflow-y-auto">
      {grouped.map(([key, group]) => {
        const first = group[0]!;
        return (
          <div key={key}>
            <div className="px-5 pt-3 pb-1 text-[10.5px] uppercase tracking-[0.08em] text-[var(--text-muted)]">
              {groupLabel(first, t)} · {group.length}
            </div>
            {group.map((issue) => {
              const Icon = SEVERITY_ICON[issue.severity];
              const active = issue.id === selectedIssueId;
              const fixable = issue.fixability !== "none";
              return (
                <div
                  key={issue.id}
                  role="button"
                  tabIndex={0}
                  aria-pressed={active}
                  onClick={() => onSelect(issue.id)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === " ") {
                      event.preventDefault();
                      onSelect(issue.id);
                    }
                  }}
                  className={`issue-card ${active ? "is-selected" : ""}`}
                >
                  <span className={`issue-card__icon ${SEVERITY_ICON_COLOR[issue.severity]}`}>
                    <Icon size={16} aria-hidden="true" />
                  </span>
                  <div className="min-w-0">
                    <div className="issue-card__title">
                      {t(`lint.issueType.${issue.issueType}`)}
                    </div>
                    <div className="issue-card__sub">{subLine(issue)}</div>
                    <div className="issue-card__tags">
                      <span className={SEVERITY_BADGE[issue.severity]}>
                        {t(`lint.severity.${issue.severity}`)}
                      </span>
                      <span className="badge">{t(`lint.issueType.${issue.issueType}`)}</span>
                      <span className="badge">{t(`lint.source.${issue.source}`)}</span>
                      {issue.fixability === "safe" ? (
                        <span className="badge badge--outline">{t("lint.tag.autoFixable")}</span>
                      ) : null}
                      {issue.fixability === "high_risk" ? (
                        <span className="badge badge--warn">{t("lint.tag.highRisk")}</span>
                      ) : null}
                    </div>
                  </div>
                  <div className="issue-card__actions">
                    {fixable ? (
                      <button
                        type="button"
                        onClick={(event) => {
                          event.stopPropagation();
                          onSelect(issue.id);
                          onApplyFix(issue);
                        }}
                        className="h-[26px] rounded-[var(--radius-md)] bg-[var(--foreground)] px-2.5 text-[12px] font-medium text-[var(--text-inverse)] hover:bg-[var(--primary-hover)]"
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
  );
}
