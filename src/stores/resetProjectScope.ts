import { useWikiStore } from "../features/wiki/wikiStore";
import { useChatStore } from "./chatStore";
import { useExportStore } from "./exportStore";
import { useGraphStore } from "./graphStore";
import { useImportStore } from "./importStore";
import { useLintStore } from "./lintStore";
import { useNavigationStore } from "./navigationStore";
import { useSettingsStore } from "./settingsStore";

export function resetProjectScopedStores(): void {
  useImportStore.getState().resetProjectPresentation("");
  useWikiStore.getState().reset();
  useChatStore.getState().reset();
  useExportStore.getState().reset();
  useGraphStore.getState().reset();
  useLintStore.getState().reset();
  useSettingsStore.getState().reset();
  useNavigationStore.getState().setActiveView("dashboard");
}
