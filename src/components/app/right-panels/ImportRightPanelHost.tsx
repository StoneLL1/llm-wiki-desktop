import { ImportRightPanel } from "../../../features/import/ImportRightPanel";
import { importProjectKey, useImportStore } from "../../../stores/importStore";
import type { RightPanelHostProps } from "./types";

export function ImportRightPanelHost({ currentProject }: RightPanelHostProps) {
  const expectedProjectKey = importProjectKey(currentProject.projectId, currentProject.rootPath);
  const importSession = useImportStore((state) => state.projectKey === expectedProjectKey ? state.session : null);
  const selectedItem = useImportStore((state) => {
    if (state.projectKey !== expectedProjectKey || !state.selectedItemId) return null;
    return state.itemById[state.selectedItemId] ?? null;
  });

  return (
    <ImportRightPanel
      selectedItem={selectedItem}
      sessionId={importSession?.sessionId ?? null}
      projectId={currentProject.projectId}
      projectRootPath={currentProject.rootPath}
      onPreviewMarkdown={(itemId) => useImportStore.getState().openPreview(itemId)}
      onPrimaryAction={(action, itemId) => useImportStore.getState().requestAction(itemId, action)}
    />
  );
}
