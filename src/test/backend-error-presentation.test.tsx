import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import { ActionableErrorNotice } from "../components/app/ActionableErrorNotice";
import { ImportCapabilityDialog } from "../features/import/ImportCapabilityDialog";
import { NoProjectWorkspace } from "../features/project/NoProjectWorkspace";
import { AiSettings } from "../features/settings/AiSettings";
import { UpdateSettings } from "../features/settings/UpdateSettings";
import { i18nReady, i18next } from "../i18n";
import {
  backendErrorCode,
  isAiConfigurationErrorCode,
  normalizeBackendError,
  redactBackendErrorDetails,
} from "../lib/backendError";
import { useProjectStore } from "../stores/projectStore";
import type { ImportCapabilityRequirement } from "../types/importV2Presentation";

const serializedBackendError = {
  code: "IMPORT_V2_CAPABILITY_UNAVAILABLE",
  message: "Capability pack was not found at C:\\Users\\Alice\\Private Wiki.",
  details: {
    path: "C:\\Users\\Alice\\Private Wiki\\raw\\source.pdf",
    url: "https://downloads.example.test/pack?token=download-secret&part=1",
  },
  recoverable: true,
  userActionRequired: true,
};

beforeAll(async () => {
  await i18nReady;
});

beforeEach(async () => {
  await i18next.changeLanguage("en");
  invokeMock.mockReset();
  vi.restoreAllMocks();
  useProjectStore.setState({
    assessment: null,
    assessmentError: null,
    assessing: false,
  });
});

