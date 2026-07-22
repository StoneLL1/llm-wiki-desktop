import type { AgentKind } from "../../types/agent";
import type { LlmProviderKind } from "../../types/llm";
import type { CommitItemDecision, ImportItem, ImportRecoveryAction, ImportSession, MediaSaveMode } from "../../types/importV2";
import type {
  AgentAssistancePolicy,
  AgentAssistanceTrigger,
  AgentCandidateActionResult,
  AgentCandidateView,
  AgentSendScope,
} from "../../types/importV2Agent";
import type { FileScanResult } from "../../types/importV2File";
import type { LegacyInventory, MigrationConfirmation, MigrationPlan, MigrationStatusSnapshot } from "../../types/importV2Migration";
import type {
  ConnectorSessionRef,
  ImportCapabilityRequirement,
  ImportFrontendReadiness,
  ImportHistoryPage,
  ImportPreviewContent,
} from "../../types/importV2Presentation";
import type { BackendTask } from "../../types/task";
import type { ImportQueueFilter } from "../../stores/importStore";
import type { ImportQueueCounts, ImportSessionProgress } from "./importViewModel";

export type ImportBootstrapState = "loading" | "ready" | "blocked" | "error";

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
  isAddingUrl?: boolean;
  pendingItemIds?: ReadonlySet<string>;
  isSyncingSession?: boolean;
  batches?: readonly ImportBatchProgress[];
  batch?: ImportBatchProgress | null;
  isCancellingBatch?: boolean;
  isBatchCancelling?: (batchId: string) => boolean;
  selectedItemId: string | null;
  filter: ImportQueueFilter;
  addPaths: (paths: string[]) => Promise<void>;
  addUrl: (url: string, mediaSaveMode?: MediaSaveMode) => Promise<void>;
  cancelDiscovery?: () => Promise<void>;
  dismissDiscovery?: () => void;
  cancelBatch?: (batchId?: string) => Promise<void>;
  dismissBatch?: (batchId?: string) => void;
  retryBatch?: (batchId: string) => Promise<void>;
  setItemSelected: (itemId: string, selected: boolean) => Promise<void>;
  startItems: (itemIds: readonly string[], recoveryAction?: ImportRecoveryAction | null) => Promise<void>;
  retryItem: (itemId: string, recoveryAction?: ImportRecoveryAction | null) => Promise<void>;
  cancelItem: (itemId: string) => Promise<void>;
  skipItem: (itemId: string) => Promise<void>;
  authorizeLocalAsr: (itemId: string) => Promise<void>;
  confirm: (decisions: CommitItemDecision[]) => Promise<void>;
  isConfirming: boolean;
  refreshSession: () => Promise<void>;
  selectItem: (itemId: string | null) => void;
  setFilter: (filter: ImportQueueFilter) => void;
  requestClipboard: (content: string) => Promise<void>;
  loadPreview: (identity: { sessionId: string; itemId: string; candidateId: string | null; historyBatchId?: string | null }) => Promise<ImportPreviewContent>;
  loadSession: (sessionId: string, historyBatchId?: string | null) => Promise<ImportSession | null>;
  getAgentPolicy: () => Promise<AgentAssistancePolicy | null>;
  setAgentPolicy: (policy: AgentAssistancePolicy, localAgentKind: AgentKind | null) => Promise<AgentAssistancePolicy | null>;
  invokeLocalAgent: (itemId: string, trigger: AgentAssistanceTrigger, agentKind: AgentKind) => Promise<BackendTask | null>;
  previewByokScope: (itemId: string, trigger: AgentAssistanceTrigger, provider: LlmProviderKind) => Promise<AgentSendScope | null>;
  approveByokAssistance: (request: {
    itemId: string;
    trigger: AgentAssistanceTrigger;
    provider: LlmProviderKind;
    model: string;
    approvalId: string;
    scopeSha256: string;
    acknowledgePossibleDuplicateCharge: boolean;
  }) => Promise<BackendTask | null>;
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
  installCapability: (itemId: string, capabilityId: string) => Promise<BackendTask | null>;
  scanMigration: () => Promise<LegacyInventory | null>;
  planMigration: (inventory: LegacyInventory) => Promise<MigrationPlan | null>;
  applyMigration: (plan: MigrationPlan, confirmation: MigrationConfirmation) => Promise<BackendTask | null>;
  getMigrationStatus: () => Promise<MigrationStatusSnapshot | null>;
  resumeMigration: (plan: MigrationPlan, confirmation: MigrationConfirmation) => Promise<BackendTask | null>;
  listHistory: (cursor?: string | null) => Promise<ImportHistoryPage | null>;
}
