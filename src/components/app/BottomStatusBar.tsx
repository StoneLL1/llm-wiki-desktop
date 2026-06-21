import { useTranslation } from "react-i18next";
import { useProjectStatus } from "../../hooks/useProjectStatus";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";

export function BottomStatusBar() {
  const { i18n, t } = useTranslation();
  const currentProject = useProjectStore((state) => state.currentProject);
  const runningCount = useTaskStore((state) => state.runningCount);
  const status = useProjectStatus(currentProject.projectId, currentProject.rootPath);

  const defaultAgent = status?.agents?.find((a) => a.isDefault) ?? status?.agents?.find((a) => a.state === "installed") ?? null;
  const agentLabel = defaultAgent ? `${defaultAgent.kind} · ${defaultAgent.version ?? "—"}` : "—";

  const git = status?.git;
  const gitLabel = git?.isRepository
    ? `${git.branch ?? "—"} · ${(git.head ?? "—").slice(0, 7)}`
    : t("status.gitNa");
  const gitCleanLabel = git?.isRepository
    ? git.hasChanges
      ? t("status.gitDirty")
      : t("status.gitClean")
    : t("status.gitNa");

  const activeLanguage = i18n.resolvedLanguage ?? i18n.language;
  const languageLabel = activeLanguage === "zh-CN" ? t("language.zhCN") : t("language.en");

  return (
    <footer className="flex h-7 items-center justify-between border-t border-[var(--border)] bg-[var(--surface)] px-3 font-mono text-[11px] text-[var(--text-secondary)]">
      <div className="flex min-w-0 items-center gap-0">
        <span className="statusbar__item truncate">{currentProject.rootPath}</span>
      </div>
      <div className="flex shrink-0 items-center gap-0">
        <span className="statusbar__item">
          <span className={`dotstatus ${defaultAgent ? "dotstatus--ok" : "dotstatus--err"}`} aria-hidden="true" />
          {agentLabel}
        </span>
        <span className="statusbar__sep" aria-hidden="true" />
        <span className="statusbar__item">
          {t("status.wikiPages", { count: currentProject.wikiPageCount })}
        </span>
        <span className="statusbar__sep" aria-hidden="true" />
        <span className="statusbar__item">{t("status.tasks", { count: runningCount })}</span>
        <span className="statusbar__sep" aria-hidden="true" />
        <span className="statusbar__item">
          {t("status.indexSync")} · {t("status.indexSync.unknown")}
        </span>
        <span className="statusbar__spacer" aria-hidden="true" />
        <span className="statusbar__item">{gitLabel}</span>
        <span className="statusbar__sep" aria-hidden="true" />
        <span className="statusbar__item">
          <span className={`dotstatus ${!git?.isRepository || git?.hasChanges ? "dotstatus--busy" : "dotstatus--ok"}`} aria-hidden="true" />
          {gitCleanLabel}
        </span>
        <span className="statusbar__sep" aria-hidden="true" />
        <span className="statusbar__item">{languageLabel}</span>
      </div>
    </footer>
  );
}
