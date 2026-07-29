import { useWikiStore } from "../features/wiki/wikiStore";
import { useSourceStore } from "../features/wiki/sourceStore";
import { useChatStore } from "./chatStore";
import { useExportStore } from "./exportStore";
import { useGraphStore } from "./graphStore";
import { useImportStore } from "./importStore";
import { useLintStore } from "./lintStore";
import { useNavigationStore } from "./navigationStore";
import { useSettingsStore } from "./settingsStore";

export function resetProjectScopedStores(): void {
  // Project switching is synchronous for the shell, but do not discard lint
  // confirmations before giving the app-global backend registry a chance to
  // cancel them. The store reset still proceeds immediately; cancellation is
  // best-effort and expired actions are rejected server-side.
  void useLintStore.getState().cancelPendingActions();
  useImportStore.getState().resetProjectPresentation("");
  useWikiStore.getState().reset();
  useSourceStore.getState().reset();
  useChatStore.getState().reset();
  useExportStore.getState().reset();
  useGraphStore.getState().reset();
  useLintStore.getState().reset();
  useSettingsStore.getState().reset();
  useNavigationStore.getState().setActiveView("dashboard");
}
