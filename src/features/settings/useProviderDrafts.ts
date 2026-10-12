import { useCallback, useEffect, useRef, useState } from "react";
import { normalizeBackendError, type NormalizedBackendError } from "../../lib/backendError";
import type { LlmProviderConfig, LlmProviderKind, ProviderStatus, ProviderTestResult } from "../../types/llm";

export const providerOrder: LlmProviderKind[] = ["anthropic", "open_ai", "google", "ollama", "custom"];
const defaultBaseUrls: Record<LlmProviderKind, string> = {
  open_ai: "https://api.openai.com",
  anthropic: "https://api.anthropic.com",
  google: "https://generativelanguage.googleapis.com",
  ollama: "http://localhost:11434",
  custom: "",
};
const defaultModels: Record<LlmProviderKind, string> = {
  open_ai: "gpt-5.4",
  anthropic: "claude-sonnet-4-6",
  google: "gemini-2.5-pro",
  ollama: "",
  custom: "",
};
export type ProviderOperation = "save_provider" | "save_secret" | "delete_secret" | "test";
type ProviderFields = { model: string; baseUrl: string; contextWindow: string };
type PendingOperation = { id: number; kind: ProviderOperation; revision: number };
export interface ProviderDraft {
  fields: ProviderFields;
  savedFields: ProviderFields;
  secret: string;
  revision: number;
  pending: PendingOperation | null;
  saved: "provider" | "secret" | "deleted" | null;
  test: { message: string; at: number } | null;
  error: NormalizedBackendError | null;
  failedOperation: ProviderOperation | null;
  approval: boolean;
  sourceSignature: string;
}
interface ProviderDraftOptions {
  providers: ProviderStatus[];
  contextWindow: number;
  onDirtyChange?: (dirty: boolean) => void;
  onSaveProvider: (config: LlmProviderConfig) => Promise<unknown> | unknown;
  onSaveSecret: (provider: LlmProviderKind, secret: string) => Promise<unknown> | unknown;
  onDeleteSecret?: (provider: LlmProviderKind) => Promise<unknown> | unknown;
  onTestProvider?: (config: LlmProviderConfig) => Promise<ProviderTestResult>;
}
const fieldsEqual = (a: ProviderFields, b: ProviderFields) =>
  a.model === b.model && a.baseUrl === b.baseUrl && a.contextWindow === b.contextWindow;
export const providerDraftDirty = (draft: ProviderDraft) => !fieldsEqual(draft.fields, draft.savedFields);
function fieldsFromStatus(provider: LlmProviderKind, status: ProviderStatus | undefined, contextWindow: number): ProviderFields {
  return {
    model: status?.config.model ?? defaultModels[provider],
    baseUrl: provider === "custom" || provider === "ollama" ? status?.config.baseUrl ?? defaultBaseUrls[provider] : defaultBaseUrls[provider],
    contextWindow: String(status?.config.contextWindow ?? contextWindow),
  };
}
const statusSignature = (status: ProviderStatus | undefined, contextWindow: number) => JSON.stringify(status ?? { contextWindow });
const credentialTargetSignature = (status: ProviderStatus | undefined) => {
  const binding = status?.credentialBinding;
  return binding ? JSON.stringify([
    binding.configId, binding.providerKind, binding.canonicalOrigin,
    binding.credentialAccountId, binding.revision,
  ]) : null;
};
function makeDraft(provider: LlmProviderKind, status: ProviderStatus | undefined, contextWindow: number): ProviderDraft {
  const fields = fieldsFromStatus(provider, status, contextWindow);
  return {
    fields,
    savedFields: fields,
    secret: "",
    revision: 0,
    pending: null,
    saved: null,
    test: null,
    error: null,
    failedOperation: null,
    approval: false,
    sourceSignature: statusSignature(status, contextWindow),
  };
}

