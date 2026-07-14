import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import type { AgentAssistancePolicy } from "../../types/importV2Agent";
import type { ImportItem } from "../../types/importV2";
import { ImportAgentControls } from "./ImportAgentControls";

const policy: AgentAssistancePolicy = {
  autoLocalOnHardFailure: false,
  autoLocalOnQualityWarning: false,
  autoByok: false,
  maxAttemptsPerItem: 1,
};

const item: ImportItem = {
  itemId: "item-failed",
  input: { kind: "file", displayName: "研究.md", locator: "D:/资料/研究.md", normalizedLocator: null },
  status: "failed",
  selected: false,
  taskId: null,
  progress: null,
  attempts: [],
  preview: null,
  issue: {
    code: "IMPORT_FAILED",
    message: "Deterministic route failed",
    stage: "extract",
    retryable: true,
    userActionRequired: true,
    recoveryActions: ["invoke_agent"],
    availableActions: ["invoke_local_agent", "request_byok"],
  },
};

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("ImportAgentControls", () => {
  it("renders backend policy without implying automatic BYOK and only commits a successful policy update", async () => {
    const onPolicyChange = vi.fn().mockResolvedValue(policy);
    render(
      <ImportAgentControls
        item={item}
        policy={policy}
        localAgentKind="codex"
        localAgentAvailable
        onPolicyChange={onPolicyChange}
        onInvokeLocalAgent={vi.fn()}
        onRequestByok={vi.fn()}
        onCompareCandidate={vi.fn()}
        onDiscardCandidate={vi.fn()}
      />,
    );

    expect(screen.getByText(/BYOK requires explicit approval/i)).toBeInTheDocument();
    const toggle = screen.getByRole("checkbox", { name: "Automatically invoke local Agent on hard failure" });
    fireEvent.click(toggle);
    await waitFor(() => expect(onPolicyChange).toHaveBeenCalledWith({ ...policy, autoLocalOnHardFailure: true }));
  });

  it("offers local assistance and BYOK only as explicit actions for a failed item", async () => {
    const onInvokeLocalAgent = vi.fn().mockResolvedValue(undefined);
    const onRequestByok = vi.fn();
    render(
      <ImportAgentControls
        item={item}
        policy={policy}
        localAgentKind="codex"
        localAgentAvailable
        onPolicyChange={vi.fn()}
        onInvokeLocalAgent={onInvokeLocalAgent}
        onRequestByok={onRequestByok}
        onCompareCandidate={vi.fn()}
        onDiscardCandidate={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /run local agent/i }));
    await waitFor(() => expect(onInvokeLocalAgent).toHaveBeenCalledWith("item-failed", "codex"));
    fireEvent.click(screen.getByRole("button", { name: /review BYOK scope/i }));
    expect(onRequestByok).toHaveBeenCalledWith("item-failed");
  });

  it("does not open a dead BYOK dialog when no provider is configured", () => {
    render(
      <ImportAgentControls
        item={item}
        policy={policy}
        localAgentKind={null}
        localAgentAvailable={false}
        byokAvailable={false}
        onPolicyChange={vi.fn()}
        onInvokeLocalAgent={vi.fn()}
        onRequestByok={vi.fn()}
        onCompareCandidate={vi.fn()}
        onDiscardCandidate={vi.fn()}
      />,
    );

    expect(screen.queryByRole("button", { name: /review BYOK scope/i })).not.toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent(/no configured BYOK provider/i);
  });
});
