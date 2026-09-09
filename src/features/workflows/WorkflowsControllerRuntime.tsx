import { useEffect } from "react";

import type { ProjectSummary } from "../../types/project";
import { useWorkflowsController, type WorkflowsController, type WorkflowsControllerOptions } from "./useWorkflowsController";

// Loaded on the first Workflow visit, then retained for background reconciliation.
export default function WorkflowsControllerRuntime({ project, enabled, onProjectPrerequisite, onReady }: {
  project: ProjectSummary;
  enabled: boolean;
  onProjectPrerequisite: WorkflowsControllerOptions["onProjectPrerequisite"];
  onReady: (projectKey: string, controller: WorkflowsController) => void;
}) {
  const controller = useWorkflowsController(project, enabled, { onProjectPrerequisite });
  useEffect(() => {
    onReady(`${project.projectId}\0${project.rootPath}`, controller);
  }, [controller, onReady, project.projectId, project.rootPath]);
  return null;
}
