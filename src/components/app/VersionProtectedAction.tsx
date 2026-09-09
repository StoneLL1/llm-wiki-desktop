import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useVersionProtection } from "../../hooks/useVersionProtection";
import { useProjectStore } from "../../stores/projectStore";
import { LazyActionableErrorNotice } from "./LazyActionableErrorNotice";

/** Reuse the operation's confirmation surface for one-time protection consent. */
export function VersionProtectedAction({ children, onConfirm, disabled, className }: {
  children: ReactNode;
  onConfirm: () => void | Promise<void>;
  disabled?: boolean;
  className: string;
}) {
  const { t } = useTranslation();
  const project = useProjectStore((state) => state.currentProject);
  const protection = useVersionProtection(project.projectId, project.rootPath);
  const needsEnable = protection.error?.code === "VERSION_NOT_ENABLED" || protection.error?.code === "GIT_REPOSITORY_MISSING";
  const confirm = async () => {
    if (disabled || protection.checking) return;
    const ready = needsEnable ? await protection.enable() : await protection.check();
    if (ready) await onConfirm();
  };
  return <div className="version-protected-action">
    {needsEnable ? <p>{t("versions.enableDescription")}</p> : protection.error ? <LazyActionableErrorNotice error={protection.error} /> : null}
    <button type="button" className={className} disabled={disabled || protection.checking} onClick={() => void confirm()}>
      {protection.checking ? t("versions.checking") : needsEnable ? t("versions.enableContinueAction") : children}
    </button>
  </div>;
}
