import type { AgentKind } from "./agent";
import type {
  AgentAssistancePolicy,
  AgentAssistanceTrigger,
  AgentCandidateActionResult,
  AgentCandidateView,
  AgentSendScope,
  DiscardImportAgentCandidateRequest,
  SelectImportAgentCandidateRequest,
} from "./importV2Agent";
import type {
  AddImportItemsV2Request,
  CommitImportSessionRequest,
  CreateImportSessionV2Request,
  GetImportSessionV2Request,
  ImportSession,
  SetImportItemSelectionV2Request,
  StartImportItemsV2Request,
} from "./importV2";
import type {
  AddImportPathsV2Request,
  CapabilityRequirement,
} from "./importV2File";
import type {
  ActivateImportV2Request,
  ActivationResult,
  GetImportBackendActivationRequest,
  ImportBackendActivation,
} from "./importV2Activation";
import type {
  ApplyImportV2MigrationRequest,
  MigrationStatus,
  MigrationPlan,
  MigrationProjectRequest,
  MigrationApplyTask,
  PlanImportV2MigrationRequest,
  ResumeImportV2MigrationRequest,
  ScanImportV2MigrationRequest,
  LegacyInventory,
  MigrationStatusSnapshot,
} from "./importV2Migration";
import type { BackendTask } from "./task";
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
}

export interface GetImportPreviewContentV2Request {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  itemId: string;
  candidateId: string | null;
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
  startedAt: string | null;
  updatedAt: string | null;
  completedAt: string | null;
  legacyReadOnly: boolean;
  availableActions: string[];
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
}

export interface AuthorizeImportPrivateTargetV2Request {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  itemId: string;
  url: string;
}

export interface AuthorizeBilibiliAsrV2Request {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  itemId: string;
}

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

export interface ImportV2CommandNames {
  readonly createSession: "create_import_session_v2";
  readonly getSession: "get_import_session_v2";
  readonly addItems: "add_import_items_v2";
  readonly addPaths: "start_add_import_paths_v2";
  readonly addUrl: "add_import_url_v2";
  readonly setSelection: "set_import_item_selection_v2";
  readonly startItems: "start_import_items_v2";
  readonly confirmSession: "confirm_import_session_v2";
  readonly getPreviewContent: "get_import_preview_content_v2";
  readonly getReadiness: "get_import_frontend_readiness_v2";
  readonly listHistory: "list_import_history_v2";
  readonly authorizePrivateTarget: "authorize_import_private_target_v2";
  readonly beginLogin: "begin_import_login_v2";
  readonly completeLogin: "complete_import_login_v2";
  readonly revokeLogin: "revoke_import_login_v2";
  readonly getCapabilityRequirement: "get_import_capability_requirement_v2";
  readonly installCapability: "install_import_capability_v2";
  readonly getAgentPolicy: "get_import_agent_policy_v2";
  readonly setAgentPolicy: "set_import_agent_policy_v2";
  readonly startAgentAssistance: "start_import_agent_assistance_v2";
  readonly previewByokScope: "preview_import_byok_scope_v2";
  readonly approveByokAssistance: "approve_import_byok_assistance_v2";
  readonly acceptAgentCandidate: "accept_import_agent_candidate_v2";
  readonly selectAgentCandidate: "select_import_agent_candidate_v2";
  readonly discardAgentCandidate: "discard_import_agent_candidate_v2";
  readonly scanMigration: "scan_import_v2_migration";
  readonly planMigration: "plan_import_v2_migration";
  readonly applyMigration: "apply_import_v2_migration";
  readonly getMigrationStatus: "get_import_v2_migration_status";
  readonly resumeMigration: "resume_import_v2_migration";
  readonly activate: "activate_import_v2";
  readonly getActivation: "get_import_backend_activation";
}

export type ImportV2Api = {
  readonly commandNames: ImportV2CommandNames;
  readonly createSession: (request: CreateImportSessionV2Request) => Promise<ImportSession>;
  readonly getSession: (request: GetImportSessionV2Request) => Promise<ImportSession>;
  readonly addItems: (request: AddImportItemsV2Request) => Promise<ImportSession>;
  readonly addPaths: (request: AddImportPathsV2Request) => Promise<BackendTask>;
  readonly addUrl: (request: import("./importV2Web").AddImportUrlV2Request) => Promise<ImportSession>;
  readonly setSelection: (request: SetImportItemSelectionV2Request) => Promise<ImportSession>;
  readonly startItems: (request: StartImportItemsV2Request) => Promise<BackendTask[]>;
  readonly confirmSession: (request: CommitImportSessionRequest) => Promise<BackendTask>;
  readonly getPreviewContent: (request: GetImportPreviewContentV2Request) => Promise<ImportPreviewContent>;
  readonly getReadiness: (request: GetImportFrontendReadinessV2Request) => Promise<ImportFrontendReadiness>;
  readonly listHistory: (request: ListImportHistoryV2Request) => Promise<ImportHistoryPage>;
  readonly authorizePrivateTarget: (request: AuthorizeImportPrivateTargetV2Request) => Promise<string>;
  readonly beginLogin: (request: BeginImportLoginV2Request) => Promise<ConnectorSessionRef>;
  readonly completeLogin: (request: CompleteImportLoginV2Request) => Promise<ConnectorSessionRef>;
  readonly revokeLogin: (request: RevokeImportLoginV2Request) => Promise<void>;
  readonly getCapabilityRequirement: (request: GetImportCapabilityRequirementV2Request) => Promise<ImportCapabilityRequirement>;
  readonly installCapability: (request: InstallImportCapabilityV2Request) => Promise<BackendTask>;
  readonly getAgentPolicy: (request: GetImportAgentPolicyV2Request) => Promise<AgentAssistancePolicy>;
  readonly setAgentPolicy: (request: SetImportAgentPolicyV2Request) => Promise<AgentAssistancePolicy>;
  readonly startAgentAssistance: (request: StartImportAgentAssistanceV2Request) => Promise<BackendTask>;
  readonly previewByokScope: (request: PreviewImportByokScopeV2Request) => Promise<AgentSendScope>;
  readonly approveByokAssistance: (request: ApproveImportByokAssistanceV2Request) => Promise<BackendTask>;
  readonly acceptAgentCandidate: (request: import("./importV2Agent").AcceptImportAgentCandidateRequest) => Promise<AgentCandidateView>;
  readonly selectAgentCandidate: (request: SelectImportAgentCandidateRequest) => Promise<AgentCandidateActionResult>;
  readonly discardAgentCandidate: (request: DiscardImportAgentCandidateRequest) => Promise<AgentCandidateActionResult>;
  readonly scanMigration: (request: ScanImportV2MigrationRequest) => Promise<LegacyInventory>;
  readonly planMigration: (request: PlanImportV2MigrationRequest) => Promise<MigrationPlan>;
  readonly applyMigration: (request: ApplyImportV2MigrationRequest) => Promise<MigrationApplyTask>;
  readonly getMigrationStatus: (request: MigrationProjectRequest) => Promise<MigrationStatusSnapshot>;
  readonly resumeMigration: (request: ResumeImportV2MigrationRequest) => Promise<MigrationApplyTask>;
  readonly activate: (request: ActivateImportV2Request) => Promise<ActivationResult>;
  readonly getActivation: (request: GetImportBackendActivationRequest) => Promise<ImportBackendActivation | null>;
};
