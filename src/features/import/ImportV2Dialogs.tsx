import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { useImportStore } from "../../stores/importStore";
import type { ImportItem } from "../../types/importV2";
import type { AgentCandidateView as AgentCandidateViewType } from "../../types/importV2Agent";
import type { ConnectorSessionRef, ImportAsrEnablementPlan, ImportCapabilityRequirement } from "../../types/importV2Presentation";
import type { WebAuthState } from "../../types/importV2Web";
import { ImportCandidateDiffDialog, type ImportCandidateDiffIntent } from "./ImportCandidateDiffDialog";
import { ImportCapabilityDialog } from "./ImportCapabilityDialog";
import { ImportAsrDialog } from "./ImportAsrDialog";
import { ImportSubtitleDialog } from "./ImportSubtitleDialog";
import { ImportLoginDialog } from "./ImportLoginDialog";
import { ImportCollectionDialog } from "./ImportCollectionDialog";
import { ImportRemoteMediaDialog } from "./ImportRemoteMediaDialog";
import { ImportRestrictedContentDialog } from "./ImportRestrictedContentDialog";
import { ImportMarkdownPreviewDialog } from "./ImportMarkdownPreviewDialog";
import { ImportPrivateTargetDialog } from "./ImportPrivateTargetDialog";
import { displayHostForImportLocator, importPlatformForLocator } from "./importLocator";
import type { ImportWorkflow } from "./useImportWorkflow";

export interface ImportV2DialogsProps {
  workflow: ImportWorkflow;
  privateItem: ImportItem | null;
  asrItem: ImportItem | null;
  asrItemIds?: readonly string[];
  subtitleItem: ImportItem | null;
  candidateView: AgentCandidateViewType | null;
  onCloseCandidate: () => void;
  onCandidateIntent: (intent: ImportCandidateDiffIntent) => void;
  onClosePrivate: () => void;
  onCloseAsr: () => void;
  onCloseSubtitle: () => void;
}

