import { useEffect, useMemo, useState } from "react";

import type { AiCapabilitiesWorkflow } from "../../hooks/useAiCapabilities";
import { useImportStore } from "../../stores/importStore";
import type { ImportItem } from "../../types/importV2";
import { balancedAgentAssistancePolicy } from "../../types/importV2Agent";
import type { AgentAssistancePolicy } from "../../types/importV2Agent";
import type { AgentCandidateView as AgentCandidateViewType, AgentSendScope } from "../../types/importV2Agent";
import type { ConnectorSessionRef, ImportCapabilityRequirement, ImportFrontendReadiness } from "../../types/importV2Presentation";
import type { MigrationConfirmation, LegacyInventory, MigrationPlan, MigrationReport } from "../../types/importV2Migration";
import type { WebAuthState } from "../../types/importV2Web";
import type { LlmProviderKind } from "../../types/llm";
import { ImportAgentControls } from "./ImportAgentControls";
import { ImportByokApprovalDialog } from "./ImportByokApprovalDialog";
import { ImportCandidateDiffDialog, type ImportCandidateDiffIntent } from "./ImportCandidateDiffDialog";
import { ImportCapabilityDialog } from "./ImportCapabilityDialog";
import { ImportLoginDialog } from "./ImportLoginDialog";
import { ImportMarkdownPreviewDialog } from "./ImportMarkdownPreviewDialog";
import { ImportMigrationDialog, type ImportMigrationUiStatus } from "./ImportMigrationDialog";
import { ImportPrivateTargetDialog } from "./ImportPrivateTargetDialog";
import type { ImportWorkflow } from "./useImportWorkflow";

export interface ImportV2DialogsProps {
  workflow: ImportWorkflow;
  capabilities: AiCapabilitiesWorkflow;
  readiness: ImportFrontendReadiness | null;
  selectedItem: ImportItem | null;
  privateItem: ImportItem | null;
  migrationOpen: boolean;
  onCloseMigration: () => void;
  candidateView: AgentCandidateViewType | null;
  onCloseCandidate: () => void;
  onCandidateIntent: (intent: ImportCandidateDiffIntent) => void;
  onClosePrivate: () => void;
  onCompareCandidate: (itemId: string) => void;
  onDiscardCandidate: (itemId: string) => void;
}

function hostFor(locator: string): string {
  try {
    return new URL(locator).host || locator;
  } catch {
    return locator;
  }
}

function migrationStatus(readiness: ImportFrontendReadiness | null): ImportMigrationUiStatus {
  if (!readiness) return "not_scanned";
  if (readiness.active && readiness.migrationStatus === "applied") return "activated";
  if (!readiness.active && readiness.migrationStatus === "applied") return "not_activated";
  return readiness.migrationStatus;
}

function providerFor(capabilities: AiCapabilitiesWorkflow) {
  return capabilities.providers.find((provider) => provider.config.enabled && (provider.hasSecret || provider.config.provider === "ollama"))?.config.provider ?? null;
}

