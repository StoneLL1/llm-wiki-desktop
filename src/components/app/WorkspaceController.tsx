import { PanelRightOpen } from "lucide-react";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";

import { RunAgentDialog } from "../../features/agent/RunAgentDialog";
import { useAgentWorkflow } from "../../features/agent/useAgentWorkflow";
import { useImportWorkflow } from "../../features/import/useImportWorkflow";
import { SettingsDialog } from "../../features/settings/SettingsDialog";
import { useProviderWorkflow } from "../../features/settings/useProviderWorkflow";
import { useAiCapabilities } from "../../hooks/useAiCapabilities";
import { useTaskLauncher } from "../../hooks/useTaskLauncher";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
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
  const agentRunPreset = useNavigationStore((state) => state.agentRunPreset);
  const clearAgentRunRequest = useNavigationStore((state) => state.clearAgentRunRequest);
  const closeSettings = useNavigationStore((state) => state.closeSettings);
  const setActiveView = useNavigationStore((state) => state.setActiveView);
  const tasks = useTaskStore((state) => state.tasks);
  const activities = useTaskStore((state) => state.activities);
  const taskOutputs = useTaskStore((state) => state.taskOutputs);
  const openTaskDrawer = useTaskStore((state) => state.openDrawer);

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

  useEffect(() => {
    if (!agentRunPreset || activeView !== "agent") return;
    clearAgentRunRequest();
    agentWorkflow.openRunDialog(agentRunPreset);
  }, [activeView, agentRunPreset, agentWorkflow.openRunDialog, clearAgentRunRequest]);

  const workspaceClass =
    activeView === "wiki" ||
    activeView === "graph" ||
    activeView === "chat" ||
    activeView === "lint" ||
    activeView === "exports" ||
    activeView === "import"
      ? "min-h-0 flex-1 overflow-hidden"
      : "min-h-0 flex-1 overflow-auto p-4";

  return (
    <section className="flex h-full flex-col">
      <header className="workspace-header">
        <h1 className="m-0 text-[16px] font-semibold tracking-[-0.01em]">
          {t(`nav.${activeView}`)}
        </h1>
        {!rightPanelOpen && workspaceFocus === null ? (
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

      <div className={workspaceClass}>
        <WorkspaceRouter
          activeView={activeView}
          capabilities={capabilities}
          taskLauncher={taskLauncher}
          importWorkflow={importWorkflow}
          agentWorkflow={agentWorkflow}
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
      />
    </section>
  );
}
