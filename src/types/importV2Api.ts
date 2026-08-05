import type {
  AddImportItemsV2Request,
  AddImportTextV2Request,
  CancelImportBatchV2Request,
  CancelImportItemV2Request,
  CommitImportSessionRequest,
  CreateImportSessionV2Request,
  GetImportSessionV2Request,
  ImportCompletion,
  ImportItem,
  ImportMergeContextV2Request,
  ImportSession,
  ImportThreeWayMergeContext,
  SetImportItemResolutionV2Request,
  SetImportItemSelectionV2Request,
  SelectImportSubtitleV2Request,
  StageImportManualMergeV2Request,
  StartImportItemsV2Request,
  StartImportBatchV2Request,
} from "./importV2";
import type {
  AcceptImportAgentCandidateRequest,
  AgentCandidateActionResult,
  AgentCandidateView,
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
  AcceptImportScanV2Request,
  AcceptImportScanV2Result,
  DiscardImportScanV2Request,
  FileScanResult,
  GetImportScanResultV2Request,
} from "./importV2File";
import type {
  ApplyImportV2MigrationRequest,
  LegacyInventory,
  MigrationApplyTask,
  MigrationPreparation,
  MigrationProjectRequest,
  MigrationStatusSnapshot,
  PlanImportV2MigrationRequest,
  ResumeImportV2MigrationRequest,
  ScanImportV2MigrationRequest,
} from "./importV2Migration";
import type {
  AuthorizeImportPrivateTargetV2Request,
  BeginImportLoginV2Request,
  CompleteImportLoginV2Request,
  CompleteImportLoginResult,
  ConnectorSessionRef,
  GetImportAsrEnablementPlanV2Request,
  GetImportCapabilityRequirementV2Request,
  GetImportFrontendReadinessV2Request,
  GetImportRestrictedContentStatusV2Request,
  GetImportPreviewContentV2Request,
  ImportCapabilityRequirement,
  ImportAsrEnablementPlan,
  ImportFrontendReadiness,
  ImportRestrictedContentStatus,
  ImportHistoryPage,
  ImportPreviewContent,
  ImportWorkbenchPreferences,
  ImportWorkbenchPreferencesRequest,
  InstallImportCapabilityV2Request,
  ListImportHistoryV2Request,
  RevokeImportLoginV2Request,
  SaveImportWorkbenchPreferencesRequest,
  StartImportAgentAssistanceV2Request,
} from "./importV2Presentation";
import type {
  AddImportCollectionItemsV2Request,
  AddImportUrlV2Request,
  AuthorizeLocalAsrV2Request,
  AuthorizeLocalOcrV2Request,
  ConfirmRemoteMediaRetentionV2Request,
  DiscoverImportCollectionV2Request,
  ImportCollectionPreview,
  ImportCollectionPage,
  LoadImportCollectionPageV2Request,
  RemoteMediaRetentionPlan,
  RemoteMediaRetentionRequest,
} from "./importV2Web";
import type { BackendTask } from "./task";