describe("BackendError presentation", () => {
  it("serialized BackendError shows a localized summary and recovery action", async () => {
    const onAction = vi.fn(async () => undefined);
    render(<ActionableErrorNotice error={serializedBackendError} onAction={onAction} />);

    expect(screen.getByRole("alert")).toHaveTextContent("A required import capability is unavailable");
    expect(screen.queryByText(/Alice|download-secret/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Open settings" }));
    await waitFor(() => expect(onAction).toHaveBeenCalledWith("open_settings"));
  });

  it("object object input never renders [object Object]", () => {
    const normalized = normalizeBackendError({ nested: { reason: "offline" } });
    expect(normalized.technicalDetails).toContain("offline");
    expect(normalized.technicalDetails).not.toContain("[object Object]");

    render(<ActionableErrorNotice error={{ nested: { reason: "offline" } }} />);
    expect(screen.getByRole("alert")).not.toHaveTextContent("[object Object]");
  });

  it("plain Error, string, null, array, and uninspectable inputs stay safe", () => {
    expect(normalizeBackendError(new Error("offline")).summaryKey).toBe(
      "backendError.summary.generic",
    );
    expect(normalizeBackendError("offline").technicalDetails).toBe("offline");
    expect(normalizeBackendError(null)).toMatchObject({
      summaryKey: "backendError.summary.generic",
      technicalDetails: null,
    });
    expect(normalizeBackendError([null, { reason: "offline" }]).technicalDetails).toContain(
      "offline",
    );

    const revoked = Proxy.revocable({}, {});
    revoked.revoke();
    expect(() => normalizeBackendError(revoked.proxy)).not.toThrow();
    expect(normalizeBackendError(revoked.proxy).technicalDetails).toContain("Uninspectable");

    const trappedError = new Proxy(new Error("hidden"), {
      get() {
        throw new Error("getter trap");
      },
    });
    expect(() => normalizeBackendError(trappedError)).not.toThrow();
    expect(() => backendErrorCode(trappedError)).not.toThrow();
    expect(backendErrorCode(trappedError)).toBeNull();

    expect(normalizeBackendError(new Error("x".repeat(20_000))).technicalDetails?.length)
      .toBeLessThanOrEqual(12_000);

    render(<ActionableErrorNotice error={new Error("offline")} />);
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Something went wrong. Try again or review the technical details.",
    );
  });

  it("circular object input does not throw or expose object coercion", () => {
    const circular: Record<string, unknown> = { reason: "temporarily unavailable" };
    circular.self = circular;

    expect(() => normalizeBackendError(circular)).not.toThrow();
    expect(normalizeBackendError(circular).technicalDetails).toContain("temporarily unavailable");
    expect(normalizeBackendError(circular).technicalDetails).not.toContain("[object Object]");
  });

  it("Authorization api key and cookie secrets are redacted from UI, copy text, and console", async () => {
    const secretDetails = [
      "Authorization: Bearer bearer-secret",
      "api_key=api-key-secret",
      "Cookie: session=cookie-secret",
      "token=bare-token-secret",
      'password="correct horse battery staple"',
      "password='single quoted horse battery staple'",
      "Authorization: 'Bearer single quoted authorization'",
      "cookie='sid=single-quoted-cookie'",
      "password=correct,hunter2",
      "api_key=abc;def",
      "password=correct horse battery staple",
      "secret=alpha beta",
      '{"Authorization":"Bearer json-bearer-secret","api_key":"json-api-secret","cookie":"sid=json-cookie-secret"}',
      "https://example.test/request?access_token=query-secret&item=42",
      "D:/Users/Alice/Private Wiki/raw/source.pdf",
      "/home/alice/private-wiki/raw/source.pdf",
      "\\\\private-server\\alice\\wiki\\source.pdf",
      "/workspace/alice/wiki/source.pdf",
      "C:\\Users\\Alice, Bob\\secret.txt",
      "/Users/Alice, Bob/private.txt",
      "file:///home/alice/private.txt",
      "file://private-server/share/private.txt",
      "path=[C:\\Users\\Alice\\private.txt]",
      "path=<C:\\Users\\Alice\\private.txt>",
      "path: [/home/alice/private.txt]",
    ].join("\n");
    const writeText = vi.fn(async (_text: string) => undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => undefined);

    render(<ActionableErrorNotice error={secretDetails} />);
    fireEvent.click(screen.getByRole("button", { name: "Copy technical details" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));

    const copied = writeText.mock.calls[0]?.[0] ?? "";
    expect(redactBackendErrorDetails(copied)).toBe(copied);
    for (const secret of [
      "bearer-secret",
      "api-key-secret",
      "cookie-secret",
      "query-secret",
      "bare-token-secret",
      "correct horse battery staple",
      "single quoted horse battery staple",
      "single quoted authorization",
      "single-quoted-cookie",
      "correct,hunter2",
      "abc;def",
      "hunter2",
      ";def",
      "alpha beta",
      "json-bearer-secret",
      "json-api-secret",
      "json-cookie-secret",
      "D:/Users/Alice",
      "/home/alice",
      "private-server",
      "/workspace/alice",
      "Bob\\secret.txt",
      "Bob/private.txt",
      "file:///home/alice",
      "file://private-server",
      "Alice\\private.txt",
    ]) {
      expect(document.body.textContent).not.toContain(secret);
      expect(copied).not.toContain(secret);
      expect(consoleError.mock.calls.flat().join(" ")).not.toContain(secret);
    }
  });

  it("zh-CN locale switches summary and action without promoting technical details", async () => {
    const { rerender } = render(<ActionableErrorNotice error={serializedBackendError} onAction={vi.fn()} />);
    expect(screen.getByRole("alert")).toHaveTextContent("A required import capability is unavailable");

    await i18next.changeLanguage("zh-CN");
    rerender(<ActionableErrorNotice error={serializedBackendError} onAction={vi.fn()} />);

    expect(screen.getByRole("alert")).toHaveTextContent("缺少导入所需能力");
    expect(screen.getByRole("button", { name: "打开设置" })).toBeVisible();
    expect(screen.queryByText(/Capability pack was not found/)).not.toBeVisible();
  });

  it("English locale keeps code and technical text out of the primary summary", () => {
    render(<ActionableErrorNotice error={serializedBackendError} />);

    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("A required import capability is unavailable");
    expect(alert.firstElementChild).not.toHaveTextContent("IMPORT_V2_CAPABILITY_UNAVAILABLE");
    expect(alert.firstElementChild).not.toHaveTextContent("Capability pack was not found");
  });

  it("maps real provider codes to recovery actions without treating every LLM error as auth", () => {
    const backendError = (code: string) => ({
      code,
      message: "provider failure",
      details: null,
      recoverable: true,
      userActionRequired: false,
    });

    expect(normalizeBackendError(backendError("LLM_AUTH_FAILED")).actionKind).toBe("reauthorize");
    expect(normalizeBackendError(backendError("LLM_RATE_LIMITED")).actionKind).toBe("retry");
    expect(normalizeBackendError(backendError("LLM_BASE_URL_INVALID")).actionKind).toBe("open_settings");
    expect(normalizeBackendError(backendError("LLM_CANCELLED")).actionKind).toBeNull();
    expect(normalizeBackendError(backendError("LLM_FUTURE_FAILURE"), {
      defaultActionKind: "retry",
    }).actionKind).toBe("retry");
    expect(normalizeBackendError(backendError("LLM_FUTURE_FAILURE"), {
      actionKindOverride: null,
    }).actionKind).toBeNull();
    for (const inheritedPropertyCode of ["toString", "constructor", "__proto__"]) {
      expect(normalizeBackendError(backendError(inheritedPropertyCode))).toMatchObject({
        code: inheritedPropertyCode,
        summaryKey: "backendError.summary.generic",
      });
    }
    expect(normalizeBackendError({
      ...backendError("AGENT_FUTURE_CONFIGURATION_REQUIRED"),
      userActionRequired: true,
    }, {
      defaultActionKind: "retry",
    }).actionKind).toBe("open_settings");
    expect(isAiConfigurationErrorCode("AGENT_UNAVAILABLE")).toBe(true);
    expect(isAiConfigurationErrorCode("PROVIDER_CONFIGURATION_REQUIRED")).toBe(true);
    expect(isAiConfigurationErrorCode("SECRET_MISSING")).toBe(true);
    expect(isAiConfigurationErrorCode("PROJECT_REPAIR_REQUIRED")).toBe(false);
    expect(normalizeBackendError({
      ...backendError("TASK_RECOVERY_FAILED"),
      recoverable: true,
    })).toMatchObject({
      summaryKey: "backendError.summary.task",
      actionKind: "retry",
    });
    expect(normalizeBackendError({
      ...backendError("AGENT_UNAVAILABLE"),
      userActionRequired: true,
    }).actionKind).toBe("open_settings");
    const alreadyNormalized = normalizeBackendError(backendError("LLM_RATE_LIMITED"));
    expect(normalizeBackendError(alreadyNormalized, { actionKindOverride: null }).actionKind)
      .toBeNull();
    expect(normalizeBackendError(alreadyNormalized, { actionKindOverride: "retry" }).actionKind)
      .toBe("retry");
  });

  it("retry failure twice restores the action instead of staying busy", async () => {
    const onAction = vi.fn()
      .mockRejectedValueOnce(new Error("first failure"))
      .mockRejectedValueOnce(new Error("second failure"));
    render(
      <ActionableErrorNotice
        error={{
          code: "UPDATE_CHECK_FAILED",
          message: "Update endpoint is offline.",
          details: null,
          recoverable: true,
          userActionRequired: false,
        }}
        onAction={onAction}
      />,
    );

    const retry = screen.getByRole("button", { name: "Retry" });
    fireEvent.click(retry);
    await waitFor(() => expect(onAction).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(retry).toBeEnabled());
    fireEvent.click(retry);
    await waitFor(() => expect(onAction).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(retry).toBeEnabled());
  });

  it("ignores stale action completion after a new error starts another action", async () => {
    let resolveFirst!: () => void;
    let resolveSecond!: () => void;
    const first = new Promise<void>((resolve) => { resolveFirst = resolve; });
    const second = new Promise<void>((resolve) => { resolveSecond = resolve; });
    const onAction = vi.fn()
      .mockReturnValueOnce(first)
      .mockReturnValueOnce(second);
    const error = (message: string) => ({
      code: "UPDATE_CHECK_FAILED",
      message,
      details: null,
      recoverable: true,
      userActionRequired: false,
    });
    const view = render(<ActionableErrorNotice error={error("first")} onAction={onAction} />);

    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    view.rerender(<ActionableErrorNotice error={error("second")} onAction={onAction} />);
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(screen.getByRole("button", { name: "Working…" })).toBeDisabled();

    await act(async () => resolveFirst());
    expect(screen.getByRole("button", { name: "Working…" })).toBeDisabled();

    await act(async () => resolveSecond());
    await waitFor(() => expect(screen.getByRole("button", { name: "Retry" })).toBeEnabled());
  });

  it("dispatches the restart action only when the consumer supplies restart behavior", async () => {
    const onAction = vi.fn();
    render(<ActionableErrorNotice error={{
      code: "UPDATE_RESTART_REQUIRED",
      message: "restart required",
      details: null,
      recoverable: false,
      userActionRequired: true,
    }} onAction={onAction} />);

    fireEvent.click(screen.getByRole("button", { name: "Restart" }));
    await waitFor(() => expect(onAction).toHaveBeenCalledWith("restart"));
  });

  it("project open errors use the shared localized notice", async () => {
    useProjectStore.setState({ assessmentError: normalizeBackendError(serializedBackendError) });

    render(<NoProjectWorkspace activeView="dashboard" />);

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("A required import capability is unavailable");
    expect(alert).not.toHaveTextContent("[object Object]");
  });

  it("capability install errors remain visible and retryable", async () => {
    const requirement: ImportCapabilityRequirement = {
      requirement: {
        capabilityId: "browser-runtime",
        minimumVersion: "1.0.0",
        protocolVersion: "2",
        targetTriple: "x86_64-pc-windows-msvc",
        acceptedLicenseExpressions: ["Apache-2.0"],
      },
      route: "web.generic.browser",
      available: false,
      installable: true,
      compressedBytes: 100,
      installedBytes: 200,
      modelBytes: null,
      license: "Apache-2.0",
      fallback: null,
    };
    const onInstall = vi.fn()
      .mockRejectedValueOnce(serializedBackendError)
      .mockResolvedValueOnce(undefined);
    render(<ImportCapabilityDialog open requirement={requirement} onInstall={onInstall} onCancel={vi.fn()} />);

    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(screen.getByRole("button", { name: "Install capability" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("A required import capability is unavailable");
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(onInstall).toHaveBeenCalledTimes(2));
  });

  it("updater check errors use the shared retry presentation", async () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", { configurable: true, value: {} });
    invokeMock.mockRejectedValue({
      code: "UPDATE_CHECK_FAILED",
      message: "Authorization: Bearer updater-secret",
      details: { url: "https://example.test/latest.json?token=updater-secret" },
      recoverable: true,
      userActionRequired: false,
    });
    render(<UpdateSettings />);

    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("The update check or download could not complete");
    expect(document.body.textContent).not.toContain("updater-secret");
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  });

  it("updater does not miswire restart-required recovery to another version check", async () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", { configurable: true, value: {} });
    invokeMock.mockRejectedValue({
      code: "UPDATE_RESTART_REQUIRED",
      message: "restart required",
      details: null,
      recoverable: false,
      userActionRequired: true,
    });
    render(<UpdateSettings />);

    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));

    expect(await screen.findByRole("alert")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Restart" })).not.toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("check_app_update");
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  });

  it("provider test errors use localized provider recovery instead of raw backend text", async () => {
    const onTestProvider = vi.fn().mockRejectedValue({
      code: "LLM_AUTH_FAILED",
      message: "api_key=provider-secret",
      details: { authorization: "Bearer provider-secret" },
      recoverable: true,
      userActionRequired: true,
    });
    render(
      <AiSettings
        agents={[]}
        providers={[{
          config: {
            provider: "anthropic",
            model: "claude-sonnet-4-6",
            baseUrl: "https://api.anthropic.com",
            contextWindow: 32_000,
            enabled: true,
          },
          credentialBinding: {
            configId: "2d40f995-0dad-4d50-9a91-737664542dc0",
            providerKind: "anthropic",
            canonicalOrigin: "https://api.anthropic.com",
            credentialAccountId: "provider.binding.v1.project.anthropic.config.origin.1",
            approvedAt: "2026-08-18T00:00:00Z",
            revision: 1,
          },
          hasSecret: true,
          secretMask: "****test",
        }]}
        agentDefault={null}
        contextWindow={32_000}
        onRefreshAgents={vi.fn()}
        onChangeDefault={vi.fn()}
        onSaveProvider={vi.fn()}
        onSaveSecret={vi.fn()}
        onDeleteSecret={vi.fn()}
        onTestProvider={onTestProvider}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "BYOK" }));
    fireEvent.click(screen.getByRole("button", { name: "Test provider" }));

    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent(
      "The AI provider could not complete this request",
    ));
    const apiKeyInput = screen.getByLabelText("API key");
    fireEvent.click(screen.getByRole("button", { name: "Authorize again" }));
    expect(apiKeyInput).toHaveFocus();
    expect(document.body.textContent).not.toContain("provider-secret");
  });

  it("provider retry repeats the failed operation instead of substituting a provider test", async () => {
    const onSaveProvider = vi.fn()
      .mockRejectedValueOnce(normalizeBackendError(new Error("save failed"), {
        defaultSummaryKey: "backendError.summary.provider",
        defaultActionKind: "retry",
        defaultRecoverable: true,
      }))
      .mockResolvedValueOnce(undefined);
    const onTestProvider = vi.fn().mockResolvedValue({ ok: true, message: "connected" });
    render(
      <AiSettings
        agents={[]}
        providers={[]}
        agentDefault={null}
        contextWindow={32_000}
        onRefreshAgents={vi.fn()}
        onChangeDefault={vi.fn()}
        onSaveProvider={onSaveProvider}
        onSaveSecret={vi.fn()}
        onDeleteSecret={vi.fn()}
        onTestProvider={onTestProvider}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "BYOK" }));
    fireEvent.click(screen.getByRole("button", { name: "Save provider" }));
    const retry = await screen.findByRole("button", { name: "Retry" });
    fireEvent.click(retry);

    await waitFor(() => expect(onSaveProvider).toHaveBeenCalledTimes(2));
    expect(onTestProvider).not.toHaveBeenCalled();
  });

  it("priority user error surfaces do not directly stringify unknown failures", () => {
    const priorityFiles = [
      "src/features/project/NoProjectWorkspace.tsx",
      "src/stores/projectStore.ts",
      "src/features/import/ImportCapabilityDialog.tsx",
      "src/features/settings/UpdateSettings.tsx",
      "src/features/settings/useProviderWorkflow.ts",
      "src/features/chat/ChatView.tsx",
      "src/features/chat/PageChatPanel.tsx",
      "src/components/app/TaskLogDrawer.tsx",
      "src/hooks/useTaskLauncher.ts",
      "src/hooks/useTaskEvents.ts",
      "src/features/workflows/useWorkflowsController.ts",
      "src/features/workflows/WorkflowsRightPanel.tsx",
      "src/features/workflows/WorkflowTaskDetail.tsx",
    ];

    for (const file of priorityFiles) {
      const source = readFileSync(resolve(process.cwd(), file), "utf8");
      expect(source, file).not.toContain("String(error)");
    }
    expect(readFileSync(
      resolve(process.cwd(), "src/features/workflows/WorkflowTaskDetail.tsx"),
      "utf8",
    )).not.toContain('"code" in error');
  });
});
