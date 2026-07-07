import { Component, Fragment, type ErrorInfo, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { RefreshCw } from "lucide-react";

interface ViewErrorBoundaryProps {
  children: ReactNode;
}

interface ViewErrorBoundaryState {
  error: Error | null;
  retryKey: number;
}

/**
 * Isolates lazy-view failures so a chunk that fails to load (missing asset in a
 * packaged build, transient IO under tauri://, antivirus interference) shows a
 * compact in-place recovery panel instead of unmounting the whole shell.
 * React.lazy needs this paired with Suspense: Suspense handles the *pending*
 * state, this handles the *rejected* state.
 */
export class ViewErrorBoundary extends Component<ViewErrorBoundaryProps, ViewErrorBoundaryState> {
  state: ViewErrorBoundaryState = { error: null, retryKey: 0 };

  static getDerivedStateFromError(error: Error): Partial<ViewErrorBoundaryState> {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error("[shell] view failed to load", error, info);
  }

  private handleRetry = () => {
    this.setState((state) => ({
      error: null,
      retryKey: state.retryKey + 1,
    }));
  };

  render(): ReactNode {
    if (this.state.error) {
      return <ViewLoadError onRetry={this.handleRetry} />;
    }
    return <Fragment key={this.state.retryKey}>{this.props.children}</Fragment>;
  }
}

function ViewLoadError({ onRetry }: { onRetry: () => void }) {
  const { t } = useTranslation();
  return (
    <div
      className="flex h-full min-h-[120px] flex-col items-center justify-center gap-3 px-6 text-center text-[12px] text-[var(--text-muted)]"
      role="alert"
    >
      <p className="m-0 max-w-sm leading-5">{t("shell.view.loadError")}</p>
      <button
        type="button"
        onClick={onRetry}
        className="inline-flex h-[28px] items-center gap-1.5 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] font-medium text-[var(--text-primary)] hover:bg-[var(--surface-muted)]"
      >
        <RefreshCw size={13} aria-hidden="true" />
        {t("shell.view.retry")}
      </button>
    </div>
  );
}
