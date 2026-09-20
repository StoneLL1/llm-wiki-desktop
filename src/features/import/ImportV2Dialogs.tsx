import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { normalizeBackendError, type NormalizedBackendError, backendErrorCode } from "../../lib/backendError";
import { useImportStore } from "../../stores/importStore";
import { useTaskStore } from "../../stores/taskStore";
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
  const sessionId = useImportStore((state) => state.session?.sessionId ?? null);
  const collectionSessionId = workflow.session?.sessionId ?? null;
  const restoredCollection = useTaskStore((state) => state.tasks.find((task) =>
    task.status === "waiting_for_confirmation"
    && task.operation?.kind === "import_collection_discovery"
    && task.operation.sessionId === collectionSessionId
    && task.result?.reference?.type === "import_collection_preview")?.result?.reference);
  const previewItemId = useImportStore((state) => state.previewItemId);
  const capabilityItemId = useImportStore((state) => state.capabilityItemId);
  const loginItemId = useImportStore((state) => state.loginItemId);
  const closePreview = useImportStore((state) => state.closePreview);
  const closeCapability = useImportStore((state) => state.closeCapability);
  const closeLogin = useImportStore((state) => state.closeLogin);
  const asrItemId = asrItem?.itemId ?? null;

  const previewItem = useImportStore((state) => previewItemId ? state.itemById[previewItemId] ?? null : null);
  const capabilityItem = useImportStore((state) => capabilityItemId ? state.itemById[capabilityItemId] ?? null : null);
  const loginItem = useImportStore((state) => loginItemId ? state.itemById[loginItemId] ?? null : null);
  const previewIdentity = sessionId && previewItem ? { sessionId, itemId: previewItem.itemId, candidateId: null } : null;

  const requirementItemId = capabilityItemId ?? loginItemId;
  const requirementItem = capabilityItem ?? loginItem;
  const [queryRevision, setQueryRevision] = useState(0);
  const [capabilityLoading, setCapabilityLoading] = useState(false);
  const [capabilityError, setCapabilityError] = useState<NormalizedBackendError | null>(null);
  const [asrPlanError, setAsrPlanError] = useState<NormalizedBackendError | null>(null);
  const preparedLogin = useRef<string | null>(null);
  const browserInstallCompletion = useTaskStore((state) => state.tasks.filter((task) =>
    task.operation?.kind === "app_capability_install" && task.operation.capabilityId === "browser-runtime"
    && task.status === "succeeded").reduce((latest, task) => task.updatedAt > latest ? task.updatedAt : latest, ""));
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
    preparedLogin.current = null;
  }, [workflow.projectKey]);

  useEffect(() => {
    if (!workflow.collectionPreview && restoredCollection?.type === "import_collection_preview") {
      workflow.restoreCollection?.(restoredCollection.preview);
    }
  }, [restoredCollection, workflow.collectionPreview, workflow.restoreCollection]);

  useEffect(() => {
    setCapability(null);
    setCapabilityError(null);
    if (!requirementItemId) { setCapabilityLoading(false); return; }
    let current = true;
    setCapabilityLoading(true);
    void workflow.getCapabilityRequirement(requirementItemId).then(async (next) => {
      if (!current) return;
      setCapability(next);
      if (next?.available && loginItemId === requirementItemId && preparedLogin.current === loginItemId) {
        preparedLogin.current = null;
        const locator = loginItem?.input.normalizedLocator ?? loginItem?.input.locator ?? "";
        const connected = await workflow.beginLogin(loginItemId, importPlatformForLocator(locator));
        if (current) setConnector(connected);
      }
    }).catch((error) => {
      if (current) setCapabilityError(normalizeBackendError(error, { defaultActionKind: "retry", defaultRecoverable: true }));
    }).finally(() => { if (current) setCapabilityLoading(false); });
    return () => { current = false; };
  }, [requirementItemId, loginItemId, loginItem?.input.locator, loginItem?.input.normalizedLocator, workflow.getCapabilityRequirement, workflow.beginLogin, workflow.projectKey, queryRevision, browserInstallCompletion]);

  useEffect(() => {
    if (!asrItemId) {
      setAsrPlan(null);
      setAsrPlanLoading(false);
      return;
    }
    let current = true;
    setAsrPlan(null);
    setAsrPlanLoading(true);
    setAsrPlanError(null);
    void workflow.getAsrEnablementPlan(asrItemId).then((next) => {
      if (current) setAsrPlan(next);
    }).catch((error) => {
      if (current) setAsrPlanError(normalizeBackendError(error, { defaultActionKind: "retry", defaultRecoverable: true }));
    }).finally(() => {
      if (current) setAsrPlanLoading(false);
    });
    return () => { current = false; };
  }, [asrItemId, workflow.getAsrEnablementPlan, workflow.projectKey, queryRevision]);

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
        open={Boolean(capabilityItemId || (loginItemId && !capability?.available))}
        loading={capabilityLoading}
        loadError={capabilityError}
        onRetryLoad={() => setQueryRevision((value) => value + 1)}
        requirement={capability}
        sessionId={sessionId}
        itemId={requirementItem?.itemId ?? null}
        onCancel={capabilityItemId ? closeCapability : closeLogin}
        onInstall={async (capabilityId) => {
          if (capabilityItem && capability?.available) {
            const projectKey = workflow.projectKey;
            if (capability.route.startsWith("ocr.")) {
              if (await workflow.authorizeLocalOcr(capabilityItem.itemId) === false) return null;
            } else {
              await workflow.retryItem(capabilityItem.itemId);
            }
            if (activeProjectKeyRef.current === projectKey) closeCapability();
            return null;
          }
          if (requirementItem && capability) {
            if (loginItemId === requirementItem.itemId) preparedLogin.current = loginItemId;
            try {
              return await workflow.installCapability(requirementItem.itemId, capabilityId, capability.requirementRevision);
            } catch (error) {
              preparedLogin.current = null;
              if (backendErrorCode(error) === "IMPORT_V2_CAPABILITY_REQUIREMENT_STALE") setQueryRevision((value) => value + 1);
              throw error;
            }
          }
          return null;
        }}
      />
      <ImportAsrDialog
        open={Boolean(asrItem)}
        plan={asrPlan}
        authorizationError={workflow.authorizationFailures?.find((failure) => (failure.itemId === asrItemId || asrItemIds.includes(failure.itemId)))?.error}
        loading={asrPlanLoading}
        loadError={asrPlanError}
        onRetryLoad={() => setQueryRevision((value) => value + 1)}
        onCancel={onCloseAsr}
        onConfirm={async (options) => {
          if (!asrItem) return;
          const projectKey = workflow.projectKey;
          const itemIds = asrItemIds.length > 0 ? asrItemIds : [asrItem.itemId];
          if (workflow.authorizeLocalAsrGroup) {
            if (await workflow.authorizeLocalAsrGroup(itemIds, options) === false) return;
          } else {
            for (const itemId of itemIds) {
              if (await workflow.authorizeLocalAsr(itemId, options) === false) return;
            }
          }
          if (activeProjectKeyRef.current === projectKey) onCloseAsr();
        }}
        sessionId={sessionId}
        itemId={asrItem?.itemId ?? null}
        onInstall={async (capabilityId, options) => {
          if (!asrItem) return;
          try {
            return await workflow.installCapability(
            asrItem.itemId,
            capabilityId,
            asrPlan!.requirementRevision,
            options,
            asrItemIds.filter((id) => id !== asrItem.itemId),
            );
          } catch (error) {
            if (backendErrorCode(error) === "IMPORT_V2_CAPABILITY_REQUIREMENT_STALE") setQueryRevision((value) => value + 1);
            throw error;
          }
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
        open={Boolean(loginItem && capability?.available && !capabilityError)}
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
