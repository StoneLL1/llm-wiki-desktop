import type {
  AddImportItemsV2Request,
  CancelImportBatchV2Request,
  CancelImportItemV2Request,
  CommitImportSessionRequest,
  CreateImportSessionV2Request,
  GetImportSessionV2Request,
  ImportSession,
  SetImportItemSelectionV2Request,
  StartImportItemsV2Request,
} from "./importV2";
import type {
  AcceptImportAgentCandidateRequest,
  AgentAssistancePolicy,
  AgentCandidateActionResult,
  AgentCandidateView,
  AgentSendScope,
  DiscardImportAgentCandidateRequest,
  SelectImportAgentCandidateRequest,
} from "./importV2Agent";
import type {
  ActivateImportV2Request,
  ActivationResult,
  GetImportBackendActivationRequest,
  ImportBackendActivation,
} from "./importV2Activation";
import type {
  AddImportPathsV2Request,
  FileScanResult,
  GetImportScanResultV2Request,
} from "./importV2File";
import type {
  ApplyImportV2MigrationRequest,
  LegacyInventory,
  MigrationApplyTask,
  MigrationPlan,
  MigrationProjectRequest,
  MigrationStatusSnapshot,
  PlanImportV2MigrationRequest,
  ResumeImportV2MigrationRequest,
  ScanImportV2MigrationRequest,
} from "./importV2Migration";
import type {
  ApproveImportByokAssistanceV2Request,
  AuthorizeImportPrivateTargetV2Request,
  BeginImportLoginV2Request,
  CompleteImportLoginV2Request,
  ConnectorSessionRef,
  GetImportAgentPolicyV2Request,
  GetImportCapabilityRequirementV2Request,
  GetImportFrontendReadinessV2Request,
  GetImportPreviewContentV2Request,
  ImportCapabilityRequirement,
  ImportFrontendReadiness,
  ImportHistoryPage,
  ImportPreviewContent,
  InstallImportCapabilityV2Request,
  ListImportHistoryV2Request,
  PreviewImportByokScopeV2Request,
  RevokeImportLoginV2Request,
  SetImportAgentPolicyV2Request,
  StartImportAgentAssistanceV2Request,
} from "./importV2Presentation";
import type { AddImportUrlV2Request, AuthorizeLocalAsrV2Request } from "./importV2Web";
import type { BackendTask } from "./task";

export interface ImportV2CommandNames {
  readonly createSession: "create_import_session_v2";
  readonly getSession: "get_import_session_v2";
  readonly getHistorySession: "get_import_history_session_v2";
  readonly addItems: "add_import_items_v2";
  readonly addPaths: "start_add_import_paths_v2";
  readonly getScanResult: "get_import_scan_result_v2";
  readonly addUrl: "add_import_url_v2";
  readonly setSelection: "set_import_item_selection_v2";
  readonly startItems: "start_import_items_v2";
  readonly cancelBatch: "cancel_import_batch_v2";
  readonly cancelItem: "cancel_import_item_v2";
  readonly skipItem: "skip_import_item_v2";
  readonly authorizeLocalAsr: "authorize_local_asr_v2";
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

export interface ImportV2Api {
  readonly commandNames: ImportV2CommandNames;
  readonly createSession: (request: CreateImportSessionV2Request) => Promise<ImportSession>;
  readonly getSession: (request: GetImportSessionV2Request) => Promise<ImportSession>;
  readonly getHistorySession: (request: GetImportSessionV2Request) => Promise<ImportSession>;
  readonly addItems: (request: AddImportItemsV2Request) => Promise<ImportSession>;
  readonly addPaths: (request: AddImportPathsV2Request) => Promise<BackendTask>;
  readonly getScanResult: (request: GetImportScanResultV2Request) => Promise<FileScanResult>;
  readonly addUrl: (request: AddImportUrlV2Request) => Promise<ImportSession>;
  readonly setSelection: (request: SetImportItemSelectionV2Request) => Promise<ImportSession>;
  readonly startItems: (request: StartImportItemsV2Request) => Promise<BackendTask[]>;
  readonly cancelBatch: (request: CancelImportBatchV2Request) => Promise<BackendTask[]>;
  readonly cancelItem: (request: CancelImportItemV2Request) => Promise<ImportSession>;
  readonly skipItem: (request: CancelImportItemV2Request) => Promise<ImportSession>;
  readonly authorizeLocalAsr: (request: AuthorizeLocalAsrV2Request) => Promise<void>;
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
  readonly acceptAgentCandidate: (request: AcceptImportAgentCandidateRequest) => Promise<AgentCandidateView>;
  readonly selectAgentCandidate: (request: SelectImportAgentCandidateRequest) => Promise<AgentCandidateActionResult>;
  readonly discardAgentCandidate: (request: DiscardImportAgentCandidateRequest) => Promise<AgentCandidateActionResult>;
  readonly scanMigration: (request: ScanImportV2MigrationRequest) => Promise<LegacyInventory>;
  readonly planMigration: (request: PlanImportV2MigrationRequest) => Promise<MigrationPlan>;
  readonly applyMigration: (request: ApplyImportV2MigrationRequest) => Promise<MigrationApplyTask>;
  readonly getMigrationStatus: (request: MigrationProjectRequest) => Promise<MigrationStatusSnapshot>;
  readonly resumeMigration: (request: ResumeImportV2MigrationRequest) => Promise<MigrationApplyTask>;
  readonly activate: (request: ActivateImportV2Request) => Promise<ActivationResult>;
  readonly getActivation: (request: GetImportBackendActivationRequest) => Promise<ImportBackendActivation | null>;
}
