import { fireEvent, render, screen } from "@testing-library/react";
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
    expect(screen.getByText(/anthropic/i)).toBeInTheDocument();
  });

  it("sets an installed Agent CLI as the default route", () => {
    const onChangeDefault = vi.fn();
    renderAi({ onChangeDefault });

    fireEvent.click(screen.getByRole("button", { name: /set default codex/i }));

    expect(onChangeDefault).toHaveBeenCalledWith("codex");
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
