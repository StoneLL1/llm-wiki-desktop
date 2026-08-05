import type { AgentKind } from "../../types/agent";
import type { CommitItemDecision, ImportAsrProfile, ImportCompletion, ImportItem, ImportItemResolution, ImportRecoveryAction, ImportSession, ImportThreeWayMergeContext, MediaSaveMode } from "../../types/importV2";
import type {
  AgentAssistanceTrigger,
  AgentCandidateActionResult,
  AgentCandidateView,
} from "../../types/importV2Agent";
import type { FileScanResult } from "../../types/importV2File";
import type { ImportCollectionPreview, RemoteMediaRetentionPlan } from "../../types/importV2Web";
import type { LegacyInventory, MigrationConfirmation, MigrationPlan, MigrationPreparation, MigrationStatusSnapshot } from "../../types/importV2Migration";
import type {
  ConnectorSessionRef,
  ImportAsrEnablementPlan,
  ImportCapabilityRequirement,
  ImportFrontendReadiness,
  ImportHistoryPage,
  ImportPreviewContent,
  ImportWorkbenchPreferences,
} from "../../types/importV2Presentation";
import type { BackendTask } from "../../types/task";
import type { ImportQueueFilter } from "../../stores/importStore";
import type { ImportQueueCounts, ImportSessionProgress } from "./importViewModel";

export type ImportBootstrapState = "loading" | "ready" | "blocked" | "error";
export interface AsrAuthorizationOptions { profile: ImportAsrProfile; language: string | null; }

export interface ImportBatchTask {
  id: string;
  itemId: string;
  title: string;
  status: BackendTask["status"] | "unknown";
  cancellable: boolean;
}

export interface ImportBatchProgress {
  id: string;
  sessionId: string;
  total: number;
  taskIds: readonly string[];
  processed: number;
  active: number;
  completed: number;
  /** Number of child tasks waiting for any user action, including login. */
  waitingForConfirmation?: number;
  /** Subset of waiting tasks whose item is ready for preview/commit review. */
  reviewReady?: number;
  failed: number;
  cancelled: number;
  cancelling: number;
  unknown: number;
  nonCancellable: number;
  failedItemIds: readonly string[];
  tasks: readonly ImportBatchTask[];
}

