import { LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";

/**
 * Compact, shell-aligned Suspense fallback for lazy feature views. Keeps the
 * pane's flex region occupied without a layout shift or decorative chrome.
 */
export function ViewFallback() {
  const { t } = useTranslation();
  return (
    <div
      className="flex h-full min-h-[120px] items-center justify-center gap-2 text-[12px] text-[var(--text-muted)]"
      role="status"
      aria-live="polite"
    >
      <LoaderCircle size={16} className="animate-spin" aria-hidden="true" />
      <span>{t("shell.view.loading")}</span>
    </div>
  );
}
