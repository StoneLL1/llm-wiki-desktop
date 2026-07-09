import { fireEvent, render, screen } from "@testing-library/react";
import { type ComponentProps } from "react";
import { describe, expect, it, vi } from "vitest";
import "../../i18n";
import type { AgentInfo } from "../../types/agent";
import { AiSettings } from "./AiSettings";

const agents: AgentInfo[] = [];

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
    renderAiProvider({ onSaveSecret: saveSecret });
    fireEvent.click(screen.getByRole("button", { name: /byok/i }));

    const input = screen.getByLabelText(/API key/i) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "sk-secret-value" } });
    fireEvent.click(screen.getByRole("button", { name: /save key/i }));

    await screen.findByText(/saved/i);
    expect(saveSecret).toHaveBeenCalledWith("anthropic", "sk-secret-value");
    expect(input.value).toBe("");
    expect(screen.queryByText("sk-secret-value")).not.toBeInTheDocument();
  });

  it("syncs the selected provider editor when provider config loads after initial render", () => {
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
              baseUrl: "https://loaded.anthropic.test",
              contextWindow: 100_000,
              enabled: true,
            },
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
    expect(screen.getByLabelText(/base url/i)).toHaveValue("https://loaded.anthropic.test");
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
      providers: [
        {
          config: {
            provider: "anthropic",
            model: "claude-test",
            baseUrl: "https://api.anthropic.com",
            contextWindow: 100_000,
            enabled: true,
          },
          hasSecret: true,
          secretMask: "****test",
        },
      ],
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
});
