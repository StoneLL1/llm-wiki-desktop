import type { AgentKind } from "./agent";
import type { AgentAssistanceTrigger } from "./importV2Agent";
import type { CapabilityRequirement } from "./importV2File";
import type { BackendTask } from "./task";
import type {
  MigrationStatus,
} from "./importV2Migration";

export type ImportWorkbenchSection = "workbench" | "capabilities" | "history";
export type ImportWorkbenchQueueFilter = "all" | "active" | "ready" | "needs_action" | "failed";

export interface ImportWorkbenchPreferences {
  schemaVersion: 1;
  activeSection: ImportWorkbenchSection;
  queueFilter: ImportWorkbenchQueueFilter;
  workbenchScrollTop: number;
  capabilitiesScrollTop: number;
  historyScrollTop: number;
  sourceMethodsExpanded: boolean;
}

export interface ImportWorkbenchPreferencesRequest {
  projectId: string;
  projectRootPath: string;
}

export interface SaveImportWorkbenchPreferencesRequest extends ImportWorkbenchPreferencesRequest {
  preferences: ImportWorkbenchPreferences;
}

export interface ImportPreviewContent {
  sessionId: string;
  itemId: string;
  candidateId: string | null;
  title: string;
  markdown: string;
  truncated: boolean;
  totalBytes: number;
  sha256: string;
  target?: ImportPreviewTarget;
  quality?: import("./importV2").QualityReport;
  rawLabel?: string;
  resources?: ImportPreviewResource[];
  comparison?: ImportPreviewComparison | null;
}

export interface ImportPreviewComparison {
  currentMarkdown: string;
  mergedMarkdown?: string | null;
}

export interface ImportPreviewTarget {
  disposition: "new_source" | "update" | "merge" | "duplicate";
  sourceId: string | null;
  versionId: string | null;
  wikiPath: string | null;
}

export interface ImportPreviewResource {
  source: string;
  name: string;
  kind: "image" | "attachment" | "subtitle" | "transcript" | "metadata" | "source_evidence" | "source_snapshot" | "markdown";
  sizeBytes: number;
  dataUrl: string | null;
}
export interface ImportFrontendReadiness {
  backendVersion: string;
  active: boolean;
  migrationStatus: MigrationStatus;
  unfinishedSessionId: string | null;
  legacyHistoryAvailable: boolean;
  files?: ImportFeatureReadiness[];
  platforms?: ImportPlatformReadiness[];
  abilities?: ImportFeatureReadiness[];
  capabilities?: ImportCapabilityReadiness[];
}

export interface GetImportRestrictedContentStatusV2Request {
  projectId: string;
  projectRootPath: string;
}

export interface ImportRestrictedContentStatus {
  confirmationRequired: boolean;
}

export interface ImportPlatformReadiness {
  id: string;
  available: boolean;
  reasonCode?: string | null;
}

export type ImportHistoryAction = "open_detail" | "open_result" | "view_logs" | "update_wiki";

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

export interface GetImportAsrEnablementPlanV2Request {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  itemId: string;
}

export type ImportAsrDependencyKind = "media_runtime" | "engine" | "model" | "language_support";

export interface ImportAsrDependency {
  kind: ImportAsrDependencyKind;
  name: string;
  available: boolean;
  bundledWithCapability: boolean;
  source: string;
  license: string;
}

export interface ImportAsrProfilePlan {
  profile: import("./importV2").ImportAsrProfile;
  capabilityId: string;
  engineName: string;
  modelName: string;
  available: boolean;
  installable: boolean;
  downloadBytes: number | null;
  installedBytes: number | null;
  modelBytes: number | null;
  device: string;
  estimatedSeconds: number | null;
  unavailableReasonCode: string | null;
  dependencies: ImportAsrDependency[];
}

export interface ImportAsrEnablementPlan {
  recommendedProfile: import("./importV2").ImportAsrProfile;
  availableMemoryBytes: number | null;
  availableDiskBytes: number | null;
  mediaDurationSeconds: number | null;
  installLocation: string | null;
  localOnly: boolean;
  profiles: ImportAsrProfilePlan[];
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
  state: string;
  accountSummary?: string | null;
  lastVerifiedAt?: string | null;
}

export interface CompleteImportLoginV2Request {
  projectId: string;
  projectRootPath: string;
  importSessionId: string;
  itemId: string;
  connectorSessionId: string;
}

export interface CompleteImportLoginResult {
  connectorSession: ConnectorSessionRef;
  resumedItemIds: string[];
  tasks: BackendTask[];
}

export interface RevokeImportLoginV2Request {
  sessionId: string;
  platform?: string | null;
}

export interface ImportFeatureReadiness {
  id: string;
  available: boolean;
  reasonCode?: string | null;
}

export interface ImportCapabilityReadiness {
  capabilityId: string;
  route: string;
  available: boolean;
  reasonCode?: string | null;
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
  profile: import("./importV2").ImportAsrProfile;
  language?: string | null;
}
export type AuthorizeBilibiliAsrV2Request = AuthorizeLocalAsrV2Request;

export interface StartImportAgentAssistanceV2Request {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  itemId: string;
  trigger: AgentAssistanceTrigger;
  agentKind: AgentKind;
}
