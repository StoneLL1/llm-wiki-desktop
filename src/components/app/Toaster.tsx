import { AlertTriangle, Info, X } from "lucide-react";
import { useTranslation } from "react-i18next";

import { useToastStore, type ToastTone } from "../../stores/toastStore";

const toneStyles: Record<ToastTone, { border: string; icon: typeof Info }> = {
  info: { border: "border-[var(--accent-border)]", icon: Info },
  warning: { border: "border-[var(--warning)]", icon: AlertTriangle },
  error: { border: "border-[var(--danger)]", icon: AlertTriangle },
};

export function Toaster() {
  const { t } = useTranslation();
  const toasts = useToastStore((state) => state.toasts);
  const dismiss = useToastStore((state) => state.dismissToast);

  if (toasts.length === 0) return null;

  return (
    <div
      role="status"
      aria-live="polite"
      className="pointer-events-none fixed bottom-[calc(var(--statusbar-h)+12px)] right-3 z-50 flex flex-col gap-2"
    >
      {toasts.map((toast) => {
        const style = toneStyles[toast.tone];
        const Icon = style.icon;
        return (
          <div
            key={toast.id}
            className={`pointer-events-auto flex max-w-[360px] items-start gap-2 rounded-[var(--radius-md)] border ${style.border} bg-[var(--surface-raised)] px-3 py-2 text-[12px] text-[var(--text-primary)] shadow-md`}
          >
            <Icon aria-hidden="true" size={14} className="mt-[2px] shrink-0 text-[var(--text-muted)]" />
            <span className="min-w-0 flex-1 leading-5">{toast.message}</span>
            <button
              type="button"
              onClick={() => dismiss(toast.id)}
              aria-label={t("toast.dismiss")}
              className="shrink-0 text-[var(--text-muted)] hover:text-[var(--text-primary)]"
            >
              <X size={12} />
            </button>
          </div>
        );
      })}
    </div>
  );
}
