import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { type ComponentProps } from "react";
import { describe, expect, it, vi } from "vitest";
import "../../i18n";
import type { AgentInfo } from "../../types/agent";
import type { ProviderStatus } from "../../types/llm";
import { AiSettings } from "./AiSettings";

const agents: AgentInfo[] = [
  {
    kind: "codex",
    command: "codex",
    state: "installed",
    version: "0.135.0",
    executablePath: "C:/bin/codex.exe",
    isDefault: false,
    installGuidance: "",
    error: null,
  },
  {
    kind: "claude",
    command: "claude",
    state: "missing",
    version: null,
    executablePath: null,
    isDefault: false,
    installGuidance: "Install Claude Code manually.",
    error: null,
  },
];

const providers: ProviderStatus[] = [
  {
    config: {
      provider: "anthropic",
      model: "claude-test",
      baseUrl: "https://api.anthropic.com",
      contextWindow: 100_000,
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
  },
];

function renderAi(overrides: Partial<ComponentProps<typeof AiSettings>> = {}) {
  return render(
    <AiSettings
      agents={agents}
      providers={providers}
      agentDefault={null}
      contextWindow={32_000}
      onRefreshAgents={vi.fn()}
      onChangeDefault={vi.fn()}
      onSaveProvider={vi.fn()}
      onSaveSecret={vi.fn()}
      onDeleteSecret={vi.fn()}
      onTestProvider={vi.fn().mockResolvedValue({ ok: true, message: "Connected" })}
      {...overrides}
    />,
  );
}

describe("AiSettings", () => {
  it("switches between Local CLI and BYOK in one AI settings surface", () => {
    renderAi();

    expect(screen.getByRole("button", { name: /local cli/i })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByText(/codex cli/i)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /byok/i }));

    expect(screen.getByRole("button", { name: /byok/i })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: /anthropic/i })).toBeInTheDocument();
  });

  it("sets an installed Agent CLI as the default route", () => {
    const onChangeDefault = vi.fn();
    renderAi({ onChangeDefault });

    fireEvent.click(screen.getByRole("button", { name: /set default codex/i }));

    expect(onChangeDefault).toHaveBeenCalledWith("codex");
  });

  it("allows a saved API connection to be selected as the shared default route", async () => {
    const onChangeAiDefaultRoute = vi.fn().mockResolvedValue(undefined);
    renderAi({ aiDefaultRoute: null, onChangeAiDefaultRoute });

    fireEvent.change(screen.getByRole("combobox"), { target: { value: "byok:anthropic" } });

    expect(onChangeAiDefaultRoute).toHaveBeenCalledWith({ kind: "byok", provider: "anthropic" });
    await waitFor(() => expect(screen.getByRole("combobox")).not.toBeDisabled());
  });

  it("explains automatic compatibility selection without claiming a missing Agent is available", () => {
    renderAi({ aiDefaultRoute: null, agentDefault: "claude", onChangeAiDefaultRoute: vi.fn() });
    expect(screen.getByRole("combobox")).toHaveValue("");
    expect(screen.getByText(/Uses the existing default, Claude Code/)).toHaveTextContent(/availability is checked/i);
    expect(screen.getByRole("option", { name: /Automatic \(existing configuration\)/ })).toBeInTheDocument();
  });

  it("explains the unique enabled provider as a candidate that still needs runtime validation", () => {
    renderAi({ aiDefaultRoute: null, onChangeAiDefaultRoute: vi.fn() });
    expect(screen.getByRole("combobox")).toHaveValue("");
    expect(screen.getByText(/Anthropic is the only enabled connection/)).toHaveTextContent(/Credentials and availability are checked/i);
  });

  it("keeps an unavailable explicit default and explains that it will not fall back", () => {
    renderAi({ providers: [], aiDefaultRoute: { kind: "byok", provider: "google" }, onChangeAiDefaultRoute: vi.fn() });
    expect(screen.getByRole("combobox")).toHaveValue("byok:google");
    expect(screen.getByText(/an unavailable default is not replaced automatically/i)).toBeInTheDocument();
  });

  it("opens CLI details on the configured default agent instead of the first detected agent", () => {
    renderAi({
      agentDefault: "claude",
      agents: [
        agents[0],
        { ...agents[1], state: "installed", isDefault: true },
      ],
    });

    expect(screen.getByRole("button", { name: /Claude Code/i })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByText("claude")).toBeInTheDocument();
  });
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => { resolve = resolvePromise; reject = rejectPromise; });
  return { promise, resolve, reject };
}

