import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { AlertCircle, AlertTriangle, Info } from "lucide-react";

import { SEVERITY_ORDER } from "../../types/lint";
import type { LintIssue, LintSeverity } from "../../types/lint";

interface LintIssueListProps {
  issues: LintIssue[];
  selectedIssueId: string | null;
  onSelect: (issueId: string) => void;
}

const SEVERITY_ICON: Record<LintSeverity, typeof Info> = {
  error: AlertCircle,
  warning: AlertTriangle,
  info: Info,
};

const SEVERITY_COLOR: Record<LintSeverity, string> = {
  error: "text-[var(--error)]",
  warning: "text-[var(--warning)]",
  info: "text-[var(--text-muted)]",
};

function groupLabel(issue: LintIssue, t: (key: string) => string): string {
  return `${t(`lint.severity.${issue.severity}`)} · ${t(`lint.source.${issue.source}`)}`;
}

export function LintIssueList({ issues, selectedIssueId, onSelect }: LintIssueListProps) {
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
            <div className="px-4 pt-3 pb-1 text-[10.5px] uppercase tracking-[0.08em] text-[var(--text-muted)]">
              {groupLabel(first, t)} · {group.length}
            </div>
            {group.map((issue) => {
              const Icon = SEVERITY_ICON[issue.severity];
              const active = issue.id === selectedIssueId;
              return (
                <button
                  key={issue.id}
                  type="button"
                  onClick={() => onSelect(issue.id)}
                  className={`flex w-full items-start gap-2 border-l-2 px-4 py-2 text-left transition-colors ${
                    active
                      ? "border-[var(--accent)] bg-[var(--surface-muted)]"
                      : "border-transparent hover:bg-[var(--surface-muted)]"
                  }`}
                >
                  <Icon size={14} className={`mt-0.5 shrink-0 ${SEVERITY_COLOR[issue.severity]}`} />
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-1.5">
                      <span className="truncate text-[12px] font-medium text-[var(--text-primary)]">
                        {t(`lint.issueType.${issue.issueType}`)}
                      </span>
                      <span className="shrink-0 text-[10.5px] text-[var(--text-muted)]">
                        {t(`lint.source.${issue.source}`)}
                      </span>
                    </div>
                    <div className="truncate font-mono text-[11px] text-[var(--text-muted)]">
                      {issue.path}
                    </div>
                  </div>
                </button>
              );
            })}
          </div>
        );
      })}
    </div>
  );
}
