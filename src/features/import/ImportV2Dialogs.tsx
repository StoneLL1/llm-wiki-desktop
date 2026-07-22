import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import type { AiCapabilitiesWorkflow } from "../../hooks/useAiCapabilities";
import { useImportStore } from "../../stores/importStore";
import type { ImportItem } from "../../types/importV2";
import type { AgentCandidateView as AgentCandidateViewType, AgentSendScope } from "../../types/importV2Agent";
import type { ConnectorSessionRef, ImportCapabilityRequirement, ImportFrontendReadiness } from "../../types/importV2Presentation";
import type { LegacyInventory, MigrationPlan, MigrationReport } from "../../types/importV2Migration";
import type { WebAuthState } from "../../types/importV2Web";
import type { LlmProviderKind } from "../../types/llm";
import { ImportByokApprovalDialog } from "./ImportByokApprovalDialog";
import { ImportCandidateDiffDialog, type ImportCandidateDiffIntent } from "./ImportCandidateDiffDialog";
import { ImportCapabilityDialog } from "./ImportCapabilityDialog";
import { ImportLoginDialog } from "./ImportLoginDialog";
import { ImportMarkdownPreviewDialog } from "./ImportMarkdownPreviewDialog";
import { ImportMigrationDialog, type ImportMigrationUiStatus } from "./ImportMigrationDialog";
import { ImportPrivateTargetDialog } from "./ImportPrivateTargetDialog";
import { displayHostForImportLocator, importPlatformForLocator } from "./importLocator";
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
  const activeProjectKeyRef = useRef(workflow.projectKey);
  activeProjectKeyRef.current = workflow.projectKey;

  const provider = useMemo(() => providerFor(capabilities), [capabilities]);

  useEffect(() => {
    setScope(null);
    setCapability(null);
    setConnector(null);
    setMigrationState(migrationStatus(readiness));
    setInventory(null);
    setPlan(null);
    setReport(null);
  }, [readiness, workflow.projectKey]);

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
    const requestProjectKey = workflow.projectKey;
    let current = true;
    setMigrationState(migrationStatus(readiness));
    void workflow.getMigrationStatus().then((snapshot) => {
      if (!current || activeProjectKeyRef.current !== requestProjectKey || !snapshot) return;
      setMigrationState(snapshot.status);
      setReport(snapshot.report ?? null);
    }).catch(() => undefined);
    return () => { current = false; };
  }, [migrationOpen, readiness, workflow.getMigrationStatus, workflow.projectKey]);

  async function scanMigration() {
    const requestProjectKey = workflow.projectKey;
    setMigrationState("scanning");
    const next = await workflow.scanMigration();
    if (activeProjectKeyRef.current !== requestProjectKey) return;
    if (!next) {
      setMigrationState(migrationStatus(readiness));
      return;
    }
    setInventory(next);
    setPlan(null);
    setReport(null);
    setMigrationState("dry_run_ready");
  }

  async function buildPlan(nextInventory: LegacyInventory) {
    const requestProjectKey = workflow.projectKey;
    setMigrationState("scanning");
    const nextPlan = await workflow.planMigration(nextInventory);
    if (activeProjectKeyRef.current !== requestProjectKey) return;
    if (!nextPlan) {
      setMigrationState(migrationStatus(readiness));
      return;
    }
    setPlan(nextPlan);
    const snapshot = await workflow.getMigrationStatus();
    if (activeProjectKeyRef.current !== requestProjectKey) return;
    setReport(snapshot?.report ?? null);
    setMigrationState(snapshot?.status ?? "awaiting_confirmation");
  }

  const loginLocator = loginItem?.input.kind === "url" ? loginItem.input.normalizedLocator ?? loginItem.input.locator : "";
  const loginDomain = loginLocator ? displayHostForImportLocator(loginLocator) : "connector";
  const loginPlatform = importPlatformForLocator(loginLocator);
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
        onBeginLogin={() => {
          const requestProjectKey = workflow.projectKey;
          return workflow.beginLogin(loginItem!.itemId, loginPlatform).then((next) => {
            if (activeProjectKeyRef.current === requestProjectKey) setConnector(next);
            return next ?? undefined;
          });
        }}
        onCheckAgain={(connectorSessionId) => {
          const requestProjectKey = workflow.projectKey;
          return workflow.completeLogin(loginItem!.itemId, connectorSessionId).then((next) => {
            if (activeProjectKeyRef.current === requestProjectKey) setConnector(next);
            return next ?? undefined;
          });
        }}
        onRevoke={async (connectorSessionId) => {
          const requestProjectKey = workflow.projectKey;
          await workflow.revokeLogin(connectorSessionId, loginPlatform);
          if (activeProjectKeyRef.current === requestProjectKey) setConnector(null);
        }}
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
        confirmation={null}
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