export interface ImportV2CommandNames {
  readonly createSession: "create_import_session_v2";
  readonly getSession: "get_import_session_v2";
  readonly getHistorySession: "get_import_history_session_v2";
  readonly getCompletion: "get_import_completion_v2";
  readonly addItems: "add_import_items_v2";
  readonly addPaths: "start_add_import_paths_v2";
  readonly addText: "add_import_text_v2";
  readonly getScanResult: "get_import_scan_result_v2";
  readonly acceptScan: "accept_import_scan_v2";
  readonly discardScan: "discard_import_scan_v2";
  readonly addUrl: "add_import_url_v2";
  readonly discoverCollection: "discover_import_collection_v2";
  readonly loadCollectionPage: "load_import_collection_page_v2";
  readonly addCollectionItems: "add_import_collection_items_v2";
  readonly getRemoteMediaRetentionPlan: "get_remote_media_retention_plan_v2";
  readonly confirmRemoteMediaRetention: "confirm_remote_media_retention_v2";
  readonly setSelection: "set_import_item_selection_v2";
  readonly selectSubtitle: "select_import_subtitle_v2";
  readonly getMergeContext: "get_import_merge_context_v2";
  readonly setItemResolution: "set_import_item_resolution_v2";
  readonly stageManualMerge: "stage_import_manual_merge_v2";
  readonly startItems: "start_import_items_v2";
  readonly startBatch: "start_import_batch_v2";
  readonly cancelBatch: "cancel_import_batch_v2";
  readonly cancelItem: "cancel_import_item_v2";
  readonly skipItem: "skip_import_item_v2";
  readonly authorizeLocalAsr: "authorize_local_asr_v2";
  readonly authorizeLocalOcr: "authorize_local_ocr_v2";
  readonly confirmSession: "confirm_import_session_v2";
  readonly getPreviewContent: "get_import_preview_content_v2";
  readonly getReadiness: "get_import_frontend_readiness_v2";
  readonly getWorkbenchPreferences: "get_import_workbench_preferences_v2";
  readonly saveWorkbenchPreferences: "save_import_workbench_preferences_v2";
  readonly getRestrictedContentStatus: "get_import_restricted_content_status_v2";
  readonly listHistory: "list_import_history_v2";
  readonly authorizePrivateTarget: "authorize_import_private_target_v2";
  readonly beginLogin: "begin_import_login_v2";
  readonly completeLogin: "complete_import_login_v2";
  readonly revokeLogin: "revoke_import_login_v2";
  readonly getCapabilityRequirement: "get_import_capability_requirement_v2";
  readonly getAsrEnablementPlan: "get_import_asr_enablement_plan_v2";
  readonly installCapability: "install_import_capability_v2";
  readonly startAgentAssistance: "start_import_agent_assistance_v2";
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
  readonly getCompletion: (request: GetImportSessionV2Request) => Promise<ImportCompletion | null>;
  readonly addItems: (request: AddImportItemsV2Request) => Promise<ImportSession>;
  readonly addPaths: (request: AddImportPathsV2Request) => Promise<BackendTask>;
  readonly addText: (request: AddImportTextV2Request) => Promise<ImportSession>;
  readonly getScanResult: (request: GetImportScanResultV2Request) => Promise<FileScanResult>;
  readonly acceptScan: (request: AcceptImportScanV2Request) => Promise<AcceptImportScanV2Result>;
  readonly discardScan: (request: DiscardImportScanV2Request) => Promise<FileScanResult>;
  readonly addUrl: (request: AddImportUrlV2Request) => Promise<ImportSession>;
  readonly discoverCollection: (request: DiscoverImportCollectionV2Request) => Promise<ImportCollectionPreview | null>;
  readonly loadCollectionPage: (request: LoadImportCollectionPageV2Request) => Promise<ImportCollectionPage>;
  readonly addCollectionItems: (request: AddImportCollectionItemsV2Request) => Promise<ImportSession>;
  readonly getRemoteMediaRetentionPlan: (request: RemoteMediaRetentionRequest) => Promise<RemoteMediaRetentionPlan>;
  readonly confirmRemoteMediaRetention: (request: ConfirmRemoteMediaRetentionV2Request) => Promise<ImportSession>;
  readonly setSelection: (request: SetImportItemSelectionV2Request) => Promise<ImportSession>;
  readonly selectSubtitle: (request: SelectImportSubtitleV2Request) => Promise<ImportSession>;
  readonly getMergeContext: (request: ImportMergeContextV2Request) => Promise<ImportThreeWayMergeContext>;
  readonly setItemResolution: (request: SetImportItemResolutionV2Request) => Promise<ImportItem>;
  readonly stageManualMerge: (request: StageImportManualMergeV2Request) => Promise<ImportItem>;
  readonly startItems: (request: StartImportItemsV2Request) => Promise<BackendTask[]>;
  readonly startBatch: (request: StartImportBatchV2Request) => Promise<BackendTask>;
  readonly cancelBatch: (request: CancelImportBatchV2Request) => Promise<BackendTask[]>;
  readonly cancelItem: (request: CancelImportItemV2Request) => Promise<ImportSession>;
  readonly skipItem: (request: CancelImportItemV2Request) => Promise<ImportSession>;
  readonly authorizeLocalAsr: (request: AuthorizeLocalAsrV2Request) => Promise<void>;
  readonly authorizeLocalOcr: (request: AuthorizeLocalOcrV2Request) => Promise<void>;
  readonly confirmSession: (request: CommitImportSessionRequest) => Promise<BackendTask>;
  readonly getPreviewContent: (request: GetImportPreviewContentV2Request) => Promise<ImportPreviewContent>;
  readonly getReadiness: (request: GetImportFrontendReadinessV2Request) => Promise<ImportFrontendReadiness>;
  readonly getWorkbenchPreferences: (request: ImportWorkbenchPreferencesRequest) => Promise<ImportWorkbenchPreferences>;
  readonly saveWorkbenchPreferences: (request: SaveImportWorkbenchPreferencesRequest) => Promise<ImportWorkbenchPreferences>;
  readonly getRestrictedContentStatus: (request: GetImportRestrictedContentStatusV2Request) => Promise<ImportRestrictedContentStatus>;
  readonly listHistory: (request: ListImportHistoryV2Request) => Promise<ImportHistoryPage>;
  readonly authorizePrivateTarget: (request: AuthorizeImportPrivateTargetV2Request) => Promise<string>;
  readonly beginLogin: (request: BeginImportLoginV2Request) => Promise<ConnectorSessionRef>;
  readonly completeLogin: (request: CompleteImportLoginV2Request) => Promise<CompleteImportLoginResult>;
  readonly revokeLogin: (request: RevokeImportLoginV2Request) => Promise<void>;
  readonly getCapabilityRequirement: (request: GetImportCapabilityRequirementV2Request) => Promise<ImportCapabilityRequirement>;
  readonly getAsrEnablementPlan: (request: GetImportAsrEnablementPlanV2Request) => Promise<ImportAsrEnablementPlan>;
  readonly installCapability: (request: InstallImportCapabilityV2Request) => Promise<BackendTask>;
  readonly startAgentAssistance: (request: StartImportAgentAssistanceV2Request) => Promise<BackendTask>;
  readonly acceptAgentCandidate: (request: AcceptImportAgentCandidateRequest) => Promise<AgentCandidateView>;
  readonly selectAgentCandidate: (request: SelectImportAgentCandidateRequest) => Promise<AgentCandidateActionResult>;
  readonly discardAgentCandidate: (request: DiscardImportAgentCandidateRequest) => Promise<AgentCandidateActionResult>;
  readonly scanMigration: (request: ScanImportV2MigrationRequest) => Promise<LegacyInventory>;
  readonly planMigration: (request: PlanImportV2MigrationRequest) => Promise<MigrationPreparation>;
  readonly applyMigration: (request: ApplyImportV2MigrationRequest) => Promise<MigrationApplyTask>;
  readonly getMigrationStatus: (request: MigrationProjectRequest) => Promise<MigrationStatusSnapshot>;
  readonly resumeMigration: (request: ResumeImportV2MigrationRequest) => Promise<MigrationApplyTask>;
  readonly activate: (request: ActivateImportV2Request) => Promise<ActivationResult>;
  readonly getActivation: (request: GetImportBackendActivationRequest) => Promise<ImportBackendActivation | null>;
}