export function ImportV2Dialogs({ workflow, capabilities, readiness, selectedItem, privateItem, migrationOpen, onCloseMigration, candidateView, onCloseCandidate, onCandidateIntent, onClosePrivate, onCompareCandidate, onDiscardCandidate }: ImportV2DialogsProps) {
  const session = useImportStore((state) => state.session);
  const previewItemId = useImportStore((state) => state.previewItemId);
  const byokItemId = useImportStore((state) => state.byokItemId);
  const capabilityItemId = useImportStore((state) => state.capabilityItemId);
  const loginItemId = useImportStore((state) => state.loginItemId);
  const closePreview = useImportStore((state) => state.closePreview);
  const closeByok = useImportStore((state) => state.closeByok);
  const closeCapability = useImportStore((state) => state.closeCapability);
  const closeLogin = useImportStore((state) => state.closeLogin);

  const previewItem = session?.items.find((item) => item.itemId === previewItemId) ?? null;
  const previewIdentity = session && previewItem ? { sessionId: session.sessionId, itemId: previewItem.itemId, candidateId: null } : null;
  const capabilityItem = session?.items.find((item) => item.itemId === capabilityItemId) ?? null;
  const loginItem = session?.items.find((item) => item.itemId === loginItemId) ?? null;

  const [scope, setScope] = useState<AgentSendScope | null>(null);
  const [capability, setCapability] = useState<ImportCapabilityRequirement | null>(null);
  const [connector, setConnector] = useState<ConnectorSessionRef | null>(null);
  const [policy, setPolicy] = useState<AgentAssistancePolicy>(balancedAgentAssistancePolicy());
  const [migrationState, setMigrationState] = useState<ImportMigrationUiStatus>(() => migrationStatus(readiness));
  const [inventory, setInventory] = useState<LegacyInventory | null>(null);
  const [plan, setPlan] = useState<MigrationPlan | null>(null);
  const [report, setReport] = useState<MigrationReport | null>(null);
  const [confirmation] = useState<MigrationConfirmation | null>(null);

  const localAgent = useMemo(() => capabilities.agents.find((agent) => agent.state === "installed" && agent.isDefault) ?? capabilities.agents.find((agent) => agent.state === "installed") ?? null, [capabilities.agents]);
  const provider = useMemo(() => providerFor(capabilities), [capabilities]);

  useEffect(() => {
    if (!selectedItem || workflow.bootstrapState !== "ready") return;
    let current = true;
    void workflow.getAgentPolicy().then((next) => {
      if (current && next) setPolicy(next);
    }).catch(() => undefined);
    return () => { current = false; };
  }, [selectedItem?.itemId, workflow.bootstrapState, workflow.getAgentPolicy]);

  useEffect(() => {
    if (!byokItemId || !provider) {
      setScope(null);
      return;
    }
    let current = true;
    setScope(null);
    void workflow.previewByokScope(byokItemId, "manual", provider).then((next) => {
      if (current) setScope(next);
    }).catch(() => {
      if (current) setScope(null);
    });
    return () => { current = false; };
  }, [byokItemId, provider, workflow.previewByokScope]);

  useEffect(() => {
    if (!capabilityItemId) {
      setCapability(null);
      return;
    }
    let current = true;
    setCapability(null);
    void workflow.getCapabilityRequirement(capabilityItemId).then((next) => {
      if (current) setCapability(next);
    }).catch(() => {
      if (current) setCapability(null);
    });
    return () => { current = false; };
  }, [capabilityItemId, workflow.getCapabilityRequirement]);

  useEffect(() => {
    if (!loginItemId) setConnector(null);
  }, [loginItemId]);

  useEffect(() => {
    if (!migrationOpen) return;
    setMigrationState(migrationStatus(readiness));
    void workflow.getMigrationStatus().then((snapshot) => {
      if (!snapshot) return;
      setMigrationState(snapshot.status);
      setReport(snapshot.report ?? null);
    }).catch(() => undefined);
  }, [migrationOpen, readiness, workflow.getMigrationStatus]);

  async function scanMigration() {
    setMigrationState("scanning");
    const next = await workflow.scanMigration();
    setInventory(next);
    setPlan(null);
    setReport(null);
    setMigrationState("dry_run_ready");
  }

  async function buildPlan(nextInventory: LegacyInventory) {
    setMigrationState("scanning");
    const nextPlan = await workflow.planMigration(nextInventory);
    setPlan(nextPlan);
    const snapshot = await workflow.getMigrationStatus();
    setReport(snapshot?.report ?? null);
    setMigrationState(snapshot?.status ?? "awaiting_confirmation");
  }

  const loginPlatform = loginItem?.input.kind === "url" ? hostFor(loginItem.input.normalizedLocator ?? loginItem.input.locator) : "connector";
  const loginAuthState: WebAuthState = loginItem?.status === "waiting_login" ? "waiting_login" : connector?.state === "authenticated" ? "authenticated" : "public";

  return (
    <>
      {selectedItem ? <ImportAgentControls
        item={selectedItem}
        policy={policy}
        localAgentKind={localAgent?.kind ?? null}
        localAgentAvailable={localAgent !== null}
        onPolicyChange={async (next) => {
          const saved = await workflow.setAgentPolicy(next, localAgent?.kind ?? null);
          if (!saved) throw new Error("Agent policy was not saved");
          setPolicy(saved);
          return saved;
        }}
        onInvokeLocalAgent={async (itemId, agentKind) => { await workflow.invokeLocalAgent(itemId, "manual", agentKind); }}
        onRequestByok={(itemId) => useImportStore.getState().openByok(itemId)}
        onCompareCandidate={onCompareCandidate}
        onDiscardCandidate={onDiscardCandidate}
      /> : null}

      <ImportMarkdownPreviewDialog open={Boolean(previewIdentity)} identity={previewIdentity} loadContent={workflow.loadPreview} onClose={closePreview} />
      <ImportByokApprovalDialog
        open={Boolean(byokItemId && scope)}
        scope={scope}
        onCancel={closeByok}
        onConfirm={async (nextScope, acknowledge) => {
          await workflow.approveByokAssistance({ itemId: nextScope.itemId, trigger: "manual", provider: nextScope.provider as LlmProviderKind, model: nextScope.model, approvalId: nextScope.approvalId, scopeSha256: nextScope.scopeSha256, acknowledgePossibleDuplicateCharge: acknowledge });
          closeByok();
        }}
      />
      <ImportCapabilityDialog
        open={Boolean(capabilityItemId && capability)}
        requirement={capability}
        onCancel={closeCapability}
        onInstall={async (capabilityId) => {
          if (capabilityItem) await workflow.installCapability(capabilityItem.itemId, capabilityId);
          closeCapability();
        }}
      />
      <ImportLoginDialog
        open={Boolean(loginItem)}
        platform={loginPlatform}
        publicDomain={loginPlatform}
        authState={loginAuthState}
        connectorSession={connector}
        onBeginLogin={() => workflow.beginLogin(loginItem!.itemId, loginPlatform).then((next) => { setConnector(next); return next ?? undefined; })}
        onCheckAgain={(connectorSessionId) => workflow.completeLogin(loginItem!.itemId, connectorSessionId).then((next) => { setConnector(next); return next ?? undefined; })}
        onRevoke={async (connectorSessionId) => { await workflow.revokeLogin(connectorSessionId); setConnector(null); }}
        onCancel={closeLogin}
      />
      <ImportPrivateTargetDialog
        open={Boolean(privateItem)}
        itemId={privateItem?.itemId ?? ""}
        target={privateItem?.input.normalizedLocator ?? privateItem?.input.locator ?? ""}
        addressCategory="private target"
        reason={privateItem?.issue?.message ?? "Authorization required"}
        onAuthorize={async (itemId, target) => { await workflow.authorizePrivateTarget(itemId, target); onClosePrivate(); await workflow.refreshSession(); }}
        onCancel={onClosePrivate}
      />
      <ImportCandidateDiffDialog open={Boolean(candidateView)} view={candidateView} onClose={onCloseCandidate} onAction={onCandidateIntent} />
      <ImportMigrationDialog
        open={migrationOpen}
        status={migrationState}
        inventory={inventory}
        plan={plan}
        report={report}
        confirmation={confirmation}
        checkpoint={null}
        resumable={migrationState === "interrupted" || migrationState === "resumable" || migrationState === "applying"}
        onScan={() => void scanMigration()}
        onPlan={(next) => void buildPlan(next)}
        onApply={async (nextPlan, nextConfirmation) => { setMigrationState("applying"); await workflow.applyMigration(nextPlan, nextConfirmation); }}
        onResume={async (nextPlan, nextConfirmation) => { setMigrationState("applying"); await workflow.resumeMigration(nextPlan, nextConfirmation); }}
        onClose={onCloseMigration}
      />
    </>
  );
}
