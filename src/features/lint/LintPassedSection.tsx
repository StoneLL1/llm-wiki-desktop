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
}

export function LintPassedSection({ passedRules }: LintPassedSectionProps) {
  const { t } = useTranslation();
  const labeled = passedRules
    .map((rule) => PASSED_RULE_LABEL[rule])
    .filter(Boolean);

  if (labeled.length === 0) return null;

  return (
    <div className="lint-passed">
      <div className="lint-passed__label">{t("lint.passed.title")}</div>
      <div className="lint-passed__badges">
        {labeled.map((key) => (
          <span key={key} className="badge badge--success">
            <Check size={11} aria-hidden="true" />
            {t(key)}
          </span>
        ))}
      </div>
    </div>
  );
}
