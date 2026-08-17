import { invoke } from "@tauri-apps/api/core";
import { useCallback, useRef } from "react";
import { useTranslation } from "react-i18next";

import type { AiCapabilitiesWorkflow } from "../../hooks/useAiCapabilities";
import {
  normalizeBackendError,
  type NormalizedBackendError,
} from "../../lib/backendError";
import {
  invalidateProjectFacts,
} from "../../stores/projectFactsStore";
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

function providerWorkflowError(error: unknown): NormalizedBackendError {
  return normalizeBackendError(error, {
    defaultSummaryKey: "backendError.summary.provider",
    defaultActionKind: "retry",
    defaultRecoverable: true,
  });
}

export function useProviderWorkflow(
  project: ProjectSummary,
  capabilities: AiCapabilitiesWorkflow,
): ProviderWorkflow {
  const { t } = useTranslation();
  const projectId = project.projectId;
  const rootPath = project.rootPath;
  const projectKey = `${projectId}\0${rootPath}`;
  const latestProjectKey = useRef(projectKey);
  latestProjectKey.current = projectKey;
  const testEpoch = useRef(0);
  const refresh = capabilities.refresh;

  const saveProvider = useCallback(
    async (config: LlmProviderConfig) => {
      if (!hasTauri()) return;
      const requestKey = projectKey;
      try {
        await invoke("save_llm_provider", {
          request: { projectId, projectRootPath: rootPath, config },
        });
        invalidateProjectFacts({ projectId, rootPath }, ["agents", "providers"], "provider_saved");
        if (latestProjectKey.current === requestKey) {
          await refresh(true);
        }
      } catch (error) {
        throw providerWorkflowError(error);
      }
    },
    [projectId, projectKey, refresh, rootPath],
  );

  const boundRequest = useCallback((provider: LlmProviderKind) => {
    const binding = capabilities.providers.find(
      (status) => status.config.provider === provider,
    )?.credentialBinding;
    if (!binding) {
      throw {
        code: "PROVIDER_CREDENTIAL_REAUTH_REQUIRED",
        message: "Save and review the provider destination before using its credential.",
        recoverable: true,
        userActionRequired: true,
      };
    }
    return {
      projectId,
      projectRootPath: rootPath,
      provider,
      configId: binding.configId,
      bindingRevision: binding.revision,
      expectedCanonicalOrigin: binding.canonicalOrigin,
    };
  }, [capabilities.providers, projectId, rootPath]);

  const saveSecret = useCallback(
    async (provider: LlmProviderKind, secret: string) => {
      if (!hasTauri()) return;
      const requestKey = projectKey;
      try {
        await invoke("store_provider_secret", {
          request: {
            ...boundRequest(provider),
            secret,
          },
        });
        invalidateProjectFacts(
          { projectId, rootPath },
          ["providers"],
          "provider_secret_saved",
        );
        if (latestProjectKey.current === requestKey) {
          await refresh(true);
        }
      } catch (error) {
        throw providerWorkflowError(error);
      }
    },
    [boundRequest, projectId, projectKey, refresh, rootPath],
  );

  const deleteSecret = useCallback(
    async (provider: LlmProviderKind) => {
      if (!hasTauri()) return;
      const requestKey = projectKey;
      try {
        await invoke("delete_provider_secret", {
          request: {
            ...boundRequest(provider),
            secret: null,
          },
        });
        invalidateProjectFacts(
          { projectId, rootPath },
          ["providers"],
          "provider_secret_deleted",
        );
        if (latestProjectKey.current === requestKey) {
          await refresh(true);
        }
      } catch (error) {
        throw providerWorkflowError(error);
      }
    },
    [boundRequest, projectId, projectKey, refresh, rootPath],
  );

  const testProvider = useCallback(
    async (config: LlmProviderConfig): Promise<ProviderTestResult> => {
      if (!hasTauri()) {
        return { ok: false, message: t("provider.testUnavailable") };
      }
      const requestKey = projectKey;
      const epoch = ++testEpoch.current;
      try {
        const result = await invoke<ProviderTestResult>("test_llm_provider", {
          request: boundRequest(config.provider),
        });
        if (latestProjectKey.current !== requestKey || testEpoch.current !== epoch) {
          return { ok: false, message: t("provider.testUnavailable") };
        }
        return result;
      } catch (error) {
        throw providerWorkflowError(error);
      }
    },
    [boundRequest, projectKey, t],
  );

  return {
    providers: capabilities.providers,
    saveProvider,
    saveSecret,
    deleteSecret,
    testProvider,
  };
}
