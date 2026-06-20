import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { DashboardView } from "../../features/dashboard/DashboardView";
import { ExportsView } from "../../features/exports/ExportsView";
import { ImportView } from "../../features/import/ImportView";
import { AgentView } from "../../features/agent/AgentView";
import { ChatView } from "../../features/chat/ChatView";
import { GraphView } from "../../features/graph/GraphView";
import { LintView } from "../../features/lint/LintView";
import { LlmProviderSettings } from "../../features/settings/LlmProviderSettings";
import { WikiView } from "../../features/wiki/WikiView";
import { useNavigationStore, type AppView } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import type { AgentInfo } from "../../types/agent";
import type { AgentKind } from "../../types/agent";
import type { LlmProviderConfig, LlmProviderKind, ProviderStatus, ProviderTestResult } from "../../types/llm";
import type { BackendTask } from "../../types/task";
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
  const tasks = useTaskStore((state) => state.tasks);
  const upsertTask = useTaskStore((state) => state.upsertTask);
  const compilePendingAction = tasks.find((task) => task.status === "waiting_for_confirmation" && task.result?.pendingAction)?.result?.pendingAction;
  const displayedPendingAction = pendingAction ?? compilePendingAction;
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
      {displayedPendingAction ? (
        <ConfirmationDialog
          action={displayedPendingAction}
          checkpointExists={false}
          onCancel={() => {
            if (pendingAction) {
              void cancelPendingAction();
            } else {
              void invoke<BackendTask>("confirm_compile_action", { request: { actionId: displayedPendingAction.id, confirmed: false } }).then(upsertTask);
            }
          }}
          onConfirm={() => {
            if (pendingAction) {
              void confirmPendingAction();
            } else {
              void invoke<BackendTask>("confirm_compile_action", { request: { actionId: displayedPendingAction.id, confirmed: true } }).then(upsertTask);
            }
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
  const setCurrentProject = useProjectStore((state) => state.setCurrentProject);
  const [importPreview, setImportPreview] = useState<ImportPreview | null>(null);
  const [isConfirmingImport, setIsConfirmingImport] = useState(false);
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  const [providers, setProviders] = useState<ProviderStatus[]>([]);
  const upsertTask = useTaskStore((state) => state.upsertTask);
  const openTaskDrawer = useTaskStore((state) => state.openDrawer);
  const tasks = useTaskStore((state) => state.tasks);

  const hasTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

  const projectRequest = {
    projectId: currentProject.projectId,
    projectRootPath: currentProject.rootPath,
  };

  const loadProviders = useCallback(async () => {
    if (!hasTauri) return;
    const statuses = await invoke<ProviderStatus[]>("list_llm_providers", { request: projectRequest });
    setProviders(statuses);
  }, [currentProject.projectId, currentProject.rootPath, hasTauri]);

  const refreshCapabilities = useCallback(async () => {
    if (!hasTauri) return;
    const [detected, statuses] = await Promise.all([
      invoke<AgentInfo[]>("detect_agents", { request: projectRequest }),
      invoke<ProviderStatus[]>("list_llm_providers", { request: projectRequest }),
    ]);
    setAgents(detected);
    setProviders(statuses);
    const agentReady = detected.some((agent) => agent.isDefault && agent.state === "installed");
    const byokReady = statuses.some((provider) => provider.config.enabled && (provider.hasSecret || provider.config.provider === "ollama"));
    const latest = useProjectStore.getState().currentProject;
    setCurrentProject({ ...latest, agentRoute: agentReady ? "agent" : byokReady ? "byok" : "unconfigured" });
  }, [currentProject.projectId, currentProject.rootPath, hasTauri, setCurrentProject]);

  useEffect(() => {
    if (activeView === "agent") {
      void refreshCapabilities();
    } else if (activeView === "settings") {
      void loadProviders();
    }
  }, [activeView, loadProviders, refreshCapabilities]);

  const startCompile = useCallback(async () => {
    if (!hasTauri) return;
    const task = await invoke<BackendTask>("start_wiki_compile", {
      request: { ...projectRequest, route: "auto", agent: null, provider: null },
    });
    upsertTask(task);
    openTaskDrawer(task.id);
  }, [currentProject.projectId, currentProject.rootPath, hasTauri, openTaskDrawer, upsertTask]);

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
      .then(async () => {
        setImportPreview(null);
        await startCompile();
      })
      .finally(() => {
        setIsConfirmingImport(false);
      });
  }, [currentProject.projectId, currentProject.rootPath, importPreview, startCompile]);

  const saveProvider = useCallback(async (config: LlmProviderConfig) => {
    if (!hasTauri) return;
    await invoke("save_llm_provider", { request: { ...projectRequest, config } });
    await refreshCapabilities();
  }, [currentProject.projectId, currentProject.rootPath, hasTauri, refreshCapabilities]);

  const saveProviderSecret = useCallback(async (provider: LlmProviderKind, secret: string) => {
    if (!hasTauri) return;
    await invoke("store_provider_secret", { request: { provider, secret } });
    await refreshCapabilities();
  }, [hasTauri, refreshCapabilities]);

  const setDefaultAgent = useCallback(async (agent: AgentKind) => {
    if (!hasTauri) return;
    await invoke("set_default_agent", { request: { ...projectRequest, agent } });
    await refreshCapabilities();
  }, [currentProject.projectId, currentProject.rootPath, hasTauri, refreshCapabilities]);

  const deleteProviderSecret = useCallback(async (provider: LlmProviderKind) => {
    if (!hasTauri) return;
    await invoke("delete_provider_secret", { request: { provider, secret: null } });
    await refreshCapabilities();
  }, [hasTauri, refreshCapabilities]);

  const testProvider = useCallback(async (config: LlmProviderConfig) => {
    if (!hasTauri) return { ok: false, message: t("provider.testUnavailable") };
    return invoke<ProviderTestResult>("test_llm_provider", { request: { ...projectRequest, config } });
  }, [currentProject.projectId, currentProject.rootPath, hasTauri]);

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

      <div className={activeView === "wiki" || activeView === "graph" || activeView === "chat" || activeView === "lint" || activeView === "exports" ? "min-h-0 flex-1 overflow-hidden" : "min-h-0 flex-1 overflow-auto p-4"}>
        {activeView === "dashboard" ? (
          <DashboardView />
        ) : activeView === "wiki" ? (
          <WikiView />
        ) : activeView === "chat" ? (
          <ChatView />
        ) : activeView === "graph" ? (
          <GraphView />
        ) : activeView === "lint" ? (
          <LintView />
        ) : activeView === "exports" ? (
          <ExportsView />
        ) : activeView === "import" ? (
          <ImportView
            preview={importPreview}
            isConfirming={isConfirmingImport}
            onRequestPreview={requestImportPreview}
            onConfirm={confirmImportPreview}
          />
        ) : activeView === "agent" ? (
          <AgentView
            agents={agents}
            providerCount={providers.filter((provider) => provider.config.enabled).length}
            tasks={tasks.filter((task) => task.taskType === "wiki_compile" || task.taskType === "agent_run" || task.taskType === "llm_request")}
            onOpenTask={openTaskDrawer}
            onDetect={() => { void refreshCapabilities(); }}
            onCompile={() => { void startCompile(); }}
            onSetDefault={(agent) => { void setDefaultAgent(agent); }}
          />
        ) : activeView === "settings" ? (
          <LlmProviderSettings providers={providers} onSaveProvider={saveProvider} onSaveSecret={saveProviderSecret} onDeleteSecret={deleteProviderSecret} onTestProvider={testProvider} />
        ) : (
          <div className="grid gap-3">
          <div className="panel">
            <div className="panel-header">
              <span>{t(`view.${activeView as string}.paneTitle`)}</span>
            </div>
            <p className="m-0 mt-2 max-w-3xl text-sm leading-6 text-[var(--text-secondary)]">
              {t(`view.${activeView as string}.emptyState`)}
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
