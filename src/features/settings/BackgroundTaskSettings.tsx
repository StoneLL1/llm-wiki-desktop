import type { CloseBehavior, SystemNotificationPrefs } from "../../types/settings";
import { useTranslation } from "react-i18next";

interface BackgroundTaskSettingsProps {
  closeBehavior: CloseBehavior;
  contextWindow: number;
  systemNotifications: SystemNotificationPrefs;
  onChangeCloseBehavior: (behavior: CloseBehavior) => void;
  onChangeContextWindow: (contextWindow: number) => void;
  onChangeSystemNotification: (key: keyof SystemNotificationPrefs, enabled: boolean) => void;
  onRequestNotificationPermission: () => void;
}

const contextOptions = [4_000, 8_000, 16_000, 32_000, 64_000, 128_000, 256_000, 1_000_000];

export function BackgroundTaskSettings({
  closeBehavior,
  contextWindow,
  systemNotifications,
  onChangeCloseBehavior,
  onChangeContextWindow,
  onChangeSystemNotification,
  onRequestNotificationPermission,
}: BackgroundTaskSettingsProps) {
  const { t } = useTranslation();

  return (
    <section className="settings-section-panel">
      <div>
        <h2 className="m-0 text-[16px] font-semibold">{t("settings.background.title")}</h2>
        <p className="mt-1 text-[12px] text-[var(--text-muted)]">{t("settings.background.description")}</p>
      </div>

      <div className="grid gap-3">
        <div className="text-[12px] font-medium text-[var(--text-secondary)]">{t("settings.background.closeBehavior")}</div>
        <div className="settings-choice-grid">
          {(["minimize_to_tray", "quit"] as const).map((option) => {
            const selected = closeBehavior === option;
            return (
              <button
                key={option}
                type="button"
                onClick={() => onChangeCloseBehavior(option)}
                className={`settings-choice-card${selected ? " is-selected" : ""}`}
              >
                <div className="text-[13px] font-medium">{t(`settings.background.closeOption.${option}`)}</div>
                <div className="mt-1 text-[11px] text-[var(--text-muted)]">{t(`settings.background.closeHelp.${option}`)}</div>
              </button>
            );
          })}
        </div>
      </div>

      <div className="grid gap-2">
        <label className="text-[12px] font-medium text-[var(--text-secondary)]" htmlFor="context-window">
          {t("settings.background.contextWindow")}
        </label>
        <select
          id="context-window"
          className="settings-input"
          value={String(contextWindow)}
          onChange={(event) => onChangeContextWindow(Number(event.target.value))}
        >
          {contextOptions.map((option) => (
            <option key={option} value={option}>
              {t("settings.background.contextWindowOption", { count: option.toLocaleString() })}
            </option>
          ))}
        </select>
      </div>

      <div className="grid gap-2">
        <div className="flex items-center justify-between gap-3">
          <div>
            <div className="text-[12px] font-medium text-[var(--text-secondary)]">{t("settings.background.notifications")}</div>
            <div className="mt-1 text-[11px] text-[var(--text-muted)]">{t("settings.background.notificationsHelp")}</div>
          </div>
          <button className="btn btn--secondary" type="button" onClick={onRequestNotificationPermission}>
            {t("settings.background.notificationPermission")}
          </button>
        </div>
        {([
          ["onTaskCompleted", "settings.background.notifyCompleted"],
          ["onTaskFailed", "settings.background.notifyFailed"],
          ["onConfirmationNeeded", "settings.background.notifyConfirmation"],
        ] as const).map(([key, label]) => (
          <label className="flex items-center justify-between gap-3 text-[12px]" key={key}>
            <span>{t(label)}</span>
            <input
              aria-label={t(label)}
              checked={systemNotifications[key]}
              onChange={(event) => onChangeSystemNotification(key, event.target.checked)}
              type="checkbox"
            />
          </label>
        ))}
      </div>
    </section>
  );
}
