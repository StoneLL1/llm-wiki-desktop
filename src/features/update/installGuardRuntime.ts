import { useWikiStore } from "../wiki/wikiStore";
import { useLintStore } from "../../stores/lintStore";
import { useImportStore } from "../../stores/importStore";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import { useWorkflowStore } from "../../stores/workflowStore";
import { collectUpdateInstallGuard } from "./installGuard";

export function collectRuntimeUpdateInstallGuard() {
  const wiki = useWikiStore.getState();
  const imports = useImportStore.getState();
  const lint = useLintStore.getState();
  const project = useProjectStore.getState();
  return collectUpdateInstallGuard({
    editor: {
      mode: wiki.mode,
      saveState: wiki.saveState,
      draft: wiki.draft,
      savedMarkdown: wiki.page?.rawMarkdown ?? null,
    },
    importSession: imports.session,
    importConfirming: imports.isConfirming,
    workflowRuns: useWorkflowStore.getState().runs,
    tasks: useTaskStore.getState().tasks,
    projectPendingAction: Boolean(project.pendingAction),
    lintPendingConfirmation: Boolean(lint.fixConfirm || lint.batchConfirmations.length > 0),
  });
}
