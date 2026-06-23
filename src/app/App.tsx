import { useEffect } from "react";

import { AppShell } from "../components/app/AppShell";
import { ProjectStartView } from "../features/project/ProjectStartView";
import { useChatStream } from "../hooks/useChatStream";
import { useTaskEvents } from "../hooks/useTaskEvents";
import "../i18n";
import { useProjectStore } from "../stores/projectStore";
import { useSettingsStore } from "../stores/settingsStore";

export function App() {
  useTaskEvents();
  useChatStream();
  const currentProject = useProjectStore((state) => state.currentProject);
  const bootstrap = useProjectStore((state) => state.bootstrap);
  const loadSettings = useSettingsStore((state) => state.loadSettings);

  useEffect(() => {
    void bootstrap();
  }, [bootstrap]);

  useEffect(() => {
    if (currentProject.projectId && currentProject.rootPath) {
      void loadSettings(currentProject.projectId, currentProject.rootPath);
    }
  }, [currentProject.projectId, currentProject.rootPath, loadSettings]);

  return currentProject.projectId && currentProject.rootPath ? <AppShell /> : <ProjectStartView />;
}
