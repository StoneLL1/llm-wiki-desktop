import type { AgentKind } from "./agent";
import type {
  AgentAssistancePolicy,
  AgentAssistanceTrigger,
} from "./importV2Agent";
import type { CapabilityRequirement } from "./importV2File";
import type {
  MigrationStatus,
} from "./importV2Migration";
import type { LlmProviderKind } from "./llm";

export interface ImportPreviewContent {
  sessionId: string;
  itemId: string;
  candidateId: string | null;
  title: string;
  markdown: string;
  truncated: boolean;
  totalBytes: number;
  sha256: string;
}
export interface ImportFrontendReadiness {
  backendVersion: string;
  active: boolean;
  migrationStatus: MigrationStatus;
  unfinishedSessionId: string | null;
  legacyHistoryAvailable: boolean;
  platforms?: ImportPlatformReadiness[];
}

export interface ImportPlatformReadiness {
  id: string;
  available: boolean;
  reasonCode?: string | null;
}

export type ImportHistoryAction = "open_detail" | "open_result" | "view_logs";

export interface GetImportPreviewContentV2Request {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  itemId: string;
  candidateId: string | null;
  historyBatchId?: string | null;
}

export interface GetImportFrontendReadinessV2Request {
  projectId: string;
  projectRootPath: string;
}

export interface ImportHistoryEntry {
  id: string;
  title: string;
  status: string;
  sessionId: string | null;
  batchId: string | null;
  taskId: string | null;
  startedAt: string | null;
  updatedAt: string | null;
  completedAt: string | null;
  legacyReadOnly: boolean;
  itemIds: string[];
  availableActions: ImportHistoryAction[];
  snapshotAvailable?: boolean;
}

export function canOpenHistoricalResult(entry: Pick<ImportHistoryEntry, "status" | "availableActions">): boolean {
  return (entry.status === "completed" || entry.status === "partially_committed")
    && entry.availableActions.includes("open_result");
}

export interface LegacyHistoryEntry {
  id: string;
  title: string;
  status: string;
  startedAt: string | null;
  updatedAt: string | null;
  completedAt: string | null;
  evidencePath: string;
  legacyReadOnly: true;
  availableActions: string[];
  canRetry: false;
  canDelete: false;
  canReplaceSource: false;
}

export interface LegacyHistoryWarning {
  code: string;
  message: string;
  evidencePath: string;
}

export interface ImportHistoryPage {
  entries: ImportHistoryEntry[];
  legacyReadOnly: LegacyHistoryEntry[];
  nextCursor: string | null;
  warnings: LegacyHistoryWarning[];
}

export interface ListImportHistoryV2Request {
  projectId: string;
  projectRootPath: string;
  cursor: string | null;
  limit?: number;
}

export interface ImportCapabilityRequirement {
  requirement: CapabilityRequirement;
  route: string;
  available: boolean;
  installable: boolean;
  compressedBytes: number | null;
  installedBytes: number | null;
  modelBytes: number | null;
  license: string | null;
  fallback: string | null;
}

export interface GetImportCapabilityRequirementV2Request {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  itemId: string;
}

export interface InstallImportCapabilityV2Request {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  itemId: string;
  capabilityId: string;
  acknowledgeInstall: boolean;
}

export interface BeginImportLoginV2Request {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  itemId: string;
  platform: string;
}

export interface ConnectorSessionRef {
  sessionId: string;
  platform: string;
  profileRef: string;
  state: string;
}

export interface CompleteImportLoginV2Request {
  projectId: string;
  projectRootPath: string;
  importSessionId: string;
  itemId: string;
  connectorSessionId: string;
}

export interface RevokeImportLoginV2Request {
  sessionId: string;
  platform?: string | null;
}

export interface AuthorizeImportPrivateTargetV2Request {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  itemId: string;
  url: string;
}

export interface AuthorizeLocalAsrV2Request {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  itemId: string;
}
export type AuthorizeBilibiliAsrV2Request = AuthorizeLocalAsrV2Request;

export interface GetImportAgentPolicyV2Request {
  projectId: string;
  projectRootPath: string;
}

export interface SetImportAgentPolicyV2Request {
  projectId: string;
  projectRootPath: string;
  policy: AgentAssistancePolicy;
  localAgentKind: AgentKind | null;
}

export interface StartImportAgentAssistanceV2Request {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  itemId: string;
  trigger: AgentAssistanceTrigger;
  agentKind: AgentKind;
}

export interface PreviewImportByokScopeV2Request {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  itemId: string;
  trigger: AgentAssistanceTrigger;
  provider: LlmProviderKind;
}

export interface ApproveImportByokAssistanceV2Request {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  itemId: string;
  trigger: AgentAssistanceTrigger;
  provider: LlmProviderKind;
  model: string;
  approvalId: string;
  scopeSha256: string;
  acknowledgePossibleDuplicateCharge: boolean;
}