export interface ImportWorkflow {
  projectKey: string;
  session: ImportSession | null;
  completion: ImportCompletion | null;
  readiness: ImportFrontendReadiness | null;
  /** Readiness is advisory; a warning must not prevent V2 staging. */
  readinessWarning?: string | null;
  /** Only session/project bootstrap failures block the import surface. */
  bootstrapError?: string | null;
  retryBootstrap?: () => void;
  bootstrapState: ImportBootstrapState;
  visibleItems: ImportItem[];
  counts: ImportQueueCounts;
  progress: ImportSessionProgress;
  discoveryTask?: BackendTask | null;
  discoveryScan?: FileScanResult | null;
  discoveryTaskUnavailable?: boolean;
  isAddingPaths?: boolean;
  isAddingText?: boolean;
  isAddingUrl?: boolean;
  pendingItemIds?: ReadonlySet<string>;
  isSyncingSession?: boolean;
  batches?: readonly ImportBatchProgress[];
  batch?: ImportBatchProgress | null;
  isCancellingBatch?: boolean;
  isBatchCancelling?: (batchId: string) => boolean;
  selectedItemId: string | null;
  filter: ImportQueueFilter;
  addPaths: (paths: string[], largeDataConfirmed?: boolean) => Promise<void>;
  addText: (content: string, sourceName: string) => Promise<void>;
  addUrl: (url: string, mediaSaveMode?: MediaSaveMode) => Promise<void>;
  collectionPreview: ImportCollectionPreview | null;
  loadCollectionPage: (loadAll?: boolean) => Promise<void>;
  confirmCollection: (itemRefs: readonly string[]) => Promise<void>;
  dismissCollection: () => void;
  remoteMediaRetentionPlan: RemoteMediaRetentionPlan | null;
  planRemoteMediaRetention: (itemId: string) => Promise<void>;
  confirmRemoteMediaRetention: () => Promise<void>;
  dismissRemoteMediaRetention: () => void;
  cancelDiscovery?: () => Promise<void>;
  confirmDiscovery?: (sourcePaths?: readonly string[]) => Promise<void>;
  dismissDiscovery?: () => Promise<void>;
  cancelBatch?: (batchId?: string) => Promise<void>;
  dismissBatch?: (batchId?: string) => void;
  retryBatch?: (batchId: string) => Promise<void>;
  setItemSelected: (itemId: string, selected: boolean) => Promise<void>;
  startItems: (itemIds: readonly string[], recoveryAction?: ImportRecoveryAction | null) => Promise<void>;
  retryItem: (itemId: string, recoveryAction?: ImportRecoveryAction | null) => Promise<void>;
  cancelItem: (itemId: string) => Promise<void>;
  skipItem: (itemId: string) => Promise<void>;
  authorizeLocalAsr: (itemId: string, options: AsrAuthorizationOptions) => Promise<void>;
  authorizeLocalAsrGroup?: (itemIds: readonly string[], options: AsrAuthorizationOptions) => Promise<void>;
  authorizeLocalOcr: (itemId: string) => Promise<void>;
  authorizeLocalOcrGroup?: (itemIds: readonly string[]) => Promise<void>;
  selectSubtitle: (itemId: string, fileName: string) => Promise<void>;
  confirm: (decisions: CommitItemDecision[]) => Promise<void>;
  restrictedCommitPending: boolean;
  confirmRestrictedContent: () => Promise<void>;
  dismissRestrictedContent: () => void;
  viewImportedSources: (
    completion?: ImportCompletion | null,
    preferredWikiPath?: string,
  ) => Promise<void>;
  updateWiki: (completion?: ImportCompletion | null) => Promise<void>;
  isConfirming: boolean;
  refreshSession: () => Promise<void>;
  selectItem: (itemId: string | null) => void;
  setFilter: (filter: ImportQueueFilter) => void;
  requestClipboard: (content: string) => Promise<void>;
  loadPreview: (identity: { sessionId: string; itemId: string; candidateId: string | null; historyBatchId?: string | null }) => Promise<ImportPreviewContent>;
  loadMergeContext: (itemId: string) => Promise<ImportThreeWayMergeContext>;
  setItemResolution: (itemId: string, resolution: ImportItemResolution) => Promise<void>;
  stageManualMerge: (itemId: string, mergedMarkdown: string) => Promise<void>;
  loadSession: (sessionId: string, historyBatchId?: string | null) => Promise<ImportSession | null>;
  loadCompletion: (sessionId: string, historyBatchId: string) => Promise<ImportCompletion | null>;
  invokeLocalAgent: (itemId: string, trigger: AgentAssistanceTrigger, agentKind: AgentKind) => Promise<BackendTask | null>;
  acceptAgentCandidate: (itemId: string, taskId: string) => Promise<AgentCandidateView | null>;
  selectAgentCandidate: (request: {
    itemId: string;
    candidateId: string;
    mergedMarkdown: string | null;
    expectedCurrentWikiSha256: string | null;
  }) => Promise<AgentCandidateActionResult | null>;
  discardAgentCandidate: (itemId: string, candidateId: string) => Promise<AgentCandidateActionResult | null>;
  beginLogin: (itemId: string, platform: string) => Promise<ConnectorSessionRef | null>;
  completeLogin: (itemId: string, connectorSessionId: string) => Promise<ConnectorSessionRef | null>;
  revokeLogin: (connectorSessionId: string, platform?: string | null) => Promise<boolean>;
  authorizePrivateTarget: (itemId: string, url: string) => Promise<string | null>;
  getCapabilityRequirement: (itemId: string) => Promise<ImportCapabilityRequirement | null>;
  getAsrEnablementPlan: (itemId: string) => Promise<ImportAsrEnablementPlan | null>;
  installCapability: (itemId: string, capabilityId: string) => Promise<BackendTask | null>;
  scanMigration: () => Promise<LegacyInventory | null>;
  planMigration: (inventory: LegacyInventory) => Promise<MigrationPreparation | null>;
  applyMigration: (plan: MigrationPlan, confirmation: MigrationConfirmation) => Promise<BackendTask | null>;
  getMigrationStatus: () => Promise<MigrationStatusSnapshot | null>;
  resumeMigration: (plan: MigrationPlan, confirmation: MigrationConfirmation) => Promise<BackendTask | null>;
  listHistory: (cursor?: string | null) => Promise<ImportHistoryPage | null>;
  loadWorkbenchPreferences?: () => Promise<ImportWorkbenchPreferences>;
  saveWorkbenchPreferences?: (preferences: ImportWorkbenchPreferences) => Promise<ImportWorkbenchPreferences>;
}
