import { Bot, GitCompareArrows, KeyRound, LoaderCircle, Trash2 } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import type { AgentAssistancePolicy } from "../../types/importV2Agent";
import type { AgentKind } from "../../types/agent";
import type { ImportItem } from "../../types/importV2";

export interface ImportAgentControlsProps {
  item: ImportItem;
  policy: AgentAssistancePolicy;
  localAgentKind: AgentKind | null;
  localAgentAvailable?: boolean;
  onPolicyChange: (policy: AgentAssistancePolicy) => Promise<AgentAssistancePolicy | void>;
  onInvokeLocalAgent: (itemId: string, agentKind: AgentKind) => Promise<void> | void;
  onRequestByok: (itemId: string) => void;
  onCompareCandidate: (itemId: string) => void;
  onDiscardCandidate: (itemId: string) => Promise<void> | void;
}

export function ImportAgentControls({
  item,
  policy,
  localAgentKind,
  localAgentAvailable = false,
  onPolicyChange,
  onInvokeLocalAgent,
  onRequestByok,
  onCompareCandidate,
  onDiscardCandidate,
}: ImportAgentControlsProps) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState<string | null>(null);
  const [policyError, setPolicyError] = useState(false);
  const available = new Set(item.issue?.availableActions ?? []);
  const canRunLocal = available.has("invoke_local_agent") && localAgentAvailable && localAgentKind !== null;
  const canRequestByok = available.has("request_byok");
  const canCompare = available.has("compare_candidate");
  const canDiscard = available.has("discard_candidate");

  async function updatePolicy(key: "autoLocalOnHardFailure" | "autoLocalOnQualityWarning", value: boolean) {
    setBusy(`policy:${key}`);
    setPolicyError(false);
    try {
      await onPolicyChange({ ...policy, [key]: value });
    } catch {
      setPolicyError(true);
    } finally {
      setBusy(null);
    }
  }

  async function runAction(key: string, action: () => Promise<void> | void) {
    setBusy(key);
    try {
      await action();
    } finally {
      setBusy(null);
    }
  }

  return (
    <section className="border-t border-[var(--border)] px-4 py-3" aria-labelledby="import-agent-controls-title">
      <h3 id="import-agent-controls-title" className="import-v2-inspector-heading">{t("importV2.agent.title")}</h3>
      <div className="space-y-2 text-[12px]">
        <p className="m-0 text-[11px] text-[var(--text-muted)]">{t("importV2.agent.policy")}</p>
        <label className="flex items-start gap-2">
          <input
            type="checkbox"
            aria-label={t("importV2.agent.autoLocal")}
            checked={policy.autoLocalOnHardFailure}
            disabled={busy !== null}
            onChange={(event) => void updatePolicy("autoLocalOnHardFailure", event.target.checked)}
          />
          <span>{t("importV2.agent.autoLocal")}</span>
        </label>
        <label className="flex items-start gap-2">
          <input
            type="checkbox"
            aria-label={t("importV2.agent.autoQuality")}
            checked={policy.autoLocalOnQualityWarning}
            disabled={busy !== null}
            onChange={(event) => void updatePolicy("autoLocalOnQualityWarning", event.target.checked)}
          />
          <span>{t("importV2.agent.autoQuality")}</span>
        </label>
        <p className="m-0 rounded-[var(--radius-md)] border border-[var(--border-subtle)] px-2 py-1.5 text-[11px] text-[var(--text-muted)]" role="note">
          {t("importV2.agent.byokExplicit")}
        </p>
        {policyError ? <p className="m-0 text-[11px] text-[var(--danger-text)]" role="alert">{t("importV2.agent.policyError")}</p> : null}
      </div>

      {item.issue ? (
        <div className="mt-3 flex flex-wrap gap-1.5">
          {canRunLocal ? (
            <button type="button" className="btn btn--sm" disabled={busy !== null} onClick={() => void runAction("local", () => onInvokeLocalAgent(item.itemId, localAgentKind!))}>
              {busy === "local" ? <LoaderCircle size={13} className="mr-1 inline animate-spin" aria-hidden="true" /> : <Bot size={13} className="mr-1 inline" aria-hidden="true" />}
              {t("importV2.agent.runLocal")}
            </button>
          ) : null}
          {canRequestByok ? (
            <button type="button" className="btn btn--sm" disabled={busy !== null} onClick={() => onRequestByok(item.itemId)}>
              <KeyRound size={13} className="mr-1 inline" aria-hidden="true" />{t("importV2.agent.reviewByok")}
            </button>
          ) : null}
          {canCompare ? (
            <button type="button" className="btn btn--sm" disabled={busy !== null} onClick={() => onCompareCandidate(item.itemId)}>
              <GitCompareArrows size={13} className="mr-1 inline" aria-hidden="true" />{t("importV2.agent.compare")}
            </button>
          ) : null}
          {canDiscard ? (
            <button type="button" className="btn btn--sm btn--ghost" disabled={busy !== null} onClick={() => void runAction("discard", () => onDiscardCandidate(item.itemId))}>
              <Trash2 size={13} className="mr-1 inline" aria-hidden="true" />{t("importV2.agent.discard")}
            </button>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}
