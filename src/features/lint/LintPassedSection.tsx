import { useTranslation } from "react-i18next";
import { Check } from "lucide-react";

import type { LintIssueType } from "../../types/lint";

/** Maps a passed deterministic rule to its i18n label key. */
const PASSED_RULE_LABEL: Record<LintIssueType, string> = {
  missing_frontmatter: "lint.passed.frontmatter",
  index_drift: "lint.passed.index",
  duplicate_filename: "lint.passed.duplicateFilename",
  missing_resource: "lint.passed.missingResource",
  path_case: "lint.passed.pathCase",
  missing_source_section: "",
  invalid_page_type: "",
  // Rules without a "passed" badge (informational / Agent-side) are not listed.
  dead_link: "",
  orphan_page: "",
  empty_page: "",
  duplicate_topic: "",
  weak_cross_reference: "",
  missing_source: "",
  schema_mismatch: "",
  outdated_content: "",
  contradiction: "",
};

interface LintPassedSectionProps {
  /** Deterministic local rules that did not fire this scan. */
  passedRules: LintIssueType[];
  notApplicableRules?: LintIssueType[];
  coverageUnknown?: boolean;
}

export function LintPassedSection({ passedRules, notApplicableRules = [], coverageUnknown = false }: LintPassedSectionProps) {
  const { t } = useTranslation();
  const labeled = passedRules
    .map((rule) => PASSED_RULE_LABEL[rule])
    .filter(Boolean);
  const notApplicable = notApplicableRules.map((rule) => PASSED_RULE_LABEL[rule]).filter(Boolean);

  if (labeled.length === 0 && notApplicable.length === 0 && !coverageUnknown) return null;

  return (
    <details className="lint-passed lint-disclosure">
      <summary className="lint-passed__label">{labeled.length > 0 ? `${t("lint.passed.title")} · ${labeled.length}` : t("lint.passed.coverage")}</summary>
      <div className="lint-passed__badges">
        {coverageUnknown ? <span className="badge">{t("lint.passed.unknown")}</span> : null}
        {labeled.map((key) => (
          <span key={key} className="badge badge--success">
            <Check size={11} aria-hidden="true" />
            {t(key)}
          </span>
        ))}
        {notApplicable.map((key) => <span key={`na-${key}`} className="badge">{t("lint.passed.notApplicable", { rule: t(key) })}</span>)}
      </div>
    </details>
  );
}
