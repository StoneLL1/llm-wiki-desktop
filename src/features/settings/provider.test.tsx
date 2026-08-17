import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { type ComponentProps } from "react";
import { describe, expect, it, vi } from "vitest";
import "../../i18n";
import type { AgentInfo } from "../../types/agent";
import { AiSettings } from "./AiSettings";

const agents: AgentInfo[] = [];
const anthropicStatus = {
  config: {
    provider: "anthropic" as const,
    model: "claude-test",
    baseUrl: "https://api.anthropic.com",
    contextWindow: 100_000,
    enabled: true,
  },
  credentialBinding: {
    configId: "2d40f995-0dad-4d50-9a91-737664542dc0",
    providerKind: "anthropic" as const,
    canonicalOrigin: "https://api.anthropic.com",
    credentialAccountId: "provider.binding.v1.project.anthropic.config.origin.1",
    approvedAt: "2026-08-18T00:00:00Z",
    revision: 1,
  },
  hasSecret: true,
  secretMask: "****test",
};

function renderAiProvider(overrides: Partial<ComponentProps<typeof AiSettings>> = {}) {
  return render(
    <AiSettings
      agents={agents}
      providers={[]}
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

describe("AiSettings BYOK providers", () => {
  it("clears the secret field after saving and never echoes secret material", async () => {
    const saveSecret = vi.fn().mockResolvedValue(undefined);
    renderAiProvider({ providers: [anthropicStatus], onSaveSecret: saveSecret });
    fireEvent.click(screen.getByRole("button", { name: /byok/i }));

    const input = screen.getByLabelText(/API key/i) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "sk-secret-value" } });
    fireEvent.click(screen.getByRole("button", { name: /save key/i }));

    await screen.findByText(/saved/i);
    expect(saveSecret).toHaveBeenCalledWith("anthropic", "sk-secret-value");
    expect(input.value).toBe("");
    expect(screen.queryByText("sk-secret-value")).not.toBeInTheDocument();
  });

  it("syncs the model but restores the reviewed official origin when legacy config loads", () => {
    const { rerender } = renderAiProvider();
    fireEvent.click(screen.getByRole("button", { name: /byok/i }));

    expect(screen.getByLabelText(/model/i)).toHaveValue("claude-sonnet-4-6");

    rerender(
      <AiSettings
        agents={agents}
        providers={[
          {
            config: {
              provider: "anthropic",
              model: "claude-loaded",
              baseUrl: "https://attacker.example",
              contextWindow: 100_000,
              enabled: true,
            },
            credentialBinding: anthropicStatus.credentialBinding,
            hasSecret: true,
            secretMask: "****test",
          },
        ]}
        agentDefault={null}
        contextWindow={32_000}
        onRefreshAgents={vi.fn()}
        onChangeDefault={vi.fn()}
        onSaveProvider={vi.fn()}
        onSaveSecret={vi.fn()}
        onDeleteSecret={vi.fn()}
        onTestProvider={vi.fn().mockResolvedValue({ ok: true, message: "Connected" })}
      />,
    );

    expect(screen.getByLabelText(/model/i)).toHaveValue("claude-loaded");
    expect(screen.getByLabelText(/base url/i)).toHaveValue("https://api.anthropic.com");
  });

  it("keeps local Ollama status neutral until a connection test is run", () => {
    renderAiProvider();
    fireEvent.click(screen.getByRole("button", { name: /byok/i }));

    expect(screen.getByRole("button", { name: /Ollama/i })).toHaveTextContent(/local service/i);
    expect(screen.getByRole("button", { name: /Ollama/i })).not.toHaveTextContent(/service down/i);
  });

  it("tests the selected provider with its saved endpoint and model", async () => {
    const testProvider = vi.fn().mockResolvedValue({ ok: true, message: "Connected" });
    renderAiProvider({
      providers: [anthropicStatus],
      onTestProvider: testProvider,
    });

    fireEvent.click(screen.getByRole("button", { name: /byok/i }));
    fireEvent.click(screen.getByText("claude-test").closest("button")!);
    fireEvent.click(screen.getByRole("button", { name: /test provider/i }));

    await screen.findByRole("status");
    expect(testProvider).toHaveBeenCalledWith(expect.objectContaining({
      provider: "anthropic",
      model: "claude-test",
      baseUrl: "https://api.anthropic.com",
    }));
  });

  it("requires explicit approval of the exact canonical custom origin before saving a key", async () => {
    const saveSecret = vi.fn().mockResolvedValue(undefined);
    renderAiProvider({
      providers: [{
        config: {
          provider: "custom",
          model: "custom-model",
          baseUrl: "https://custom.example:8443/v1",
          contextWindow: 32_000,
          enabled: true,
        },
        credentialBinding: {
          configId: "e07c3067-5056-46da-91a0-b51870e739e6",
          providerKind: "custom",
          canonicalOrigin: "https://custom.example:8443",
          credentialAccountId: "provider.binding.v1.project.custom.config.origin.1",
          approvedAt: "forged-project-value",
          revision: 1,
        },
        hasSecret: false,
        secretMask: null,
      }],
      onSaveSecret: saveSecret,
    });
    fireEvent.click(screen.getByRole("button", { name: /byok/i }));
    fireEvent.click(screen.getByRole("button", { name: /custom/i }));
    fireEvent.change(screen.getByLabelText(/API key/i), {
      target: { value: "custom-secret" },
    });
    fireEvent.click(screen.getByRole("button", { name: /save key/i }));

    const approval = await screen.findByRole("alertdialog");
    expect(approval).toHaveTextContent("https://custom.example:8443");
    expect(saveSecret).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: /authorize and save key/i }));
    await waitFor(() => expect(saveSecret).toHaveBeenCalledWith("custom", "custom-secret"));
  });
});