function openByok() {
  fireEvent.click(screen.getByRole("button", { name: /byok/i }));
}
function selectProvider(name: string) {
  fireEvent.click(screen.getByRole("button", { name: new RegExp(`^${name}`, "i") }));
}

describe("AiSettings provider draft isolation", () => {
  it("clears saved-key feedback when another settings panel removes the credential", async () => {
    const options = {
      agents, providers, agentDefault: null, contextWindow: 32_000,
      onRefreshAgents: vi.fn(), onChangeDefault: vi.fn(), onSaveProvider: vi.fn(),
      onSaveSecret: vi.fn().mockResolvedValue(undefined), onDeleteSecret: vi.fn(),
    };
    const { rerender } = render(<AiSettings {...options} />);
    openByok();
    fireEvent.change(screen.getByLabelText(/API key/i), { target: { value: "submitted-key" } });
    fireEvent.click(screen.getByRole("button", { name: /save key/i }));
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent(/Key saved/i));

    rerender(<AiSettings {...options} providers={[{ ...providers[0], hasSecret: false, secretMask: null }]} />);

    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /delete key/i })).not.toBeInTheDocument();
  });

  it.each(["removed", "rebound"])("does not report a completed key write as current after the credential was %s", async (change) => {
    const request = deferred<void>();
    const options = {
      agents, providers, agentDefault: null, contextWindow: 32_000,
      onRefreshAgents: vi.fn(), onChangeDefault: vi.fn(), onSaveProvider: vi.fn(),
      onSaveSecret: vi.fn().mockReturnValue(request.promise), onDeleteSecret: vi.fn(),
    };
    const { rerender } = render(<AiSettings {...options} />);
    openByok();
    fireEvent.change(screen.getByLabelText(/API key/i), { target: { value: "submitted-key" } });
    fireEvent.click(screen.getByRole("button", { name: /save key/i }));
    const current = change === "removed"
      ? { ...providers[0], hasSecret: false, secretMask: null }
      : { ...providers[0], credentialBinding: { ...providers[0].credentialBinding!, revision: 2 } };
    rerender(<AiSettings {...options} providers={[current]} />);
    await act(async () => request.resolve());

    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.getByLabelText(/API key/i)).toHaveValue("");
  });

  it("does not report a completed deletion as current when another key is already stored", async () => {
    const request = deferred<void>();
    const options = {
      agents, providers, agentDefault: null, contextWindow: 32_000,
      onRefreshAgents: vi.fn(), onChangeDefault: vi.fn(), onSaveProvider: vi.fn(),
      onSaveSecret: vi.fn(), onDeleteSecret: vi.fn().mockReturnValue(request.promise),
    };
    const { rerender } = render(<AiSettings {...options} />);
    openByok();
    fireEvent.click(screen.getByRole("button", { name: /delete key/i }));
    rerender(<AiSettings {...options} providers={[{ ...providers[0], secretMask: "****new" }]} />);
    await act(async () => request.resolve());

    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /delete key/i })).toBeInTheDocument();
  });

  it("clears stale errors and their retry action after external provider changes", async () => {
    const options = {
      agents, providers, agentDefault: null, contextWindow: 32_000,
      onRefreshAgents: vi.fn(), onChangeDefault: vi.fn(), onSaveProvider: vi.fn(),
      onSaveSecret: vi.fn(), onDeleteSecret: vi.fn(),
      onTestProvider: vi.fn().mockRejectedValue(new Error("Old connection failure")),
    };
    const { rerender } = render(<AiSettings {...options} />);
    openByok();
    fireEvent.click(screen.getByRole("button", { name: /test provider/i }));
    await waitFor(() => expect(screen.getByRole("status")).toBeInTheDocument());
    rerender(<AiSettings {...options} providers={[{ ...providers[0], config: { ...providers[0].config, model: "updated-model" } }]} />);

    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^retry$/i })).not.toBeInTheDocument();
  });

  it("retains each provider draft and secret across repeated selections and tab switches", () => {
    const onDirtyChange = vi.fn();
    renderAi({ onDirtyChange });
    openByok();
    fireEvent.change(screen.getByLabelText(/^model$/i), { target: { value: "claude-draft" } });
    fireEvent.change(screen.getByLabelText(/API key/i), { target: { value: "anthropic-draft-key" } });
    selectProvider("Anthropic");
    expect(screen.getByLabelText(/^model$/i)).toHaveValue("claude-draft");
    expect(screen.getByLabelText(/API key/i)).toHaveValue("anthropic-draft-key");
    selectProvider("OpenAI");
    fireEvent.change(screen.getByLabelText(/^model$/i), { target: { value: "openai-draft" } });
    expect(screen.getByLabelText(/API key/i)).toHaveValue("");
    fireEvent.click(screen.getByRole("button", { name: /local cli/i }));
    openByok();
    expect(screen.getByLabelText(/^model$/i)).toHaveValue("openai-draft");
    selectProvider("Anthropic");
    expect(screen.getByLabelText(/^model$/i)).toHaveValue("claude-draft");
    expect(screen.getByLabelText(/API key/i)).toHaveValue("anthropic-draft-key");
    expect(onDirtyChange).toHaveBeenLastCalledWith(true);
  });

  it("keeps a late connection result on the provider that was tested", async () => {
    const request = deferred<{ ok: boolean; message: string }>();
    const onTestProvider = vi.fn().mockReturnValue(request.promise);
    renderAi({ onTestProvider });
    openByok();
    fireEvent.click(screen.getByRole("button", { name: /test provider/i }));
    selectProvider("OpenAI");
    await act(async () => request.resolve({ ok: true, message: "Anthropic connection confirmed" }));
    expect(screen.queryByText(/Anthropic connection confirmed/)).not.toBeInTheDocument();
    selectProvider("Anthropic");
    expect(screen.getByRole("status")).toHaveTextContent(/Anthropic connection confirmed/);
    expect(onTestProvider).toHaveBeenCalledWith(expect.objectContaining({ provider: "anthropic", contextWindow: 100_000 }));
  });

  it("suppresses stale test results after editing the tested configuration", async () => {
    const request = deferred<{ ok: boolean; message: string }>();
    renderAi({ onTestProvider: vi.fn().mockReturnValue(request.promise) });
    openByok();
    fireEvent.click(screen.getByRole("button", { name: /test provider/i }));
    fireEvent.change(screen.getByLabelText(/^model$/i), { target: { value: "new-model" } });
    await act(async () => request.resolve({ ok: true, message: "Old model connected" }));
    expect(screen.queryByText(/Old model connected/)).not.toBeInTheDocument();
    expect(screen.getByLabelText(/^model$/i)).toHaveValue("new-model");
  });

  it("discards a pending test result if saved credentials change from another settings panel", async () => {
    const request = deferred<{ ok: boolean; message: string }>();
    const onTestProvider = vi.fn().mockReturnValue(request.promise);
    const options = {
      agents, providers, agentDefault: null, contextWindow: 32_000,
      onRefreshAgents: vi.fn(), onChangeDefault: vi.fn(), onSaveProvider: vi.fn(),
      onSaveSecret: vi.fn(), onDeleteSecret: vi.fn(), onTestProvider,
    };
    const { rerender } = render(<AiSettings {...options} />);
    openByok();
    fireEvent.click(screen.getByRole("button", { name: /test provider/i }));
    rerender(<AiSettings {...options} providers={[{ ...providers[0], hasSecret: false, secretMask: null }]} />);
    await act(async () => request.resolve({ ok: true, message: "Removed credential connected" }));
    expect(screen.queryByText(/Removed credential connected/)).not.toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("preserves edits made during saving and saves each provider's own context window", async () => {
    const request = deferred<void>();
    const onSaveProvider = vi.fn().mockReturnValueOnce(request.promise).mockResolvedValue(undefined);
    const onDirtyChange = vi.fn();
    renderAi({ onSaveProvider, onDirtyChange });
    openByok();
    fireEvent.change(screen.getByLabelText(/^model$/i), { target: { value: "submitted-model" } });
    fireEvent.change(screen.getByRole("spinbutton"), { target: { value: "48000" } });
    fireEvent.click(screen.getByRole("button", { name: /save provider/i }));
    fireEvent.click(screen.getByRole("button", { name: /save provider/i }));
    expect(onSaveProvider).toHaveBeenCalledTimes(1);
    expect(onSaveProvider).toHaveBeenCalledWith(expect.objectContaining({ model: "submitted-model", contextWindow: 48000 }));
    fireEvent.change(screen.getByLabelText(/^model$/i), { target: { value: "newer-model" } });
    await act(async () => request.resolve());
    expect(screen.getByLabelText(/^model$/i)).toHaveValue("newer-model");
    expect(screen.getByRole("button", { name: /test provider/i })).toBeDisabled();
    expect(onDirtyChange).toHaveBeenLastCalledWith(true);
    fireEvent.click(screen.getByRole("button", { name: /save provider/i }));
    await waitFor(() => expect(onDirtyChange).toHaveBeenLastCalledWith(false));
    expect(onSaveProvider).toHaveBeenLastCalledWith(expect.objectContaining({ model: "newer-model", contextWindow: 48000 }));
  });

  it("does not clear another provider's secret when a key write completes", async () => {
    const request = deferred<void>();
    const onSaveSecret = vi.fn().mockReturnValue(request.promise);
    renderAi({ onSaveSecret });
    openByok();
    fireEvent.change(screen.getByLabelText(/API key/i), { target: { value: "anthropic-key" } });
    fireEvent.click(screen.getByRole("button", { name: /save key/i }));
    selectProvider("OpenAI");
    fireEvent.change(screen.getByLabelText(/API key/i), { target: { value: "openai-key" } });
    await act(async () => request.resolve());
    expect(screen.getByLabelText(/API key/i)).toHaveValue("openai-key");
    selectProvider("Anthropic");
    expect(screen.getByLabelText(/API key/i)).toHaveValue("");
    expect(onSaveSecret).toHaveBeenCalledWith("anthropic", "anthropic-key");
  });

  it.each(["success", "failure"])("preserves a newer key draft after a stale %s", async (outcome) => {
    const request = deferred<void>();
    renderAi({ onSaveSecret: vi.fn().mockReturnValue(request.promise) });
    openByok();
    fireEvent.change(screen.getByLabelText(/API key/i), { target: { value: "submitted-key" } });
    fireEvent.click(screen.getByRole("button", { name: /save key/i }));
    fireEvent.change(screen.getByLabelText(/API key/i), { target: { value: "newer-key" } });
    await act(async () => {
      if (outcome === "success") request.resolve();
      else request.reject(new Error("Old request failed"));
    });
    expect(screen.getByLabelText(/API key/i)).toHaveValue("newer-key");
    expect(screen.queryByText(/Old request failed/)).not.toBeInTheDocument();
  });
});
