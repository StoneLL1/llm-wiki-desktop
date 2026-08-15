import { ImportRightPanel } from "../../../features/import/ImportRightPanel";
import { useImportStore } from "../../../stores/importStore";
import type { RightPanelHostProps } from "./types";

export function ImportRightPanelHost({ currentProject }: RightPanelHostProps) {
  const importSession = useImportStore((state) => state.session);
  const selectedItemId = useImportStore((state) => state.selectedItemId);
  const selectedItem = importSession?.items.find((item) => item.itemId === selectedItemId) ?? null;

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