export function ImportV2Dialogs({ workflow, privateItem, asrItem, asrItemIds = [], subtitleItem, candidateView, onCloseCandidate, onCandidateIntent, onClosePrivate, onCloseAsr, onCloseSubtitle }: ImportV2DialogsProps) {
  const { t } = useTranslation();
  const session = useImportStore((state) => state.session);
  const previewItemId = useImportStore((state) => state.previewItemId);
  const capabilityItemId = useImportStore((state) => state.capabilityItemId);
  const loginItemId = useImportStore((state) => state.loginItemId);
  const closePreview = useImportStore((state) => state.closePreview);
  const closeCapability = useImportStore((state) => state.closeCapability);
  const closeLogin = useImportStore((state) => state.closeLogin);
  const asrItemId = asrItem?.itemId ?? null;

  const previewItem = session?.items.find((item) => item.itemId === previewItemId) ?? null;
  const previewIdentity = session && previewItem ? { sessionId: session.sessionId, itemId: previewItem.itemId, candidateId: null } : null;
  const capabilityItem = session?.items.find((item) => item.itemId === capabilityItemId) ?? null;
  const loginItem = session?.items.find((item) => item.itemId === loginItemId) ?? null;

  const [capability, setCapability] = useState<ImportCapabilityRequirement | null>(null);
  const [asrPlan, setAsrPlan] = useState<ImportAsrEnablementPlan | null>(null);
  const [asrPlanLoading, setAsrPlanLoading] = useState(false);
  const [connector, setConnector] = useState<ConnectorSessionRef | null>(null);
  const activeProjectKeyRef = useRef(workflow.projectKey);
  activeProjectKeyRef.current = workflow.projectKey;

  useEffect(() => {
    setCapability(null);
    setAsrPlan(null);
    setAsrPlanLoading(false);
    setConnector(null);
  }, [workflow.projectKey]);

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
    if (!asrItemId) {
      setAsrPlan(null);
      setAsrPlanLoading(false);
      return;
    }
    let current = true;
    setAsrPlan(null);
    setAsrPlanLoading(true);
    void workflow.getAsrEnablementPlan(asrItemId).then((next) => {
      if (current) setAsrPlan(next);
    }).catch(() => {
      if (current) setAsrPlan(null);
    }).finally(() => {
      if (current) setAsrPlanLoading(false);
    });
    return () => { current = false; };
  }, [asrItemId, workflow.getAsrEnablementPlan]);

  useEffect(() => {
    if (!loginItemId) setConnector(null);
  }, [loginItemId]);

  const loginLocator = loginItem?.input.kind === "url" ? loginItem.input.normalizedLocator ?? loginItem.input.locator : "";
  const loginDomain = loginLocator ? displayHostForImportLocator(loginLocator) : "connector";
  const loginPlatform = importPlatformForLocator(loginLocator);
  const loginPlatformLabel = t(`importV2.platform.${loginPlatform}`, { defaultValue: loginPlatform });
  const loginAuthState: WebAuthState = loginItem?.status === "waiting_login" ? "waiting_login" : connector?.state === "authenticated" ? "authenticated" : "public";

  return (
    <>
      <ImportCollectionDialog
        preview={workflow.collectionPreview}
        onLoadMore={workflow.loadCollectionPage}
        onConfirm={workflow.confirmCollection}
        onCancel={workflow.dismissCollection}
      />
      <ImportRemoteMediaDialog
        plan={workflow.remoteMediaRetentionPlan}
        onConfirm={workflow.confirmRemoteMediaRetention}
        onCancel={workflow.dismissRemoteMediaRetention}
      />
      <ImportRestrictedContentDialog
        open={workflow.restrictedCommitPending}
        onConfirm={workflow.confirmRestrictedContent}
        onCancel={workflow.dismissRestrictedContent}
      />
      <ImportMarkdownPreviewDialog open={Boolean(previewIdentity)} identity={previewIdentity} loadContent={workflow.loadPreview} onClose={closePreview} />
      <ImportCapabilityDialog
        open={Boolean(capabilityItemId && capability)}
        requirement={capability}
        sessionId={session?.sessionId ?? null}
        itemId={capabilityItem?.itemId ?? null}
        onCancel={closeCapability}
        onInstall={async (capabilityId) => {
          if (capabilityItem && capability) return workflow.installCapability(
            capabilityItem.itemId,
            capabilityId,
            capability.requirementRevision,
          );
          return null;
        }}
      />
      <ImportAsrDialog
        open={Boolean(asrItem)}
        plan={asrPlan}
        loading={asrPlanLoading}
        onCancel={onCloseAsr}
        onConfirm={async (options) => {
          if (!asrItem) return;
          const itemIds = asrItemIds.length > 0 ? asrItemIds : [asrItem.itemId];
          if (workflow.authorizeLocalAsrGroup) {
            await workflow.authorizeLocalAsrGroup(itemIds, options);
          } else {
            for (const itemId of itemIds) {
              await workflow.authorizeLocalAsr(itemId, options);
            }
          }
          onCloseAsr();
        }}
        sessionId={session?.sessionId ?? null}
        itemId={asrItem?.itemId ?? null}
        onInstall={async (capabilityId, options) => {
          if (!asrItem) return;
          return workflow.installCapability(
            asrItem.itemId,
            capabilityId,
            asrPlan!.requirementRevision,
            options,
          );
        }}
      />
      <ImportSubtitleDialog
        open={Boolean(subtitleItem)}
        candidates={subtitleItem?.issue?.subtitleCandidates ?? []}
        onCancel={onCloseSubtitle}
        onConfirm={async (fileName) => {
          if (!subtitleItem) return;
          await workflow.selectSubtitle(subtitleItem.itemId, fileName);
          onCloseSubtitle();
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
    </>
  );
}
