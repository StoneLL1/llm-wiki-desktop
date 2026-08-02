import { History, PanelRightOpen } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import { RunAgentDialog } from "../../features/agent/RunAgentDialog";
import { useAgentWorkflow } from "../../features/agent/useAgentWorkflow";
import { useWorkflowsController } from "../../features/workflows/useWorkflowsController";
import { ProjectAuthorityDialog } from "../../features/project/ProjectAuthorityDialog";
import { useImportWorkflow } from "../../features/import/useImportWorkflow";
import { SettingsDialog } from "../../features/settings/SettingsDialog";
import { useProviderWorkflow } from "../../features/settings/useProviderWorkflow";
import { useAiCapabilities } from "../../hooks/useAiCapabilities";
import { useTaskLauncher } from "../../hooks/useTaskLauncher";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import { useWorkflowStore } from "../../stores/workflowStore";
import type { ProjectSummary } from "../../types/project";
import { WorkspaceRouter } from "./WorkspaceRouter";

export function WorkspaceController() {
  const { t } = useTranslation();
  const currentProject = useProjectStore((state) => state.currentProject);
  const activeView = useNavigationStore((state) => state.activeView);
  const rightPanelOpen = useNavigationStore((state) => state.rightPanelOpen);
  const setRightPanelOpen = useNavigationStore(
    (state) => state.setRightPanelOpen,
  );
  const workspaceFocus = useNavigationStore((state) => state.workspaceFocus);
  const settingsOpen = useNavigationStore((state) => state.settingsOpen);
  const workflowLaunchIntent = useNavigationStore((state) => state.workflowLaunchIntent);
  const clearWorkflowLaunchIntent = useNavigationStore(
    (state) => state.clearWorkflowLaunchIntent,
  );
  const closeSettings = useNavigationStore((state) => state.closeSettings);
  const setActiveView = useNavigationStore((state) => state.setActiveView);
  const tasks = useTaskStore((state) => state.tasks);
  const activities = useTaskStore((state) => state.activities);
  const taskOutputs = useTaskStore((state) => state.taskOutputs);
  const openTaskDrawer = useTaskStore((state) => state.openDrawer);
  const setWorkflowsSurface = useWorkflowStore((state) => state.setSurface);
  const [projectAuthorityProject, setProjectAuthorityProject] = useState<
    Pick<ProjectSummary, "projectId" | "rootPath"> | null
  >(null);

  useEffect(() => {
    if (
      projectAuthorityProject
      && (projectAuthorityProject.projectId !== currentProject.projectId
        || projectAuthorityProject.rootPath !== currentProject.rootPath)
    ) {
      setProjectAuthorityProject(null);
    }
  }, [currentProject.projectId, currentProject.rootPath, projectAuthorityProject]);

  const capabilities = useAiCapabilities(
    currentProject,
    activeView === "agent" || activeView === "wiki" || settingsOpen,
  );
  const taskLauncher = useTaskLauncher(currentProject);
  const importWorkflow = useImportWorkflow(
    currentProject,
    activeView,
    taskLauncher,
  );
  const providerWorkflow = useProviderWorkflow(currentProject, capabilities);
  const agentWorkflow = useAgentWorkflow(
    currentProject,
    capabilities,
    taskLauncher,
  );
  const workflowsController = useWorkflowsController(
    currentProject,
    activeView === "workflows",
  );

  useEffect(() => {
    if (!workflowLaunchIntent) return;
    if (
      workflowLaunchIntent.projectId !== currentProject.projectId ||
      workflowLaunchIntent.projectRootPath !== currentProject.rootPath
    ) {
      clearWorkflowLaunchIntent();
      return;
    }
    if (activeView !== "workflows") return;
    clearWorkflowLaunchIntent();
    void workflowsController.prepare(
      workflowLaunchIntent.kind,
      workflowLaunchIntent.scopePreset,
    );
  }, [
    activeView,
    clearWorkflowLaunchIntent,
    currentProject.projectId,
    currentProject.rootPath,
    workflowLaunchIntent,
    workflowsController,
  ]);

  const workspaceClass =
    activeView === "wiki" ||
    activeView === "graph" ||
    activeView === "chat" ||
    activeView === "lint" ||
    activeView === "exports" ||
    activeView === "import"
    || activeView === "workflows"
      ? "min-h-0 flex-1 overflow-hidden"
      : "min-h-0 flex-1 overflow-auto p-4";

  return (
    <section className="flex h-full flex-col">
      <header className="workspace-header">
        <div>
          <h1 className="m-0 text-[16px] font-semibold tracking-[-0.01em]">{t(`nav.${activeView}`)}</h1>
          {activeView === "workflows" ? <p className="m-0 text-[11px] text-[var(--text-muted)]">{t("workflows.header.subtitle")}</p> : null}
        </div>
        {activeView === "workflows" ? (
          <button
            aria-label={t("workflows.history.title")}
            className="icon-button ml-auto"
            onClick={() => setWorkflowsSurface("history")}
            title={t("workflows.history.title")}
            type="button"
          >
            <History aria-hidden="true" size={16} />
          </button>
        ) : null}
        {!rightPanelOpen && workspaceFocus === null ? (
          <button
            aria-controls="right-context-panel"
            aria-expanded="false"
            aria-label={t("shell.contextPanel.open")}
            className={`icon-button ${activeView === "workflows" ? "" : "ml-auto"}`}
            onClick={() => setRightPanelOpen(true)}
            title={t("shell.contextPanel.open")}
            type="button"
          >
            <PanelRightOpen aria-hidden="true" size={16} />
          </button>
        ) : null}
      </header>

      <div className={workspaceClass}>
        <WorkspaceRouter
          activeView={activeView}
          capabilities={capabilities}
          taskLauncher={taskLauncher}
          importWorkflow={importWorkflow}
          agentWorkflow={agentWorkflow}
          workflowsController={workflowsController}
          tasks={tasks}
          activities={activities}
          taskOutputs={taskOutputs}
          onOpenTask={openTaskDrawer}
          onNavigate={setActiveView}
        />
      </div>

      <RunAgentDialog
        key={`agent:${currentProject.projectId}\0${currentProject.rootPath}`}
        open={agentWorkflow.dialogOpen}
        onClose={agentWorkflow.closeRunDialog}
        onRun={(options) => {
          void agentWorkflow.runAgent(options);
        }}
        agents={agentWorkflow.agents}
        providers={capabilities.providers}
        defaultAgentKind={agentWorkflow.defaultAgentKind}
        presetSkill={agentWorkflow.dialogPreset}
      />
      <SettingsDialog
        key={`settings:${currentProject.projectId}\0${currentProject.rootPath}`}
        open={settingsOpen}
        onClose={closeSettings}
        project={currentProject}
        providers={providerWorkflow.providers}
        agents={capabilities.agents}
        importWorkflow={importWorkflow}
        onRefreshCapabilities={capabilities.refresh}
        onSaveProvider={providerWorkflow.saveProvider}
        onSaveSecret={providerWorkflow.saveSecret}
        onDeleteSecret={providerWorkflow.deleteSecret}
        onTestProvider={providerWorkflow.testProvider}
        onManageProjectAuthority={() => {
          closeSettings();
          setProjectAuthorityProject(currentProject);
        }}
      />
      {projectAuthorityProject ? (
        <ProjectAuthorityDialog
          action="manage"
          project={projectAuthorityProject}
          onClose={() => setProjectAuthorityProject(null)}
          onSatisfied={() => undefined}
        />
      ) : null}
    </section>
  );
}
