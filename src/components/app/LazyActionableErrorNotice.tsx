import { lazy, Suspense } from "react";
import { useTranslation } from "react-i18next";

import type { ActionableErrorNoticeProps } from "./ActionableErrorNotice";
import { ViewErrorBoundary } from "./ViewErrorBoundary";

const ActionableErrorNotice = lazy(async () => {
  const module = await import("./ActionableErrorNotice");
  return { default: module.ActionableErrorNotice };
});

export function LazyActionableErrorNotice(props: ActionableErrorNoticeProps) {
  return (
    <ViewErrorBoundary errorRole={props.role}>
      <Suspense fallback={<ErrorNoticeLoading />}>
        <ActionableErrorNotice {...props} />
      </Suspense>
    </ViewErrorBoundary>
  );
}

function ErrorNoticeLoading() {
  const { t } = useTranslation();
  return (
    <div className="text-[12px] text-[var(--text-muted)]" role="status">
      {t("shell.view.loading")}
    </div>
  );
}
