import { invoke } from "@tauri-apps/api/core";
import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import type { AiCapabilitiesWorkflow } from "../../hooks/useAiCapabilities";
import type {
  TaskLaunchOptions,
  TaskLauncher,
} from "../../hooks/useTaskLauncher";
import { useNavigationStore } from "../../stores/navigationStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { useToastStore } from "../../stores/toastStore";
import type { AgentInfo, AgentKind } from "../../types/agent";
import type { ExportType } from "../../types/export";
import type { ProjectSummary } from "../../types/project";
import type {
  AgentSkill,
  RunAgentOptions,
} from "./RunAgentDialog";

export interface AgentWorkflow {
  agents: AgentInfo[];
  defaultAgentKind: AgentKind | null;
  dialogOpen: boolean;
  dialogPreset: AgentSkill | undefined;
  openRunDialog: (preset?: AgentSkill) => void;
  closeRunDialog: () => void;
  setDefaultAgent: (agent: AgentKind) => Promise<void>;
  runAgent: (options: RunAgentOptions) => Promise<void>;
}

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(error);
}

const exportSkillMap: Partial<Record<AgentSkill, ExportType>> = {
  "html-beautiful-read": "beautiful_read",
  "html-knowledge-card": "knowledge_card",
  "html-concept-map": "concept_map",
  "html-project-report": "project_report",
};

export function useAgentWorkflow(
  project: ProjectSummary,
  capabilities: AiCapabilitiesWorkflow,
  taskLauncher: TaskLauncher,
): AgentWorkflow {
  const { t } = useTranslation();
  const projectId = project.projectId;
  const rootPath = project.rootPath;
  const agents = capabilities.agents;
  const refreshCapabilities = capabilities.refresh;
  const startCompile = taskLauncher.startCompile;
  const startDeepLint = taskLauncher.startDeepLint;
  const startExport = taskLauncher.startExport;
  const setActiveView = useNavigationStore((state) => state.setActiveView);
  const pushToast = useToastStore((state) => state.pushToast);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [dialogPreset, setDialogPreset] = useState<AgentSkill | undefined>();

  const defaultAgentKind = useMemo<AgentKind | null>(
    () =>
      agents.find(
        (agent) => agent.isDefault && agent.state === "installed",
      )?.kind ?? null,
    [agents],
  );

  const openRunDialog = useCallback((preset?: AgentSkill) => {
    setDialogPreset(preset);
    setDialogOpen(true);
  }, []);

  const closeRunDialog = useCallback(() => {
    setDialogOpen(false);
  }, []);

  const setDefaultAgent = useCallback(
    async (agent: AgentKind) => {
      if (!hasTauri()) return;
      try {
        await invoke("set_default_agent", {
          request: { projectId, projectRootPath: rootPath, agent },
        });
        await useSettingsStore.getState().loadSettings(projectId, rootPath);
        await refreshCapabilities();
      } catch (error) {
        pushToast("error", errorMessage(error));
      }
    },
    [projectId, pushToast, refreshCapabilities, rootPath],
  );

  const runAgent = useCallback(
    async (options: RunAgentOptions) => {
      setDialogOpen(false);
      if (!hasTauri()) return;
      const launchOptions: TaskLaunchOptions = {
        route: options.route,
        agent: options.agent,
        provider: options.provider,
      };

      try {
        if (options.skill === "wiki-ingest") {
          await startCompile(launchOptions);
          pushToast(
            "info",
            t("agent.task.skillLoaded", { skill: "wiki-ingest" }),
          );
          return;
        }
        if (options.skill === "wiki-lint") {
          await startDeepLint(launchOptions);
          setActiveView("lint");
          return;
        }
        if (options.skill === "wiki-query") {
          setActiveView("chat");
          pushToast("info", t("agent.task.queryHint"));
          return;
        }

        const exportType = exportSkillMap[options.skill];
        if (!exportType) return;
        if (exportType !== "project_report") {
          setActiveView("exports");
          pushToast("info", t("agent.task.queryHint"));
          return;
        }
        await startExport(exportType, null, launchOptions);
        setActiveView("exports");
      } catch (error) {
        pushToast("error", errorMessage(error));
      }
    },
    [pushToast, setActiveView, startCompile, startDeepLint, startExport, t],
  );

  return {
    agents,
    defaultAgentKind,
    dialogOpen,
    dialogPreset,
    openRunDialog,
    closeRunDialog,
    setDefaultAgent,
    runAgent,
  };
}
