import { lazy, Suspense, useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { X } from "lucide-react";
import { useModalDialog } from "../../hooks/useModalDialog";
import { ViewErrorBoundary } from "../../components/app/ViewErrorBoundary";
import { setSettingsDraftDirty } from "../../lib/settingsDraftGuard";
import type { SettingsViewProps } from "./SettingsView";

const SettingsView = lazy(() => import("./SettingsView").then((m) => ({ default: m.SettingsView })));
interface SettingsDialogProps extends Omit<SettingsViewProps, "onDirtyChange"> {
  open: boolean;
  onClose: () => void;
}

function DiscardDraftDialog({ onCancel, onDiscard }: { onCancel: () => void; onDiscard: () => void }) {
  const { t } = useTranslation();
  const cancelRef = useRef<HTMLButtonElement>(null);
  const ref = useModalDialog<HTMLDivElement>({ open: true, onClose: onCancel, initialFocusRef: cancelRef });
  return <div className="dialog-overlay settings-discard-overlay" ref={ref} role="alertdialog" aria-modal="true" aria-labelledby="settings-discard-title" aria-describedby="settings-discard-description" tabIndex={-1}>
    <section className="dialog settings-discard-dialog"><div className="dialog__head"><h2 id="settings-discard-title">{t("settings.draft.closeTitle")}</h2></div><div className="dialog__body"><p id="settings-discard-description">{t("settings.draft.closeHelp")}</p></div><div className="dialog__foot"><button type="button" ref={cancelRef} className="btn btn--secondary" onClick={onCancel}>{t("settings.draft.keepEditing")}</button><button type="button" className="btn btn--danger" onClick={onDiscard}>{t("settings.draft.discard")}</button></div></section>
  </div>;
}

export function SettingsDialog({ open, onClose, ...settingsProps }: SettingsDialogProps) {
  const { t } = useTranslation();
  const owner = useId();
  const dirtyRef = useRef(false);
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const closeBtnRef = useRef<HTMLButtonElement>(null);
  const requestClose = () => {
    if (dirtyRef.current) setConfirmDiscard(true);
    else onClose();
  };
  const dialogRef = useModalDialog<HTMLDivElement>({ open: open && !confirmDiscard, onClose: requestClose, initialFocusRef: closeBtnRef });
  const projectKey = `${settingsProps.project?.projectId ?? ""}\0${settingsProps.project?.rootPath ?? ""}`;
  useEffect(() => {
    dirtyRef.current = false;
    setConfirmDiscard(false);
    setSettingsDraftDirty(owner, false);
    return () => { setSettingsDraftDirty(owner, false); };
  }, [open, projectKey, owner]);

  if (!open) return null;
  return <>
    <div ref={dialogRef} className="dialog-overlay settings-dialog__overlay" role="dialog" aria-modal="true" aria-labelledby="settings-dialog-title" tabIndex={-1} inert={confirmDiscard} onClick={(event) => { if (event.target === event.currentTarget) requestClose(); }}>
      <section className="dialog dialog--settings settings-dialog">
        <header className="dialog__head"><div className="min-w-0"><h2 id="settings-dialog-title" className="settings-dialog__title">{t("nav.settings")}</h2><p className="settings-dialog__subtitle">{t("settings.dialog.subtitle")}</p></div><button ref={closeBtnRef} type="button" className="icon-button ml-auto shrink-0" aria-label={t("settings.dialog.close")} title={t("settings.dialog.close")} onClick={requestClose}><X aria-hidden="true" size={16} /></button></header>
        <div className="settings-dialog__body"><ViewErrorBoundary><Suspense fallback={<div className="settings-dialog__fallback">{t("settings.state.loading")}</div>}><SettingsView key={projectKey} {...settingsProps} onDirtyChange={(dirty) => { dirtyRef.current = dirty; setSettingsDraftDirty(owner, dirty); }} /></Suspense></ViewErrorBoundary></div>
      </section>
    </div>
    {confirmDiscard ? <DiscardDraftDialog onCancel={() => setConfirmDiscard(false)} onDiscard={() => { dirtyRef.current = false; setSettingsDraftDirty(owner, false); setConfirmDiscard(false); onClose(); }} /> : null}
  </>;
}