/** Drafts and raw credentials live only in this mounted settings session. */
export function useProviderDrafts(options: ProviderDraftOptions) {
  const { providers, contextWindow, onDirtyChange } = options;
  const [drafts, setDrafts] = useState(() => Object.fromEntries(providerOrder.map((provider) => [
    provider,
    makeDraft(provider, providers.find((item) => item.config.provider === provider), contextWindow),
  ])) as Record<LlmProviderKind, ProviderDraft>);
  const latest = useRef(drafts);
  const latestProviders = useRef(providers);
  latestProviders.current = providers;
  const mounted = useRef(true);
  const requestId = useRef(0);
  const update = useCallback((provider: LlmProviderKind, change: (draft: ProviderDraft) => ProviderDraft) => {
    if (!mounted.current) return;
    const previous = latest.current;
    const next = change(previous[provider]);
    if (next === previous[provider]) return;
    latest.current = { ...previous, [provider]: next };
    setDrafts(latest.current);
  }, []);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  useEffect(() => {
    for (const provider of providerOrder) {
      const status = providers.find((item) => item.config.provider === provider);
      const signature = statusSignature(status, contextWindow);
      update(provider, (draft) => {
        if (draft.sourceSignature === signature) return draft;
        if (draft.pending) return { ...draft, saved: null, error: null, failedOperation: null, test: null, approval: false };
        const savedFields = fieldsFromStatus(provider, status, contextWindow);
        return {
          ...draft,
          savedFields,
          fields: providerDraftDirty(draft) ? draft.fields : savedFields,
          sourceSignature: signature,
          saved: null,
          error: null,
          failedOperation: null,
          test: null,
          approval: false,
        };
      });
    }
  }, [providers, contextWindow, update]);
  const dirty = Object.values(drafts).some((draft) => providerDraftDirty(draft) || Boolean(draft.secret) || Boolean(draft.pending));
  useEffect(() => { onDirtyChange?.(dirty); }, [dirty, onDirtyChange]);

  const edit = (provider: LlmProviderKind, patch: Partial<ProviderFields> & { secret?: string }) => {
    update(provider, (draft) => {
      const { secret, ...fields } = patch;
      return {
        ...draft,
        fields: { ...draft.fields, ...fields },
        secret: secret ?? draft.secret,
        revision: draft.revision + 1,
        saved: null,
        test: null,
        error: null,
        failedOperation: null,
        approval: false,
      };
    });
  };
  const run = async (provider: LlmProviderKind, kind: ProviderOperation, originApproved = false) => {
    const snapshot = latest.current[provider];
    if (snapshot.pending) return;
    const status = providers.find((item) => item.config.provider === provider);
    const requestSourceSignature = statusSignature(status, contextWindow);
    const requestCredentialTarget = credentialTargetSignature(status);
    if (kind === "save_secret" || kind === "test") {
      if (!status?.credentialBinding || providerDraftDirty(snapshot)) {
        update(provider, (draft) => ({
          ...draft,
          failedOperation: "save_provider",
          error: normalizeBackendError({
            code: "PROVIDER_CREDENTIAL_REAUTH_REQUIRED",
            message: "Save and review the provider destination before using its credential.",
            recoverable: true,
            userActionRequired: true,
          }, {
            defaultSummaryKey: "backendError.summary.provider",
            defaultActionKind: "retry",
            defaultUserActionRequired: true,
          }),
        }));
        return;
      }
    }
    if (kind === "save_secret" && provider === "custom" && !status?.hasSecret && !originApproved) {
      update(provider, (draft) => ({ ...draft, approval: true }));
      return;
    }
    const pending = { id: ++requestId.current, kind, revision: snapshot.revision };
    const config: LlmProviderConfig = {
      provider,
      ...snapshot.fields,
      contextWindow: Number(snapshot.fields.contextWindow),
      enabled: true,
    };
    update(provider, (draft) => ({
      ...draft, pending, error: null, failedOperation: null, saved: null, test: null,
    }));
    try {
      let testResult: ProviderTestResult | undefined;
      if (kind === "save_provider") await options.onSaveProvider(config);
      else if (kind === "save_secret") await options.onSaveSecret(provider, snapshot.secret);
      else if (kind === "delete_secret") await options.onDeleteSecret?.(provider);
      else {
        testResult = await options.onTestProvider?.(config);
        if (!testResult?.ok) throw testResult?.message ?? "Provider connection test is unavailable.";
      }
      update(provider, (draft) => {
        if (draft.pending?.id !== pending.id) return draft;
        const latestStatus = latestProviders.current.find((item) => item.config.provider === provider);
        const sourceSignature = statusSignature(latestStatus, contextWindow);
        const sourceChanged = sourceSignature !== requestSourceSignature;
        const savedFields = kind === "save_provider"
          ? snapshot.fields
          : sourceChanged ? fieldsFromStatus(provider, latestStatus, contextWindow) : draft.savedFields;
        // A successful write updates the baseline, but never overwrites edits made while it ran.
        const next = {
          ...draft,
          pending: null,
          sourceSignature,
          savedFields,
          fields: kind !== "save_provider" && sourceChanged && !providerDraftDirty(draft) ? savedFields : draft.fields,
        };
        if (draft.revision !== pending.revision || kind === "test" && sourceChanged) return next;
        const credentialTargetMatches = requestCredentialTarget !== null
          && credentialTargetSignature(latestStatus) === requestCredentialTarget;
        const credentialWriteConfirmed = credentialTargetMatches && (
          kind === "save_secret" && latestStatus?.hasSecret === true
          || kind === "delete_secret" && latestStatus?.hasSecret === false
        );
        const savedCredential = credentialWriteConfirmed
          ? kind === "save_secret" ? "secret" : "deleted"
          : null;
        return {
          ...next,
          // The submitted secret is cleared after a completed write, but success
          // feedback must also agree with the latest credential facts.
          secret: kind === "save_secret" ? "" : draft.secret,
          saved: kind === "save_provider" ? "provider" : savedCredential,
          test: kind === "test" && testResult ? { message: testResult.message, at: Date.now() } : null,
          approval: false,
        };
      });
    } catch (error) {
      update(provider, (draft) => {
        if (draft.pending?.id !== pending.id) return draft;
        const latestStatus = latestProviders.current.find((item) => item.config.provider === provider);
        const sourceSignature = statusSignature(latestStatus, contextWindow);
        if (sourceSignature !== requestSourceSignature) {
          const savedFields = fieldsFromStatus(provider, latestStatus, contextWindow);
          return {
            ...draft,
            pending: null,
            sourceSignature,
            saved: null,
            error: null,
            failedOperation: null,
            test: null,
            approval: false,
            savedFields,
            fields: providerDraftDirty(draft) ? draft.fields : savedFields,
          };
        }
        if (draft.revision !== pending.revision) return { ...draft, pending: null };
        return {
          ...draft,
          pending: null,
          failedOperation: kind,
          error: normalizeBackendError(error, {
            defaultSummaryKey: "backendError.summary.provider",
            defaultActionKind: kind === "save_secret" ? "reauthorize" : "retry",
            defaultRecoverable: true,
            defaultUserActionRequired: kind === "save_secret",
          }),
        };
      });
    }
  };
  return {
    drafts,
    edit,
    run,
    dismissApproval: (provider: LlmProviderKind) => update(provider, (draft) => ({ ...draft, approval: false })),
  };
}
