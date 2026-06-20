import { useEffect } from "react";

import { AppShell } from "../components/app/AppShell";
import { useTaskEvents } from "../hooks/useTaskEvents";
import "../i18n";
import { useProjectStore } from "../stores/projectStore";
import { useSettingsStore } from "../stores/settingsStore";

export function App() {
  useTaskEvents();
  const currentProject = useProjectStore((state) => state.currentProject);
  const loadSettings = useSettingsStore((state) => state.loadSettings);

  useEffect(() => {
    void loadSettings(currentProject.projectId, currentProject.rootPath);
  }, [currentProject.projectId, currentProject.rootPath, loadSettings]);

  return <AppShell />;
}
