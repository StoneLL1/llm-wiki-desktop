import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import "../../i18n";
import { LlmProviderSettings } from "./LlmProviderSettings";

describe("LlmProviderSettings", () => {
  it("clears the secret field after saving and never echoes secret material", async () => {
    const saveSecret = vi.fn().mockResolvedValue(undefined);
    render(<LlmProviderSettings providers={[]} onSaveProvider={vi.fn()} onSaveSecret={saveSecret} />);
    const input = screen.getByLabelText(/API key/i) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "sk-secret-value" } });
    fireEvent.click(screen.getByRole("button", { name: /save key/i }));
    await screen.findByText(/saved/i);
    expect(saveSecret).toHaveBeenCalled();
    expect(input.value).toBe("");
    expect(screen.queryByText("sk-secret-value")).not.toBeInTheDocument();
  });

  it("tests the selected provider with its saved endpoint and model", async () => {
    const testProvider = vi.fn().mockResolvedValue({ ok: true, message: "Connected" });
    render(<LlmProviderSettings providers={[{
      config: { provider: "anthropic", model: "claude-test", baseUrl: "https://api.anthropic.com", contextWindow: 100_000, enabled: true },
      hasSecret: true,
      secretMask: "••••test",
    }]} onSaveProvider={vi.fn()} onSaveSecret={vi.fn()} onTestProvider={testProvider} />);
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
