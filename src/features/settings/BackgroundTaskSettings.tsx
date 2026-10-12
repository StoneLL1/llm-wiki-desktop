import { invoke } from "@tauri-apps/api/core";
import { useEffect, useId, useRef, useState } from "react";
import { Bell, CheckCircle2, ExternalLink, RefreshCw } from "lucide-react";
import type { CloseBehavior, SystemNotificationPrefs } from "../../types/settings";
import { useTranslation } from "react-i18next";
import { ActionableErrorNotice } from "../../components/app/ActionableErrorNotice";
import { readNotificationPermission, sendTestNotification, type NotificationPermissionState } from "../../services/notifications";

interface BackgroundTaskSettingsProps {
  closeBehavior: CloseBehavior;
  systemNotifications: SystemNotificationPrefs;
  onChangeCloseBehavior: (behavior: CloseBehavior) => void;
  onChangeSystemNotification: (key: keyof SystemNotificationPrefs, enabled: boolean) => Promise<boolean | void>;
  onRequestNotificationPermission: () => Promise<boolean>;
}

export function BackgroundTaskSettings({ closeBehavior, systemNotifications, onChangeCloseBehavior, onChangeSystemNotification, onRequestNotificationPermission }: BackgroundTaskSettingsProps) {
  const { t } = useTranslation();
  const radioName = useId();
  const mounted = useRef(true);
  const busyRef = useRef(false);
  const permissionEpoch = useRef(0);
  const [permission, setPermission] = useState<NotificationPermissionState>("unknown");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [testSent, setTestSent] = useState(false);
  const readPermission = async (isCurrent: () => boolean) => {
    const result = await readNotificationPermission();
    if (isCurrent()) { setPermission(result); setError(null); }
  };
  const refreshPermission = async () => {
    // Returning focus from a system prompt must not race its explicit result.
    if (busyRef.current) return;
    const epoch = ++permissionEpoch.current;
    const isCurrent = () => mounted.current && epoch === permissionEpoch.current;
    try { await readPermission(isCurrent); }
    catch (reason) { if (isCurrent()) setError(reason); }
  };
  useEffect(() => {
    mounted.current = true;
    void refreshPermission();
    const refresh = () => { void refreshPermission(); };
    window.addEventListener("focus", refresh);
    return () => { mounted.current = false; permissionEpoch.current += 1; window.removeEventListener("focus", refresh); };
  }, []);
  const run = async (operation: (isCurrent: () => boolean) => Promise<void>) => {
    if (busyRef.current) return;
    const epoch = ++permissionEpoch.current;
    const isCurrent = () => mounted.current && epoch === permissionEpoch.current;
    busyRef.current = true; setBusy(true); setError(null); setTestSent(false);
    try { await operation(isCurrent); } catch (reason) { if (isCurrent()) setError(reason); }
    finally { busyRef.current = false; if (isCurrent()) setBusy(false); }
  };
  return <section className="settings-section-panel" id="settings-background" tabIndex={-1}>
    <div><h2 className="settings-view__section-title">{t("settings.background.title")}</h2><p className="settings-view__section-desc">{t("settings.background.description")}</p></div>
    <div id="settings-close-behavior" tabIndex={-1}><h3 className="settings-subheading">{t("settings.background.closeBehavior")}</h3><div className="settings-radio-rows" role="radiogroup" aria-label={t("settings.background.closeBehavior")}>
      {(["minimize_to_tray", "ask", "quit"] as const).map((option) => <label key={option} className={`settings-radio-row${closeBehavior === option ? " is-selected" : ""}`}><input type="radio" name={radioName} checked={closeBehavior === option} onChange={() => onChangeCloseBehavior(option)} /><span><strong>{t(`settings.background.closeOption.${option}`)}</strong><small>{t(`settings.background.closeHelp.${option}`)}</small></span></label>)}
    </div></div>
    <div id="settings-notifications" tabIndex={-1} className="settings-notification-section">
      <div className="settings-section-heading"><h3 className="settings-subheading"><Bell size={14} aria-hidden="true" />{t("settings.background.notifications")}</h3><span className="settings-permission-status" role="status">{permission === "granted" ? <CheckCircle2 size={13} aria-hidden="true" /> : null}{t(`settings.notification.permission.${permission}`)}</span></div>
      <p className="settings-help">{t("settings.background.notificationsHelp")}</p>
      <div className="settings-inline-actions">
        <button type="button" className="settings-button settings-button--secondary" disabled={busy} onClick={() => void run(async (isCurrent) => { const granted = await onRequestNotificationPermission(); if (isCurrent()) setPermission(granted ? "granted" : "denied"); })}><RefreshCw size={12} aria-hidden="true" />{t("settings.background.notificationPermission")}</button>
        <button type="button" className="settings-button settings-button--secondary" disabled={busy || permission !== "granted"} onClick={() => void run(async (isCurrent) => { const sent = await sendTestNotification(); if (isCurrent()) { setTestSent(sent); if (!sent) setPermission("denied"); } })}>{t("settings.notification.test")}</button>
        <button type="button" className="settings-button settings-button--secondary" disabled={busy} onClick={() => void run(async () => { await invoke("open_notification_preferences"); })}><ExternalLink size={12} aria-hidden="true" />{t("settings.notification.openSystem")}</button>
      </div>
      {error ? <ActionableErrorNotice error={error} onAction={() => run(readPermission)} /> : null}
      {testSent ? <p className="settings-inline-status" role="status">{t("settings.notification.testSent")}</p> : null}
      {([ ["onTaskCompleted", "settings.background.notifyCompleted"], ["onTaskFailed", "settings.background.notifyFailed"], ["onConfirmationNeeded", "settings.background.notifyConfirmation"] ] as const).map(([key, label]) => <label className="settings-toggle-row" key={key}><span>{t(label)}</span><input type="checkbox" aria-label={t(label)} checked={systemNotifications[key]} disabled={busy} onChange={(event) => { const enabled = event.target.checked; void run(async (isCurrent) => { const granted = await onChangeSystemNotification(key, enabled); if (isCurrent() && typeof granted === "boolean") setPermission(granted ? "granted" : "denied"); }); }} /></label>)}
    </div>
  </section>;
}
