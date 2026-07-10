import { lazy, Suspense, useRef } from "react";
import { useTranslation } from "react-i18next";
import { X } from "lucide-react";
import { useModalDialog } from "../../hooks/useModalDialog";
import { ViewErrorBoundary } from "../../components/app/ViewErrorBoundary";
import type { AgentInfo } from "../../types/agent";
import type {
  LlmProviderConfig,
  LlmProviderKind,
  ProviderStatus,
  ProviderTestResult,
} from "../../types/llm";
import type { ProjectSummary } from "../../types/project";

// Settings is a secondary control surface, not a workspace destination, so it
// loads on demand: the chunk only fetches when the dialog first opens.
const SettingsView = lazy(() =>
  import("./SettingsView").then((m) => ({ default: m.SettingsView })),
);

interface SettingsDialogProps {
  open: boolean;
  onClose: () => void;
  project: ProjectSummary;
  providers: ProviderStatus[];
  agents: AgentInfo[];
  onRefreshCapabilities: () => Promise<void> | void;
  onSaveProvider: (config: LlmProviderConfig) => Promise<unknown> | unknown;
  onSaveSecret: (provider: LlmProviderKind, secret: string) => Promise<unknown> | unknown;
  onDeleteSecret: (provider: LlmProviderKind) => Promise<unknown> | unknown;
  onTestProvider: (config: LlmProviderConfig) => Promise<ProviderTestResult>;
}

export function SettingsDialog({ open, onClose, ...settingsProps }: SettingsDialogProps) {
  const { t } = useTranslation();
  const closeBtnRef = useRef<HTMLButtonElement>(null);
  const dialogRef = useModalDialog<HTMLDivElement>({ open, onClose, initialFocusRef: closeBtnRef });

  if (!open) return null;

  return (
    <div
      ref={dialogRef}
      className="dialog-overlay settings-dialog__overlay"
      role="dialog"
      aria-modal="true"
      aria-labelledby="settings-dialog-title"
      tabIndex={-1}
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <section className="dialog dialog--settings settings-dialog">
        <header className="dialog__head">
          <div className="min-w-0">
            <h2 id="settings-dialog-title" className="settings-dialog__title">
              {t("nav.settings")}
            </h2>
            <p className="settings-dialog__subtitle">{t("settings.dialog.subtitle")}</p>
          </div>
          <button
            ref={closeBtnRef}
            type="button"
            className="icon-button ml-auto shrink-0"
            aria-label={t("settings.dialog.close")}
            title={t("settings.dialog.close")}
            onClick={onClose}
          >
            <X aria-hidden="true" size={16} />
          </button>
        </header>
        <div className="settings-dialog__body">
          <ViewErrorBoundary>
            <Suspense
              fallback={<div className="settings-dialog__fallback">{t("settings.state.loading")}</div>}
            >
              <SettingsView {...settingsProps} />
            </Suspense>
          </ViewErrorBoundary>
        </div>
      </section>
    </div>
  );
}
