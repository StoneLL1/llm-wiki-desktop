import { invoke } from "@tauri-apps/api/core";
import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { DashboardView } from "../../features/dashboard/DashboardView";
import { ImportView } from "../../features/import/ImportView";
import { useNavigationStore, type AppView } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import type { ConfirmedImport, ImportPreview } from "../../types/import";
import { BottomStatusBar } from "./BottomStatusBar";
import { ConfirmationDialog } from "./ConfirmationDialog";
import { LeftSidebar } from "./LeftSidebar";
import { RightContextPanel } from "./RightContextPanel";
import { TaskLogDrawer } from "./TaskLogDrawer";
import { TopBar } from "./TopBar";

const viewSummaryKeys: Record<AppView, string> = {
  dashboard: "view.dashboard.summary",
  wiki: "view.wiki.summary",
  chat: "view.chat.summary",
  graph: "view.graph.summary",
  agent: "view.agent.summary",
  import: "view.import.summary",
  lint: "view.lint.summary",
  exports: "view.exports.summary",
  settings: "view.settings.summary",
};

const viewActionKeys: Record<AppView, string[]> = {
  dashboard: ["view.dashboard.actionPrimary", "view.dashboard.actionSecondary"],
  wiki: ["view.wiki.actionPrimary", "view.wiki.actionSecondary"],
  chat: ["view.chat.actionPrimary", "view.chat.actionSecondary"],
  graph: ["view.graph.actionPrimary", "view.graph.actionSecondary"],
  agent: ["view.agent.actionPrimary", "view.agent.actionSecondary"],
  import: ["view.import.actionPrimary", "view.import.actionSecondary"],
  lint: ["view.lint.actionPrimary", "view.lint.actionSecondary"],
  exports: ["view.exports.actionPrimary", "view.exports.actionSecondary"],
  settings: ["view.settings.actionPrimary", "view.settings.actionSecondary"],
};

export function AppShell() {
  const { t } = useTranslation();
  const activeView = useNavigationStore((state) => state.activeView);
  const pendingAction = useProjectStore((state) => state.pendingAction);
  const confirmPendingAction = useProjectStore((state) => state.confirmPendingAction);
  const cancelPendingAction = useProjectStore((state) => state.cancelPendingAction);
  const title = t(`nav.${activeView}`);

  return (
    <div className="grid h-full min-w-[1120px] grid-rows-[var(--topbar-h)_1fr_var(--statusbar-h)] bg-[var(--background)] text-[var(--foreground)]">
      <TopBar />

      <div className="grid min-h-0 grid-cols-[var(--sidebar-w)_minmax(0,1fr)_var(--rightpanel-w)]">
        <LeftSidebar />
        <main className="min-w-0 overflow-hidden bg-[var(--background)]">
          <WorkspaceView activeView={activeView} title={title} />
        </main>
        <RightContextPanel />
      </div>

      <BottomStatusBar />
      {pendingAction ? (
        <ConfirmationDialog
          action={pendingAction}
          checkpointExists={false}
          onCancel={() => {
            void cancelPendingAction();
          }}
          onConfirm={() => {
            void confirmPendingAction();
          }}
        />
      ) : null}
      <TaskLogDrawer />
    </div>
  );
}

interface WorkspaceViewProps {
  activeView: AppView;
  title: string;
}

function WorkspaceView({ activeView, title }: WorkspaceViewProps) {
  const { t } = useTranslation();
  const actions = viewActionKeys[activeView];
  const currentProject = useProjectStore((state) => state.currentProject);
  const [importPreview, setImportPreview] = useState<ImportPreview | null>(null);
  const [isConfirmingImport, setIsConfirmingImport] = useState(false);

  const requestImportPreview = useCallback(
    (files: File[]) => {
      const sourcePaths = files
        .map((file) => (file as File & { path?: string }).path ?? "")
        .filter((path) => path.trim().length > 0);

      if (sourcePaths.length === 0) {
        setImportPreview(null);
        return;
      }

      void invoke<ImportPreview>("preview_import", {
        request: {
          projectId: currentProject.projectId,
          projectRootPath: currentProject.rootPath,
          sourcePaths,
          allowDuplicates: false,
          linkDuplicates: false,
        },
      }).then(setImportPreview);
    },
    [currentProject.projectId, currentProject.rootPath],
  );

  const confirmImportPreview = useCallback(() => {
    if (!importPreview) return;
    setIsConfirmingImport(true);
    void invoke<ConfirmedImport>("confirm_import_preview", {
      request: {
        projectId: currentProject.projectId,
        projectRootPath: currentProject.rootPath,
        preview: importPreview,
      },
    })
      .then(() => {
        setImportPreview(null);
      })
      .finally(() => {
        setIsConfirmingImport(false);
      });
  }, [currentProject.projectId, currentProject.rootPath, importPreview]);

  return (
    <section className="flex h-full flex-col">
      <header className="flex h-[52px] items-center gap-3 border-b border-[var(--border)] px-5">
        <h1 className="m-0 text-[16px] font-semibold tracking-[-0.01em]">{title}</h1>
        <span className="truncate font-mono text-xs text-[var(--text-muted)]">{t(viewSummaryKeys[activeView])}</span>
        <div className="ml-auto flex items-center gap-2">
          {actions.map((actionKey, index) => (
            <button
              key={actionKey}
              className={`h-[30px] rounded-[var(--radius-md)] px-3 text-[13px] font-medium ${
                index === 0
                  ? "bg-[var(--foreground)] text-[var(--text-inverse)] hover:bg-[#1a1a1a]"
                  : "border border-[var(--border)] bg-[var(--surface-raised)] hover:bg-[var(--surface-muted)]"
              }`}
              type="button"
            >
              {t(actionKey)}
            </button>
          ))}
        </div>
      </header>

      <div className="min-h-0 flex-1 overflow-auto p-4">
        {activeView === "dashboard" ? (
          <DashboardView />
        ) : activeView === "import" ? (
          <ImportView
            preview={importPreview}
            isConfirming={isConfirmingImport}
            onRequestPreview={requestImportPreview}
            onConfirm={confirmImportPreview}
          />
        ) : (
          <div className="grid gap-3">
          <div className="panel">
            <div className="panel-header">
              <span>{t(`view.${activeView}.paneTitle`)}</span>
            </div>
            <p className="m-0 mt-2 max-w-3xl text-sm leading-6 text-[var(--text-secondary)]">
              {t(`view.${activeView}.emptyState`)}
            </p>
          </div>

          <div className="grid grid-cols-3 gap-3">
            <div className="panel">
              <div className="panel-header">{t("view.shared.localFiles")}</div>
              <p className="m-0 mt-2 text-xs leading-5 text-[var(--text-muted)]">{t("view.shared.localFilesCopy")}</p>
            </div>
            <div className="panel">
              <div className="panel-header">{t("view.shared.taskState")}</div>
              <p className="m-0 mt-2 text-xs leading-5 text-[var(--text-muted)]">{t("view.shared.taskStateCopy")}</p>
            </div>
            <div className="panel">
              <div className="panel-header">{t("view.shared.gitSafety")}</div>
              <p className="m-0 mt-2 text-xs leading-5 text-[var(--text-muted)]">{t("view.shared.gitSafetyCopy")}</p>
            </div>
          </div>
        </div>
        )}
      </div>
    </section>
  );
}
