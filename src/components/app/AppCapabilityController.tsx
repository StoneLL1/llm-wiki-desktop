import { lazy, Suspense, useEffect, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { X } from "lucide-react";

import { useModalDialog } from "../../hooks/useModalDialog";
import { useAppCapabilityStore } from "../../stores/appCapabilityStore";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import { ViewErrorBoundary } from "./ViewErrorBoundary";

const ImportCapabilitiesPanel = lazy(async () => {
  const module = await import("../../features/import/ImportCapabilitiesPanel");
  return { default: module.ImportCapabilitiesPanel };
});

const ImportCapabilityDialog = lazy(async () => {
  const module = await import("../../features/import/ImportCapabilityDialog");
  return { default: module.ImportCapabilityDialog };
});

export function AppCapabilityController() {
  const { t } = useTranslation();
  const initialize = useAppCapabilityStore((state) => state.initialize);
  const refresh = useAppCapabilityStore((state) => state.refresh);
  const managementOpen = useAppCapabilityStore((state) => state.managementOpen);
  const closeManagement = useAppCapabilityStore((state) => state.closeManagement);
  const dialogCapabilityId = useAppCapabilityStore((state) => state.dialogCapabilityId);
  const dialogIntent = useAppCapabilityStore((state) => state.dialogIntent);
  const closeDialog = useAppCapabilityStore((state) => state.closeDialog);
  const capabilities = useAppCapabilityStore((state) => state.capabilities);
  const projectId = useProjectStore((state) => state.currentProject.projectId);
  const projectRootPath = useProjectStore((state) => state.currentProject.rootPath);
  const globalTaskSignature = useTaskStore((state) => Object.values(state.taskById)
    .filter((task) => task.projectId === null && task.operation?.kind === "app_capability_install")
    .map((task) => `${task.id}:${task.updatedAt}:${task.status}`)
    .sort()
    .join("|"));
  const selectedCapability = useMemo(() =>
    capabilities.find((capability) => capability.capabilityId === dialogCapabilityId) ?? null,
  [capabilities, dialogCapabilityId]);
  const managementRef = useModalDialog<HTMLDivElement>({
    open: managementOpen,
    onClose: closeManagement,
    returnFocusSelector: '[data-open-capability-management="true"]',
  });

  useEffect(() => {
    void initialize().catch(() => undefined);
  }, [initialize]);

  useEffect(() => {
    if (!globalTaskSignature && !projectId && !projectRootPath) return;
    const timer = window.setTimeout(() => {
      void refresh(true).catch(() => undefined);
    }, 120);
    return () => window.clearTimeout(timer);
  }, [globalTaskSignature, projectId, projectRootPath, refresh]);

  return (
    <ViewErrorBoundary>
      <Suspense fallback={null}>
        {managementOpen ? (
          <div ref={managementRef} tabIndex={-1} className="dialog-overlay" role="dialog" aria-modal="true" aria-labelledby="app-capability-management-title" inert={dialogIntent ? true : undefined}>
            <section className="app-capability-management-dialog">
              <header>
                <div><h2 id="app-capability-management-title">{t("importV2.capabilityManagement.title")}</h2><p>{t("importV2.capabilityManagement.globalHint")}</p></div>
                <button className="icon-button" type="button" onClick={closeManagement} aria-label={t("importV2.capability.close")} title={t("importV2.capability.close")}><X size={15} aria-hidden="true" /></button>
              </header>
              <div className="app-capability-management-dialog__body"><ImportCapabilitiesPanel /></div>
            </section>
          </div>
        ) : null}
        {dialogIntent ? <ImportCapabilityDialog origin="management" open capability={selectedCapability} intent={dialogIntent} onCancel={closeDialog} /> : null}
      </Suspense>
    </ViewErrorBoundary>
  );
}
