import { useEffect } from "react";
import { useTranslation } from "react-i18next";

import { ActionableErrorNotice } from "../../components/app/ActionableErrorNotice";
import { useUpdateStore } from "../../stores/updateStore";
import type { SaveGlobalUpdatePreferences, UpdateFrequency } from "../../types/update";

export function UpdateSettings() {
  const { t } = useTranslation();
  const store = useUpdateStore();

  useEffect(() => {
    void store.initialize();
  }, [store.initialize]);

  const save = (patch: Partial<SaveGlobalUpdatePreferences>) => {
    const current = store.preferences;
    void store.savePreferences({
      checkUpdates: current.checkUpdates,
      updateFrequency: current.updateFrequency,
      autoDownloadUpdates: current.autoDownloadUpdates,
      promptChangelogBeforeInstall: current.promptChangelogBeforeInstall,
      ...patch,
    }).catch(() => undefined);
  };

  return (
    <section className="settings-section-panel">
      <div>
        <h2 className="m-0 text-[16px] font-semibold">{t("settings.updates.title")}</h2>
        <p className="mt-1 text-[12px] text-[var(--text-muted)]">{t("settings.updates.description")}</p>
      </div>

      <label className="settings-row-card">
        <div>
          <div className="settings-row-card__title">{t("settings.updates.autoCheck")}</div>
          <div className="settings-row-card__meta">{t("settings.updates.autoCheckHelp")}</div>
        </div>
        <input
          checked={store.preferences.checkUpdates}
          disabled={store.preferencesSaving}
          onChange={(event) => save({ checkUpdates: event.target.checked })}
          type="checkbox"
        />
      </label>

      <label className="settings-row-card">
        <div>
          <div className="settings-row-card__title">{t("settings.updates.frequency")}</div>
          <div className="settings-row-card__meta">{t("settings.updates.frequencyHelp")}</div>
        </div>
        <select
          aria-label={t("settings.updates.frequency")}
          className="select h-[30px] w-[140px]"
          disabled={store.preferencesSaving || !store.preferences.checkUpdates}
          onChange={(event) => save({ updateFrequency: event.target.value as UpdateFrequency })}
          value={store.preferences.updateFrequency}
        >
          <option value="daily">{t("settings.updates.frequency.daily")}</option>
          <option value="weekly">{t("settings.updates.frequency.weekly")}</option>
          <option value="never">{t("settings.updates.frequency.never")}</option>
        </select>
      </label>

      <label className="settings-row-card">
        <div>
          <div className="settings-row-card__title">{t("settings.updates.autoDownload")}</div>
          <div className="settings-row-card__meta">{t("settings.updates.autoDownloadHelp")}</div>
        </div>
        <input
          checked={store.preferences.autoDownloadUpdates}
          disabled={store.preferencesSaving}
          onChange={(event) => save({ autoDownloadUpdates: event.target.checked })}
          type="checkbox"
        />
      </label>

      <label className="settings-row-card">
        <div>
          <div className="settings-row-card__title">{t("settings.updates.showChangelog")}</div>
          <div className="settings-row-card__meta">{t("settings.updates.showChangelogHelp")}</div>
        </div>
        <input
          checked={store.preferences.promptChangelogBeforeInstall}
          disabled={store.preferencesSaving}
          onChange={(event) => save({ promptChangelogBeforeInstall: event.target.checked })}
          type="checkbox"
        />
      </label>

      <div className="settings-detail-card">
        <div className="settings-detail-card__metrics">
          <div>
            <div className="text-[12px] text-[var(--text-muted)]">{t("settings.updates.currentVersion")}</div>
            <div className="font-mono text-[13px]">{store.currentVersion}</div>
          </div>
          <div>
            <div className="text-[12px] text-[var(--text-muted)]">{t("settings.updates.latestVersion")}</div>
            <div className="font-mono text-[13px]">
              {store.backendState.offer?.version
                ?? (store.uiStatus === "up_to_date" ? store.currentVersion : t("settings.updates.notChecked"))}
            </div>
          </div>
          <div>
            <div className="text-[12px] text-[var(--text-muted)]">{t("settings.updates.lastChecked")}</div>
            <div className="font-mono text-[11px]">
              {store.preferences.lastCheckedAt
                ? new Date(store.preferences.lastCheckedAt).toLocaleString()
                : t("settings.updates.notChecked")}
            </div>
          </div>
        </div>
        <div className="settings-detail-card__actions">
          <button
            className="settings-button"
            disabled={store.uiStatus === "checking"}
            onClick={() => void store.checkNow().catch(() => undefined)}
            type="button"
          >
            {store.uiStatus === "checking" ? t("settings.updates.checking") : t("settings.updates.checkNow")}
          </button>
          <button className="settings-button settings-button--secondary" onClick={() => store.openDialog(false)} type="button">
            {t("settings.updates.openDetails")}
          </button>
        </div>
        <div className="text-[12px] text-[var(--text-muted)]" role="status">
          {t(`settings.updates.status.${store.uiStatus}`)}
        </div>
        {store.error ? (
          <ActionableErrorNotice
            error={store.error}
            onAction={store.error.actionKind === "retry"
              ? () => store.retry()
              : undefined}
          />
        ) : null}
      </div>
    </section>
  );
}
