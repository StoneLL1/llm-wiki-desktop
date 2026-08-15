import { useWikiStore } from "../features/wiki/wikiStore";
import { useSourceStore } from "../features/wiki/sourceStore";
import { useChatStore } from "./chatStore";
import { useExportStore } from "./exportStore";
import { useGraphStore } from "./graphStore";
import { useImportStore } from "./importStore";
import { useLintStore } from "./lintStore";
import { useNavigationStore } from "./navigationStore";
import { useSettingsStore } from "./settingsStore";
import { useWorkflowStore } from "./workflowStore";

export function resetProjectScopedStores(): void {
  // Project switching is synchronous for the shell, but do not discard lint
  // confirmations before giving the app-global backend registry a chance to
  // cancel them. The store reset still proceeds immediately; cancellation is
  // best-effort and expired actions are rejected server-side.
  void useLintStore.getState().cancelPendingActions();
  useImportStore.getState().resetProjectPresentation("");
  for (const store of [
    useWikiStore,
    useSourceStore,
    useChatStore,
    useExportStore,
    useGraphStore,
    useLintStore,
    useSettingsStore,
    useWorkflowStore,
  ]) store.getState().reset();
  useNavigationStore.getState().setActiveView("dashboard");
}
