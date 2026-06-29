import { invoke } from "@tauri-apps/api/core";
import { PanelRightOpen } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { DashboardView } from "../../features/dashboard/DashboardView";
import { ExportsView } from "../../features/exports/ExportsView";
import { ImportView } from "../../features/import/ImportView";
import { AgentView } from "../../features/agent/AgentView";
import { RunAgentDialog, type AgentSkill, type RunAgentOptions } from "../../features/agent/RunAgentDialog";
import { ChatView } from "../../features/chat/ChatView";
import { GraphView } from "../../features/graph/GraphView";
import { LintView } from "../../features/lint/LintView";
import { SettingsView } from "../../features/settings/SettingsView";
import { WikiView } from "../../features/wiki/WikiView";
import { useImportStore } from "../../stores/importStore";
import { useNavigationStore, type AppView } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { useTaskStore, cancelTaskRequest } from "../../stores/taskStore";
import { useToastStore } from "../../stores/toastStore";
import { useWikiStore } from "../../features/wiki/wikiStore";
import type { AgentInfo } from "../../types/agent";
import type { PendingAction } from "../../types/backend";
import type { AgentKind } from "../../types/agent";
import type { LlmProviderConfig, LlmProviderKind, ProviderStatus, ProviderTestResult } from "../../types/llm";
import type { BackendTask } from "../../types/task";
import type { ExportType } from "../../types/export";
import type { ConfirmedImport, ImportedSource, ImportPreview } from "../../types/import";
import type { FetchedImportUrl } from "../../types/import";
import { articleToMarkdown, extractArticleFromHtml } from "../../lib/readability";
import { BottomStatusBar } from "./BottomStatusBar";
import { ConfirmationDialog } from "./ConfirmationDialog";
import { CompileConflictDialog } from "./CompileConflictDialog";
import { LeftSidebar } from "./LeftSidebar";
import { RightContextPanel } from "./RightContextPanel";
import { TaskLogDrawer } from "./TaskLogDrawer";
import { Toaster } from "./Toaster";
import { TopBar } from "./TopBar";

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(error);
}

function isTerminalTask(task: BackendTask): boolean {
  return task.status === "succeeded" || task.status === "failed" || task.status === "cancelled";
}

