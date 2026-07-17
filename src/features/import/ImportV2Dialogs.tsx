import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import type { AiCapabilitiesWorkflow } from "../../hooks/useAiCapabilities";
import { useImportStore } from "../../stores/importStore";
import type { ImportItem } from "../../types/importV2";
import type { AgentCandidateView as AgentCandidateViewType, AgentSendScope } from "../../types/importV2Agent";
import type { ConnectorSessionRef, ImportCapabilityRequirement, ImportFrontendReadiness } from "../../types/importV2Presentation";
import type { MigrationConfirmation, LegacyInventory, MigrationPlan, MigrationReport } from "../../types/importV2Migration";
import type { WebAuthState } from "../../types/importV2Web";
import type { LlmProviderKind } from "../../types/llm";
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
  privateItem: ImportItem | null;
  migrationOpen: boolean;
  onCloseMigration: () => void;
  candidateView: AgentCandidateViewType | null;
  onCloseCandidate: () => void;
  onCandidateIntent: (intent: ImportCandidateDiffIntent) => void;
  onClosePrivate: () => void;
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

export function ImportV2Dialogs({ workflow, capabilities, readiness, privateItem, migrationOpen, onCloseMigration, candidateView, onCloseCandidate, onCandidateIntent, onClosePrivate }: ImportV2DialogsProps) {
  const { t } = useTranslation();
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
  const [migrationState, setMigrationState] = useState<ImportMigrationUiStatus>(() => migrationStatus(readiness));
  const [inventory, setInventory] = useState<LegacyInventory | null>(null);
  const [plan, setPlan] = useState<MigrationPlan | null>(null);
  const [report, setReport] = useState<MigrationReport | null>(null);
  const [confirmation] = useState<MigrationConfirmation | null>(null);

  const provider = useMemo(() => providerFor(capabilities), [capabilities]);

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

  const loginDomain = loginItem?.input.kind === "url" ? hostFor(loginItem.input.normalizedLocator ?? loginItem.input.locator) : "connector";
  const loginPlatform = connectorIdForHost(loginDomain);
  const loginPlatformLabel = t(`importV2.platform.${loginPlatform}`, { defaultValue: loginPlatform });
  const loginAuthState: WebAuthState = loginItem?.status === "waiting_login" ? "waiting_login" : connector?.state === "authenticated" ? "authenticated" : "public";

  return (
    <>
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
        platform={loginPlatformLabel}
        publicDomain={loginDomain}
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
        addressCategory={t("importV2.private.addressCategory")}
        reason={privateItem?.issue?.message ?? t("importV2.private.authorizationRequired")}
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

function connectorIdForHost(host: string): string {
  const normalized = host.toLowerCase();
  if (normalized === "mp.weixin.qq.com") return "wechat";
  if (normalized === "zhihu.com" || normalized.endsWith(".zhihu.com")) return "zhihu";
  if (normalized === "bilibili.com" || normalized.endsWith(".bilibili.com") || normalized === "b23.tv") return "bilibili";
  if (normalized === "xiaohongshu.com" || normalized.endsWith(".xiaohongshu.com")) return "xiaohongshu";
  if (normalized === "x.com" || normalized.endsWith(".x.com") || normalized === "twitter.com" || normalized.endsWith(".twitter.com")) return "x";
  return "connector";
}
