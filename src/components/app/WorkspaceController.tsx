import { History, PanelRightOpen } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import {
  useWorkflowsController,
  type WorkflowProjectPrerequisiteAction,
  type WorkflowProjectPrerequisiteContext,
} from "../../features/workflows/useWorkflowsController";
import { ProjectAuthorityDialog, type ProjectAuthorityAction } from "../../features/project/ProjectAuthorityDialog";
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
  const settingsSection = useNavigationStore((state) => state.settingsSection);
  const workflowSettingsReturnIntent = useNavigationStore(
    (state) => state.workflowSettingsReturnIntent,
  );
  const clearWorkflowSettingsReturnIntent = useNavigationStore(
    (state) => state.clearWorkflowSettingsReturnIntent,
  );
  const workflowLaunchIntent = useNavigationStore((state) => state.workflowLaunchIntent);
  const clearWorkflowLaunchIntent = useNavigationStore(
    (state) => state.clearWorkflowLaunchIntent,
  );
  const closeSettings = useNavigationStore((state) => state.closeSettings);
  const openTaskDrawer = useTaskStore((state) => state.openDrawer);
  const setWorkflowsSurface = useWorkflowStore((state) => state.setSurface);
  const [projectAuthorityRequest, setProjectAuthorityRequest] = useState<{
    action: ProjectAuthorityAction;
    project: Pick<ProjectSummary, "projectId" | "rootPath">;
    context?: WorkflowProjectPrerequisiteContext;
    workflowRequestEpoch?: number;
  } | null>(null);
  const onProjectPrerequisite = useCallback((
    action: WorkflowProjectPrerequisiteAction,
    context: WorkflowProjectPrerequisiteContext,
  ) => {
    setProjectAuthorityRequest({
      action,
      project: context.project,
      context,
      workflowRequestEpoch: useWorkflowStore.getState().requestEpoch,
    });
  }, []);

  useEffect(() => {
    if (
      projectAuthorityRequest
      && (projectAuthorityRequest.project.projectId !== currentProject.projectId
        || projectAuthorityRequest.project.rootPath !== currentProject.rootPath)
    ) {
      setProjectAuthorityRequest(null);
    }
  }, [currentProject.projectId, currentProject.rootPath, projectAuthorityRequest]);

  const capabilities = useAiCapabilities(
    currentProject,
    activeView === "wiki" || settingsOpen,
  );
  const taskLauncher = useTaskLauncher(currentProject);
  const importWorkflow = useImportWorkflow(
    currentProject,
    activeView,
    taskLauncher,
  );
  const providerWorkflow = useProviderWorkflow(currentProject, capabilities);
  const workflowsController = useWorkflowsController(
    currentProject,
    activeView === "workflows",
    { onProjectPrerequisite },
  );
  const settingsWasOpenRef = useRef(settingsOpen);

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

  useEffect(() => {
    const wasOpen = settingsWasOpenRef.current;
    settingsWasOpenRef.current = settingsOpen;
    if (!wasOpen || settingsOpen || !workflowSettingsReturnIntent) return;

    clearWorkflowSettingsReturnIntent();
    const state = useWorkflowStore.getState();
    const expectedProjectKey = `${currentProject.projectId}\0${currentProject.rootPath}`;
    const selectedRun = state.runs.find(
      (run) => run.taskId === state.selectedTaskId,
    );
    const currentIdentity =
      workflowSettingsReturnIntent.expectedSurface === "preparation"
        ? state.preparation?.projectAccess
        : selectedRun;
    if (
      workflowSettingsReturnIntent.projectId !== currentProject.projectId ||
      workflowSettingsReturnIntent.projectRootPath !== currentProject.rootPath ||
      activeView !== "workflows" ||
      state.projectKey !== expectedProjectKey ||
      state.surface !== workflowSettingsReturnIntent.expectedSurface ||
      currentIdentity?.canonicalIdentityKey !==
        workflowSettingsReturnIntent.expectedCanonicalIdentityKey ||
      currentIdentity?.identityRevision !==
        workflowSettingsReturnIntent.expectedIdentityRevision
    ) {
      return;
    }
    if (
      workflowSettingsReturnIntent.expectedSurface === "preparation" &&
      (state.preparation?.preparationId !==
        workflowSettingsReturnIntent.expectedPreparationId ||
        state.preparation?.preparationRevision !==
          workflowSettingsReturnIntent.expectedPreparationRevision)
    ) {
      return;
    }
    if (
      workflowSettingsReturnIntent.expectedSurface === "detail" &&
      state.selectedTaskId !== workflowSettingsReturnIntent.expectedTaskId
    ) {
      return;
    }

    void workflowsController.prepare(
      workflowSettingsReturnIntent.kind,
      workflowSettingsReturnIntent.scope,
      workflowSettingsReturnIntent.routeSelection,
    );
  }, [
    activeView,
    clearWorkflowSettingsReturnIntent,
    currentProject.projectId,
    currentProject.rootPath,
    settingsOpen,
    workflowSettingsReturnIntent,
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
            className="btn btn--secondary ml-auto"
            onClick={() => setWorkflowsSurface("history")}
            type="button"
          >
            <History aria-hidden="true" size={14} />
            {t("workflows.history.title")}
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
          importWorkflow={importWorkflow}
          workflowsController={workflowsController}
          onOpenTask={openTaskDrawer}
        />
      </div>
      <SettingsDialog
        key={`settings:${currentProject.projectId}\0${currentProject.rootPath}`}
        open={settingsOpen}
        initialSection={settingsSection}
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
          setProjectAuthorityRequest({ action: "manage", project: currentProject });
        }}
      />
      {projectAuthorityRequest ? (
        <ProjectAuthorityDialog
          key={`${projectAuthorityRequest.project.projectId}\0${projectAuthorityRequest.project.rootPath}:${projectAuthorityRequest.action}`}
          action={projectAuthorityRequest.action}
          project={projectAuthorityRequest.project}
          onClose={() => setProjectAuthorityRequest(null)}
          onSatisfied={() => {
            const context = projectAuthorityRequest.context;
            const project = useProjectStore.getState().currentProject;
            if (
              projectAuthorityRequest.project.projectId !== project.projectId
              || projectAuthorityRequest.project.rootPath !== project.rootPath
            ) {
              return;
            }
            if (!context) return;
            const workflow = useWorkflowStore.getState();
            const expected = context.preparation;
            if (
              workflow.projectKey !== `${project.projectId}\0${project.rootPath}`
              || workflow.requestEpoch !== projectAuthorityRequest.workflowRequestEpoch
              || !expected
              || workflow.preparation?.preparationId !== expected.preparationId
              || workflow.preparation?.preparationRevision !== expected.preparationRevision
              || workflow.preparation?.projectAccess.canonicalIdentityKey
                !== expected.projectAccess.canonicalIdentityKey
              || workflow.preparation?.projectAccess.identityRevision
                !== expected.projectAccess.identityRevision
            ) {
              return;
            }
            return context.prepareAgain();
          }}
        />
      ) : null}
    </section>
  );
}
