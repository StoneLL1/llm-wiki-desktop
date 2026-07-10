import { invoke } from "@tauri-apps/api/core";
import { useCallback } from "react";
import { useTranslation } from "react-i18next";

import type { AiCapabilitiesWorkflow } from "../../hooks/useAiCapabilities";
import type {
  LlmProviderConfig,
  LlmProviderKind,
  ProviderStatus,
  ProviderTestResult,
} from "../../types/llm";
import type { ProjectSummary } from "../../types/project";

export interface ProviderWorkflow {
  providers: ProviderStatus[];
  saveProvider: (config: LlmProviderConfig) => Promise<void>;
  saveSecret: (
    provider: LlmProviderKind,
    secret: string,
  ) => Promise<void>;
  deleteSecret: (provider: LlmProviderKind) => Promise<void>;
  testProvider: (config: LlmProviderConfig) => Promise<ProviderTestResult>;
}

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export function useProviderWorkflow(
  project: ProjectSummary,
  capabilities: AiCapabilitiesWorkflow,
): ProviderWorkflow {
  const { t } = useTranslation();
  const projectId = project.projectId;
  const rootPath = project.rootPath;
  const refresh = capabilities.refresh;

  const saveProvider = useCallback(
    async (config: LlmProviderConfig) => {
      if (!hasTauri()) return;
      await invoke("save_llm_provider", {
        request: { projectId, projectRootPath: rootPath, config },
      });
      await refresh();
    },
    [projectId, refresh, rootPath],
  );

  const saveSecret = useCallback(
    async (provider: LlmProviderKind, secret: string) => {
      if (!hasTauri()) return;
      await invoke("store_provider_secret", {
        request: { provider, secret },
      });
      await refresh();
    },
    [refresh],
  );

  const deleteSecret = useCallback(
    async (provider: LlmProviderKind) => {
      if (!hasTauri()) return;
      await invoke("delete_provider_secret", {
        request: { provider, secret: null },
      });
      await refresh();
    },
    [refresh],
  );

  const testProvider = useCallback(
    async (config: LlmProviderConfig): Promise<ProviderTestResult> => {
      if (!hasTauri()) {
        return { ok: false, message: t("provider.testUnavailable") };
      }
      return invoke<ProviderTestResult>("test_llm_provider", {
        request: { projectId, projectRootPath: rootPath, config },
      });
    },
    [projectId, rootPath, t],
  );

  return {
    providers: capabilities.providers,
    saveProvider,
    saveSecret,
    deleteSecret,
    testProvider,
  };
}
