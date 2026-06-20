import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";
import { useTranslation } from "react-i18next";

interface UpdateSettingsProps {
  checkUpdates: boolean;
  onToggle: (value: boolean) => void;
}

interface AppSummary {
  name: string;
  version: string;
}

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export function UpdateSettings({ checkUpdates, onToggle }: UpdateSettingsProps) {
  const { t } = useTranslation();
  const [currentVersion, setCurrentVersion] = useState<string>("0.1.0");
  const [latestVersion, setLatestVersion] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);

  const checkNow = async () => {
    setChecking(true);
    try {
      const summary = hasTauri()
        ? await invoke<AppSummary>("get_app_summary")
        : { name: "LLM Wiki Desktop", version: "0.1.0" };
      setCurrentVersion(summary.version);
      setLatestVersion(null);
      setStatus(t("settings.updates.noUpdateSource", { version: summary.version }));
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setStatus(t("settings.updates.error", { message }));
    } finally {
      setChecking(false);
    }
  };

  const confirmDownload = () => {
    if (!latestVersion) {
      setStatus(t("settings.updates.downloadUnavailable"));
      return;
    }

    const approved = typeof window === "undefined"
      ? false
      : window.confirm(t("settings.updates.confirmDownloadPrompt", { version: latestVersion }));
    setStatus(
      approved
        ? t("settings.updates.downloadQueued", { version: latestVersion })
        : t("settings.updates.downloadCancelled"),
    );
  };

  return (
    <section className="grid gap-4">
      <div>
        <h2 className="m-0 text-[16px] font-semibold">{t("settings.updates.title")}</h2>
        <p className="mt-1 text-[12px] text-[var(--text-muted)]">{t("settings.updates.description")}</p>
      </div>

      <label className="flex items-center justify-between gap-3 rounded-[var(--radius-md)] border border-[var(--border)] p-3">
        <div>
          <div className="text-[13px] font-medium">{t("settings.updates.autoCheck")}</div>
          <div className="mt-1 text-[11px] text-[var(--text-muted)]">{t("settings.updates.autoCheckHelp")}</div>
        </div>
        <input type="checkbox" checked={checkUpdates} onChange={(event) => onToggle(event.target.checked)} />
      </label>

      <div className="grid gap-2 rounded-[var(--radius-md)] border border-[var(--border)] p-3">
        <div className="flex items-center justify-between gap-3">
          <div>
            <div className="text-[12px] text-[var(--text-muted)]">{t("settings.updates.currentVersion")}</div>
            <div className="font-mono text-[13px]">{currentVersion}</div>
          </div>
          <div>
            <div className="text-[12px] text-[var(--text-muted)]">{t("settings.updates.latestVersion")}</div>
            <div className="font-mono text-[13px]">{latestVersion ?? t("settings.updates.notChecked")}</div>
          </div>
        </div>
        <div className="flex gap-2">
          <button type="button" className="settings-button" disabled={checking} onClick={() => void checkNow()}>
            {checking ? t("settings.updates.checking") : t("settings.updates.checkNow")}
          </button>
          <button
            type="button"
            className="settings-button settings-button--secondary"
            disabled={!latestVersion}
            onClick={confirmDownload}
          >
            {t("settings.updates.confirmDownload")}
          </button>
        </div>
        {status ? <div className="text-[12px] text-[var(--text-muted)]">{status}</div> : null}
      </div>
    </section>
  );
}
