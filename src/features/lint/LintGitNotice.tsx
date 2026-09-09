import { useTranslation } from "react-i18next";

import { LazyActionableErrorNotice } from "../../components/app/LazyActionableErrorNotice";
import { normalizeBackendError } from "../../lib/backendError";

export function LintGitNotice({ error, checking, onConfigure, onRefresh }: {
  error: unknown;
  checking: boolean;
  onConfigure: () => void;
  onRefresh: () => void;
}) {
  const { t } = useTranslation();
  const normalized = normalizeBackendError(error, {
    defaultSummaryKey: "lint.git.unavailable",
    actionKindOverride: null,
  });
  const missing = normalized.code === "VERSION_NOT_ENABLED" || normalized.code === "GIT_REPOSITORY_MISSING" || normalized.code === "GIT_HEAD_MISSING";
  const summaryKey = missing ? "lint.git.required"
    : normalized.code === "GIT_CHECKPOINT_PATH_IGNORED" ? "lint.git.ignored"
      : "lint.git.unavailable";
  return (
    <div className="border-b border-[var(--border)] px-4 py-3">
      {missing ? <p className="m-0 text-[12px] text-[var(--text-secondary)]">{t("versions.enableDescription")}</p> : <LazyActionableErrorNotice error={{ ...normalized, summaryKey }} />}
      <div className="mt-2 flex flex-wrap gap-2">
        {missing ? (
          <button type="button" className="btn btn--secondary btn--sm" disabled={checking} onClick={onConfigure}>
            {t("versions.enableContinue")}
          </button>
        ) : null}
        <button type="button" className="btn btn--secondary btn--sm" disabled={checking} onClick={onRefresh}>
          {t(checking ? "lint.git.checking" : "lint.git.refresh")}
        </button>
      </div>
    </div>
  );
}