export function AppShell() {
  const { t } = useTranslation();
  const activeView = useNavigationStore((state) => state.activeView);
  const rightPanelOpen = useNavigationStore((state) => state.rightPanelOpen);
  const setRightPanelOpen = useNavigationStore((state) => state.setRightPanelOpen);
  const pendingAction = useProjectStore((state) => state.pendingAction);
  const confirmPendingAction = useProjectStore((state) => state.confirmPendingAction);
  const cancelPendingAction = useProjectStore((state) => state.cancelPendingAction);
  const currentProject = useProjectStore((state) => state.currentProject);
  const tasks = useTaskStore((state) => state.tasks);
  const upsertTask = useTaskStore((state) => state.upsertTask);
  const openTaskDrawer = useTaskStore((state) => state.openDrawer);
  const pushToast = useToastStore((state) => state.pushToast);
  const compilePendingAction = tasks.find((task) => task.status === "waiting_for_confirmation" && task.result?.pendingAction)?.result?.pendingAction;
  const displayedPendingAction = pendingAction ?? compilePendingAction;
  const title = t(`nav.${activeView}`);

  useEffect(() => {
    if (
      typeof window.matchMedia === "function" &&
      window.matchMedia("(max-width: 1180px)").matches
    ) {
      setRightPanelOpen(false);
    }
  }, [setRightPanelOpen]);

  useEffect(() => {
    if (!rightPanelOpen) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (
        event.key === "Escape" &&
        !event.defaultPrevented &&
        !document.querySelector('[aria-modal="true"]')
      ) {
        setRightPanelOpen(false);
      }
    };
    document.addEventListener("keydown", closeOnEscape);
    return () => document.removeEventListener("keydown", closeOnEscape);
  }, [rightPanelOpen, setRightPanelOpen]);

  const confirmProjectAction = useCallback(async () => {
    const action = pendingAction;
    const confirmed = await confirmPendingAction();
    if (
      !confirmed ||
      !action ||
      (action.actionType !== "delete_source" && action.actionType !== "replace_source")
    ) {
      return;
    }
    try {
      const task = await invoke<BackendTask>("start_wiki_compile", {
        request: {
          projectId: currentProject.projectId,
          projectRootPath: currentProject.rootPath,
          route: "auto",
          agent: null,
          provider: null,
        },
      });
      upsertTask(task);
      openTaskDrawer(task.id);
    } catch (error) {
      pushToast("error", t("import.sourceCompileError", { message: errorMessage(error) }));
    }
  }, [
    confirmPendingAction,
    currentProject.projectId,
    currentProject.rootPath,
    openTaskDrawer,
    pendingAction,
    pushToast,
    t,
    upsertTask,
  ]);

  return (
    <div className={`app-shell ${rightPanelOpen ? "is-right-open" : "is-right-collapsed"}`}>
      <TopBar />

      <div className="app-shell__workbench">
        <LeftSidebar />
        <main className="app-shell__main">
          <WorkspaceView activeView={activeView} title={title} />
        </main>
        {rightPanelOpen ? <RightContextPanel /> : null}
        {rightPanelOpen ? (
          <button
            aria-hidden="true"
            className="right-panel__backdrop"
            onClick={() => setRightPanelOpen(false)}
            tabIndex={-1}
            type="button"
          />
        ) : null}
      </div>

      <BottomStatusBar />
      <Toaster />
      {displayedPendingAction?.actionType === "merge_conflict" && compilePendingAction ? (
        <CompileConflictDialog
          action={displayedPendingAction}
          onCancel={() => {
            void invoke<BackendTask>("confirm_compile_action", {
              request: { actionId: displayedPendingAction.id, confirmed: false },
            }).then(upsertTask);
          }}
          onResolved={upsertTask}
        />
      ) : displayedPendingAction ? (
        <ConfirmationDialog
          action={displayedPendingAction}
          checkpointExists={displayedPendingAction.checkpointHash != null}
          onCancel={() => {
            if (pendingAction) {
              void cancelPendingAction();
            } else {
              void invoke<BackendTask>("confirm_compile_action", { request: { actionId: displayedPendingAction.id, confirmed: false } }).then(upsertTask);
            }
          }}
          onConfirm={() => {
            if (pendingAction) {
              void confirmProjectAction();
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
  const rightPanelOpen = useNavigationStore((state) => state.rightPanelOpen);
  const setRightPanelOpen = useNavigationStore((state) => state.setRightPanelOpen);
  const currentProject = useProjectStore((state) => state.currentProject);
  const setCurrentProject = useProjectStore((state) => state.setCurrentProject);
  const setPendingAction = useProjectStore((state) => state.setPendingAction);
  const setActiveView = useNavigationStore((state) => state.setActiveView);
  const pushToast = useToastStore((state) => state.pushToast);
  const setImportPreview = useImportStore((state) => state.setPreview);
  const setIsConfirmingImport = useImportStore((state) => state.setIsConfirming);
  const isConfirmingImport = useImportStore((state) => state.isConfirming);
  const setImportedSources = useImportStore((state) => state.setImportedSources);
  const importedSources = useImportStore((state) => state.importedSources);
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  const [providers, setProviders] = useState<ProviderStatus[]>([]);
  const upsertTask = useTaskStore((state) => state.upsertTask);
  const openTaskDrawer = useTaskStore((state) => state.openDrawer);
  const tasks = useTaskStore((state) => state.tasks);

  const hasTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

  useEffect(() => {
    // Clear stale import staging state when the active project changes so a
    // previously-selected file / preview from another project does not leak.
    useImportStore.getState().reset();
  }, [currentProject.projectId]);

  useEffect(() => {
    if (!hasTauri || activeView !== "import" || !currentProject.projectId) return;
    let active = true;
    void invoke<ImportedSource[]>("list_imported_sources", {
      request: {
        projectId: currentProject.projectId,
        projectRootPath: currentProject.rootPath,
      },
    })
      .then((sources) => {
        if (active) setImportedSources(sources);
      })
      .catch((error) => {
        if (active) pushToast("error", t("import.sourceListError", { message: errorMessage(error) }));
      });
    return () => {
      active = false;
    };
  }, [activeView, currentProject.projectId, currentProject.rootPath, currentProject.sourceCount, hasTauri, pushToast, t]);

  const projectRequest = {
    projectId: currentProject.projectId,
    projectRootPath: currentProject.rootPath,
  };

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
      void refreshCapabilities();
    }
  }, [activeView, refreshCapabilities]);

  // Detect agents/providers once per project switch so the dashboard first
  // screen doesn't read as "Agent not detected" before the user opens the
  // Agent/Settings view (PRD-AGENT-001, roadmap 2.3).
  useEffect(() => {
    if (!hasTauri || !currentProject.projectId) return;
    void refreshCapabilities();
  }, [currentProject.projectId, hasTauri, refreshCapabilities]);

  const startCompile = useCallback(async () => {
    if (!hasTauri) return;
    const task = await invoke<BackendTask>("start_wiki_compile", {
      request: { ...projectRequest, route: "auto", agent: null, provider: null },
    });
    upsertTask(task);
    openTaskDrawer(task.id);
  }, [currentProject.projectId, currentProject.rootPath, hasTauri, openTaskDrawer, upsertTask]);

  const requestImportPreview = useCallback(
    (paths: string[]) => {
      const sourcePaths = paths
        .map((path) => path.trim())
        .filter((path) => path.trim().length > 0);

      if (sourcePaths.length === 0) {
        setImportPreview(null);
        return;
      }

      void invoke<BackendTask>("preview_import", {
        request: {
          projectId: currentProject.projectId,
          projectRootPath: currentProject.rootPath,
          sourcePaths,
          allowDuplicates: false,
          linkDuplicates: false,
        },
      })
        .then(async (started) => {
          let task = started;
          upsertTask(task);
          openTaskDrawer(task.id);
          while (!isTerminalTask(task)) {
            await new Promise((resolve) => setTimeout(resolve, 250));
            task = await invoke<BackendTask>("get_task", { request: { taskId: task.id } });
            upsertTask(task);
          }
          if (task.status !== "succeeded") {
            throw new Error(task.error?.message ?? `Import preview ${task.status}.`);
          }
          const preview = await invoke<ImportPreview>("get_import_preview", {
            request: {
              projectId: currentProject.projectId,
              projectRootPath: currentProject.rootPath,
              taskId: task.id,
            },
          });
          const active = useProjectStore.getState().currentProject;
          if (
            active.projectId === currentProject.projectId &&
            active.rootPath === currentProject.rootPath
          ) {
            setImportPreview(preview);
          }
        })
        .catch((error) => {
          setImportPreview(null);
          pushToast("error", t("import.previewError", { message: errorMessage(error) }));
        });
    },
    [currentProject.projectId, currentProject.rootPath, openTaskDrawer, pushToast, t, upsertTask],
  );

  const requestTextImportPreview = useCallback(
    async (kind: "clipboard" | "url", value: string) => {
      try {
        let content = value;
        let sourceName = "clipboard-import";
        let title: string | null = null;
        let author: string | null = null;
        if (kind === "url") {
          const fetched = await invoke<FetchedImportUrl>("fetch_import_url", {
            request: {
              projectId: currentProject.projectId,
              projectRootPath: currentProject.rootPath,
              url: value,
            },
          });
          const article = extractArticleFromHtml(fetched.html, fetched.url);
          if (!article) throw new Error(t("import.readabilityError"));
          content = articleToMarkdown(article, fetched.url);
          sourceName = article.title || new URL(fetched.url).hostname;
          title = article.title || null;
          author = article.byline;
        }
        const preview = await invoke<ImportPreview>("preview_text_import", {
          request: {
            projectId: currentProject.projectId,
            projectRootPath: currentProject.rootPath,
            kind,
            sourceName,
            content,
            title,
            author,
          },
        });
        setImportPreview(preview);
      } catch (error) {
        setImportPreview(null);
        pushToast("error", t("import.previewError", { message: errorMessage(error) }));
      }
    },
    [currentProject.projectId, currentProject.rootPath, pushToast, t],
  );

  const requestSourceAction = useCallback(
    async (kind: "delete" | "replace", targetPath: string, replacementPath?: string) => {
      try {
        const action = await invoke<PendingAction>(
          kind === "delete" ? "request_delete_source" : "request_replace_source",
          {
            request: {
              projectId: currentProject.projectId,
              projectRootPath: currentProject.rootPath,
              targetPath,
              ...(kind === "replace" ? { replacementPath } : {}),
            },
          },
        );
        setPendingAction(action);
      } catch (error) {
        pushToast("error", t("import.sourceActionError", { message: errorMessage(error) }));
      }
    },
    [currentProject.projectId, currentProject.rootPath, pushToast, setPendingAction, t],
  );

  const confirmImportPreview = useCallback(
    (opts: { createCheckpoint: boolean; compileAfterImport: boolean }) => {
      const preview = useImportStore.getState().preview;
      if (!preview) return;
      setIsConfirmingImport(true);
      void invoke<ConfirmedImport>("confirm_import_preview", {
        request: {
          projectId: currentProject.projectId,
          projectRootPath: currentProject.rootPath,
          preview,
          createCheckpoint: opts.createCheckpoint,
        },
      })
        .then(async () => {
          setImportPreview(null);
          // Promoted source originals now live under wiki/sources/; re-scan so
          // they appear in the tree immediately, even if compile is skipped.
          await useWikiStore.getState().scan(
            currentProject.projectId,
            currentProject.rootPath,
          );
          if (opts.compileAfterImport) {
            await startCompile();
          }
        })
        .catch((error) => {
          pushToast("error", t("import.confirmError", { message: errorMessage(error) }));
        })
        .finally(() => {
          setIsConfirmingImport(false);
        });
    },
    [currentProject.projectId, currentProject.rootPath, pushToast, setIsConfirmingImport, setImportPreview, startCompile, t],
  );

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
    try {
      await invoke("set_default_agent", { request: { ...projectRequest, agent } });
      await useSettingsStore.getState().loadSettings(
        currentProject.projectId,
        currentProject.rootPath,
      );
      await refreshCapabilities();
    } catch (error) {
      pushToast("error", errorMessage(error));
    }
  }, [currentProject.projectId, currentProject.rootPath, hasTauri, pushToast, refreshCapabilities]);

  const defaultAgentKind = useMemo<AgentKind | null>(
    () =>
      agents.find((a) => a.isDefault && a.state === "installed")?.kind
      ?? null,
    [agents],
  );

  const [runDialogOpen, setRunDialogOpen] = useState(false);
  const [runDialogPreset, setRunDialogPreset] = useState<AgentSkill | undefined>(undefined);
  const openRunDialog = useCallback((preset?: AgentSkill) => {
    setRunDialogPreset(preset);
    setRunDialogOpen(true);
  }, []);

  const runAgent = useCallback(async (options: RunAgentOptions) => {
    setRunDialogOpen(false);
    if (!hasTauri) return;
    const route = options.route;
    const agent = options.agent;
    const provider = options.provider;
    try {
      if (options.skill === "wiki-ingest") {
        const task = await invoke<BackendTask>("start_wiki_compile", {
          request: { ...projectRequest, route, agent, provider },
        });
        upsertTask(task);
        openTaskDrawer(task.id);
        pushToast("info", t("agent.task.skillLoaded", { skill: "wiki-ingest" }));
        return;
      }
      if (options.skill === "wiki-lint") {
        const task = await invoke<BackendTask>("start_deep_lint", {
          request: { ...projectRequest, route, agent, provider },
        });
        upsertTask(task);
        openTaskDrawer(task.id);
        setActiveView("lint");
        return;
      }
      if (options.skill === "wiki-query") {
        setActiveView("chat");
        pushToast("info", t("agent.task.queryHint"));
        return;
      }
      const exportSkillMap: Partial<Record<AgentSkill, ExportType>> = {
        "html-beautiful-read": "beautiful_read",
        "html-knowledge-card": "knowledge_card",
        "html-concept-map": "concept_map",
        "html-project-report": "project_report",
      };
      const exportType = exportSkillMap[options.skill];
      if (!exportType) return;
      // Single-page exports need a source page picker — defer to Exports view.
      if (exportType !== "project_report") {
        setActiveView("exports");
        pushToast("info", t("agent.task.queryHint"));
        return;
      }
      const task = await invoke<BackendTask>("start_export", {
        request: { ...projectRequest, exportType, sourcePath: null, route, agent, provider },
      });
      upsertTask(task);
      openTaskDrawer(task.id);
      setActiveView("exports");
    } catch (error) {
      pushToast("error", errorMessage(error));
    }
  }, [
    currentProject.projectId,
    currentProject.rootPath,
    hasTauri,
    openTaskDrawer,
    projectRequest,
    pushToast,
    setActiveView,
    t,
    upsertTask,
  ]);

  const cancelTask = useCallback(async (taskId: string) => {
    try {
      await cancelTaskRequest(taskId);
    } catch (error) {
      pushToast("error", t("task.cancelError", { message: errorMessage(error) }));
    }
  }, [pushToast, t]);

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
      <header className="workspace-header">
        <h1 className="m-0 text-[16px] font-semibold tracking-[-0.01em]">{title}</h1>
        {!rightPanelOpen ? (
          <button
                aria-controls="right-context-panel"
                aria-expanded="false"
                aria-label={t("shell.contextPanel.open")}
            className="icon-button ml-auto"
            onClick={() => setRightPanelOpen(true)}
            title={t("shell.contextPanel.open")}
            type="button"
          >
            <PanelRightOpen aria-hidden="true" size={16} />
          </button>
        ) : null}
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
            isConfirming={isConfirmingImport}
            onRequestPreview={requestImportPreview}
            onRequestClipboard={(content) => { void requestTextImportPreview("clipboard", content); }}
            onRequestUrl={(url) => { void requestTextImportPreview("url", url); }}
            importedSources={importedSources}
            onDeleteSource={(path) => { void requestSourceAction("delete", path); }}
            onReplaceSource={(path, replacementPath) => { void requestSourceAction("replace", path, replacementPath); }}
            onConfirm={confirmImportPreview}
          />
        ) : activeView === "agent" ? (
          <AgentView
            agents={agents}
            providers={providers}
            tasks={tasks.filter((task) => task.taskType === "wiki_compile" || task.taskType === "agent_run" || task.taskType === "llm_request" || task.taskType === "deep_lint" || task.taskType === "export")}
            onOpenTask={openTaskDrawer}
            onDetect={() => { void refreshCapabilities(); }}
            onRunAgent={(preset) => openRunDialog(preset)}
            onSetDefault={(agent) => { void setDefaultAgent(agent); }}
            onCancelTask={(taskId) => { void cancelTask(taskId); }}
            onNavigate={(view) => setActiveView(view)}
          />
        ) : activeView === "settings" ? (
          <SettingsView
            project={currentProject}
            providers={providers}
            agents={agents}
            onRefreshCapabilities={refreshCapabilities}
            onSaveProvider={saveProvider}
            onSaveSecret={saveProviderSecret}
            onDeleteSecret={deleteProviderSecret}
            onTestProvider={testProvider}
          />
        ) : (
          <div className="grid gap-3">
          <div className="panel">
            <div className="panel-header">
              <span>{t(`view.${activeView as string}.paneTitle`)}</span>
            </div>
            <p className="m-0 mt-2 max-w-3xl text-[13px] leading-6 text-[var(--text-secondary)]">
              {t(`view.${activeView as string}.emptyState`)}
            </p>
          </div>

          <div className="grid grid-cols-3 gap-3">
            <div className="panel">
              <div className="panel-header">{t("view.shared.localFiles")}</div>
              <p className="m-0 mt-2 text-[12px] leading-5 text-[var(--text-muted)]">{t("view.shared.localFilesCopy")}</p>
            </div>
            <div className="panel">
              <div className="panel-header">{t("view.shared.taskState")}</div>
              <p className="m-0 mt-2 text-[12px] leading-5 text-[var(--text-muted)]">{t("view.shared.taskStateCopy")}</p>
            </div>
            <div className="panel">
              <div className="panel-header">{t("view.shared.gitSafety")}</div>
              <p className="m-0 mt-2 text-[12px] leading-5 text-[var(--text-muted)]">{t("view.shared.gitSafetyCopy")}</p>
            </div>
          </div>
        </div>
        )}
      </div>
      <RunAgentDialog
        open={runDialogOpen}
        onClose={() => setRunDialogOpen(false)}
        onRun={(options) => { void runAgent(options); }}
        agents={agents}
        providers={providers}
        defaultAgentKind={defaultAgentKind}
        presetSkill={runDialogPreset}
      />
    </section>
  );
}
