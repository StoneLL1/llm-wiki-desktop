import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import type { AgentSendScope } from "../../types/importV2Agent";
import { ImportByokApprovalDialog } from "./ImportByokApprovalDialog";

const scope: AgentSendScope = {
  approvalId: "approval-1",
  itemId: "item-1",
  provider: "open_ai",
  model: "gpt-5",
  destination: "api.openai.com/v1/responses",
  publicMetadata: ["title: Research"],
  files: [
    { relativePath: "研究.md", sha256: "a".repeat(64), sizeBytes: 2048, estimatedTokens: 512, redactions: ["email"] },
  ],
  estimatedInputTokens: 512,
  estimatedCostMicros: 1200,
  requiresDuplicateChargeAcknowledgement: true,
  scopeSha256: "b".repeat(64),
  expiresAt: "2099-01-01T00:00:00.000Z",
};

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("ImportByokApprovalDialog", () => {
  it("shows the exact bounded scope and never asks for or renders a secret", () => {
    render(<ImportByokApprovalDialog open scope={scope} onCancel={vi.fn()} onConfirm={vi.fn()} />);

    expect(screen.getByText("api.openai.com/v1/responses")).toBeInTheDocument();
    expect(screen.getByText("研究.md")).toBeInTheDocument();
    expect(screen.getByText(/2,048 bytes/i)).toBeInTheDocument();
    expect(screen.getByText(/email/i)).toBeInTheDocument();
    expect(screen.getByText(/scope expires/i)).toBeInTheDocument();
    expect(screen.queryByLabelText(/API key|secret/i)).not.toBeInTheDocument();
  });

  it("requires duplicate-charge acknowledgement and confirms the current scope only", async () => {
    const onConfirm = vi.fn().mockResolvedValue(undefined);
    render(<ImportByokApprovalDialog open scope={scope} onCancel={vi.fn()} onConfirm={onConfirm} />);

    const confirm = screen.getByRole("button", { name: /approve BYOK assistance/i });
    expect(confirm).toBeDisabled();
    fireEvent.click(screen.getByRole("checkbox", { name: /duplicate charge/i }));
    fireEvent.click(confirm);
    await waitFor(() => expect(onConfirm).toHaveBeenCalledWith(scope, true));
  });

  it("forces a fresh preview when the scope is expired", () => {
    render(
      <ImportByokApprovalDialog
        open
        scope={{ ...scope, expiresAt: "2000-01-01T00:00:00.000Z" }}
        onCancel={vi.fn()}
        onConfirm={vi.fn()}
      />,
    );

    expect(screen.getByRole("alert")).toHaveTextContent(/expired.*scope again/i);
    expect(screen.getByRole("button", { name: /approve BYOK assistance/i })).toBeDisabled();
  });
});
