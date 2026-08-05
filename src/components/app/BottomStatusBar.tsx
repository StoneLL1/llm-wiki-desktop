import { useTranslation } from "react-i18next";
import { useProjectStatus } from "../../hooks/useProjectStatus";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";

export function BottomStatusBar() {
  const { i18n, t } = useTranslation();
  const currentProject = useProjectStore((state) => state.currentProject);
  const runningCount = useTaskStore((state) => state.runningCount);
  const status = useProjectStatus(currentProject.projectId, currentProject.rootPath);

  if (!currentProject.projectId || !currentProject.rootPath) {
    return (
      <footer className="statusbar flex h-7 items-center border-t border-[var(--border)] bg-[var(--surface)] px-3 font-mono text-[11px] text-[var(--text-secondary)]">
        {t("noProject.status")}
      </footer>
    );
  }

  const defaultAgent = status?.agents?.find((a) => a.isDefault) ?? null;
  const agentReady = defaultAgent?.state === "installed";
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
  const wikiLabel = (() => {
    switch (currentProject.inventoryState) {
      case "scanning":
        return t("status.inventory.scanning");
      case "partial":
        return t("status.inventory.partial", { count: currentProject.wikiPageCount });
      case "failed":
        return t("status.inventory.failed");
      default:
        return t("status.wikiPages", { count: currentProject.wikiPageCount });
    }
  })();

  return (
    <footer className="statusbar flex h-7 items-center justify-between border-t border-[var(--border)] bg-[var(--surface)] px-3 font-mono text-[11px] text-[var(--text-secondary)]">
      <div className="statusbar__path flex min-w-0 items-center gap-0">
        <span className="statusbar__item truncate">{currentProject.rootPath}</span>
      </div>
      <div className="statusbar__details flex shrink-0 items-center gap-0">
        <span className="statusbar__item statusbar__agent">
          <span className={`dotstatus ${agentReady ? "dotstatus--ok" : "dotstatus--err"}`} aria-hidden="true" />
          {agentLabel}
        </span>
        <span className="statusbar__item statusbar__wiki">
          {wikiLabel}
        </span>
        <span className="statusbar__item statusbar__tasks">{t("status.tasks", { count: runningCount })}</span>
        <span className="statusbar__item statusbar__index">
          {t("status.indexSync")} · {t("status.indexSync.unknown")}
        </span>
        <span className="statusbar__item statusbar__git-label">{gitLabel}</span>
        <span className="statusbar__item statusbar__git-state">
          <span className={`dotstatus ${!git?.isRepository || git?.hasChanges ? "dotstatus--busy" : "dotstatus--ok"}`} aria-hidden="true" />
          {gitCleanLabel}
        </span>
        <span className="statusbar__item statusbar__language">{languageLabel}</span>
      </div>
    </footer>
  );
}
