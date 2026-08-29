import type { TaskProgress, TaskStatus } from "./task";
import type { AgentRecoveryAction } from "./importV2Agent";

export type {
  AcceptImportAgentCandidateRequest,
  AgentCandidate,
  AgentCandidateActionResult,
  AgentCandidateDiff,
  AgentCandidateView,
  DiscardImportAgentCandidateRequest,
  SelectImportAgentCandidateRequest,
} from "./importV2Agent";

export type ImportResourceMode = "balanced" | "performance" | "saver";
export type ImportInputKind = "file" | "folder" | "url" | "clipboard_text";
export type MediaSaveMode = "preserve_original" | "extract_only";
export type ImportStage = "inspect" | "route" | "extract" | "validate" | "commit";
export type QualityLevel = "pass" | "warning" | "fail";
export type ArtifactKind = "source_snapshot" | "source_evidence" | "markdown" | "image" | "attachment" | "subtitle" | "transcript" | "metadata";
export type AttemptOutcome = "succeeded" | "failed" | "cancelled";
export type ImportItemStatus = "queued" | "inspecting" | "waiting_capability" | "waiting_login" | "waiting_authorization" | "extracting" | "validating" | "preview_ready" | "needs_merge" | "committing" | "completed" | "paused" | "cancelled" | "skipped" | "failed";
export type ImportSessionStatus = "draft" | "processing" | "waiting_for_confirmation" | "partially_committed" | "completed" | "cancelled";
export type ImportUserState = "discovering" | "processing" | "needs_action" | "ready" | "committing" | "committed" | "failed";

export type ImportItemResolution =
  | { kind: "new_source" }
  | { kind: "exact_duplicate_skip"; sourceId: string; candidateHash: string; currentHash: string; targetVersionId: string }
  | { kind: "same_source_new_version"; sourceId: string; candidateHash: string; currentHash: string; targetVersionId: string }
  | { kind: "keep_current_source"; sourceId: string; candidateHash: string; currentHash: string; targetVersionId: string }
  | { kind: "apply_import_candidate"; sourceId: string; candidateHash: string; currentHash: string; targetVersionId: string }
  | { kind: "manual_merge"; sourceId: string; candidateHash: string; currentHash: string; targetVersionId: string; mergedHash: string };

export type ImportResolutionKind =
  | "new_source"
  | "exact_duplicate"
  | "same_source_new_version"
  | "needs_three_way_merge";

export interface ImportResolutionBinding {
  sourceId: string;
  candidateHash: string;
  currentHash: string;
  targetVersionId: string;
}

export interface ImportResolutionContext {
  kind: ImportResolutionKind;
  binding?: ImportResolutionBinding;
  defaultResolution?: ImportItemResolution;
  targetWikiPath?: string;
}

export interface ImportThreeWayMergeContext {
  resolution: ImportResolutionContext;
  baselineMarkdown: string;
  currentMarkdown: string;
  candidateMarkdown: string;
}

export type ImportPrimaryAction =
  | "retry"
  | "sign_in"
  | "authorize"
  | "install_capability"
  | "enable_ocr"
  | "authorize_local_asr"
  | "invoke_local_agent"
  | "review"
  | "resolve"
  | "resume";

export interface ImportIssueDiagnostics {
  technicalCode?: string;
  technicalMessage?: string;
  route?: string;
  engineId?: string;
  artifactPath?: string;
  contentHash?: string;
}

export interface UserIssue {
  code: string;
  title: string;
  dataSafety: string;
  primaryAction: ImportPrimaryAction | null;
  detail?: ImportIssueDiagnostics;
}

export interface SourceVersionChange {
  sourceId: string;
  versionId: string;
  wikiPath: string;
  contentHash: string;
}

export interface DuplicateResult {
  itemId: string;
  sourceId: string;
  versionId: string;
  contentHash: string;
}

export interface ItemFailure {
  itemId: string;
  inputLabel: string;
  issue: UserIssue;
}

export interface ImportCompletion {
  sessionId: string;
  batchId: string;
  newSources: SourceVersionChange[];
  updatedSources: SourceVersionChange[];
  duplicateSkips: DuplicateResult[];
  warnings: UserIssue[];
  failures: ItemFailure[];
}

export interface SourceIdentity {
  canonicalPath: string;
  sizeBytes: number;
  modifiedNanos: number | null;
  fileId: string | null;
  sha256: string;
  magic: string;
}

export interface ImportInput {
  kind: ImportInputKind;
  displayName: string;
  locator: string;
  normalizedLocator: string | null;
  sourceIdentity?: SourceIdentity | null;
  mediaSaveMode?: MediaSaveMode;
}
export interface AddImportTextV2Request {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  sourceName: string;
  content: string;
}
export interface QualityMetric { code: string; actual: number; minimum: number; passed: boolean; }
export interface QualityReport { level: QualityLevel; metrics: QualityMetric[]; warnings: string[]; sheetCountExact?: number; slideCountExact?: number; nonEmptyCellCoverage?: number; formulaValuePairs?: number; meaningfulImageCoverage?: number; }
export interface SourceFrontmatter {
  type: "source";
  sourceId: string;
  versionId: string;
  sourceKind: string;
  title: string;
  importedAt: string;
  contentHash: string;
  platform?: string;
  canonicalUrl?: string;
  platformContentId?: string;
  author?: string;
  publishedAt?: string;
  language?: string;
  quality: QualityReport;
  restricted: boolean;
}
export interface SourceAlias {
  kind: string;
  value: string;
  createdAt: string;
}
export interface SourceArtifactRecord {
  path: string;
  sha256: string;
  sizeBytes: number;
  kind: string;
}
export interface SourceCandidateRecord {
  markdownHash: string;
  title: string;
  sourceKind: string;
  canonicalUrl?: string;
  platform?: string;
  platformContentId?: string;
  author?: string;
  publishedAt?: string;
  language?: string;
}
export interface SourceProvenance {
  locator: string;
  route: string;
  engineId: string;
  engineVersion: string;
}
export interface SourceVersion {
  versionId: string;
  contentHash: string;
  rawEvidence: SourceArtifactRecord[];
  assets: SourceArtifactRecord[];
  baselinePath: string;
  candidate: SourceCandidateRecord;
  provenance: SourceProvenance;
  quality: QualityReport;
  createdAt: string;
  humanEditHash?: string;
  checkpoint?: string;
}
export interface CompiledConsumption {
  versionId: string;
  contentHash: string;
  compileTaskId: string;
  consumedAt: string;
}
export interface SourceTimelineEvent {
  eventId: string;
  kind: string;
  versionId?: string;
  createdAt: string;
  checkpoint?: string;
}
export interface SourceManifest {
  schemaVersion: 3;
  sourceId: string;
  sourceKind: string;
  currentVersionId: string;
  wikiPath: string;
  aliases: SourceAlias[];
  origins: string[];
  canonicalUrl?: string;
  platform?: string;
  platformContentId?: string;
  title: string;
  author?: string;
  publishedAt?: string;
  importedAt: string;
  language?: string;
  versions: SourceVersion[];
  compiledConsumptions: CompiledConsumption[];
  restrictedContent: boolean;
  restrictedIdentitySummary?: string;
  timeline: SourceTimelineEvent[];
}
export interface ImportArtifact { kind: ArtifactKind; relativePath: string; sha256: string; sizeBytes: number; }
export interface AttemptRecord { route: string; engineId: string; engineVersion: string; stage: ImportStage; startedAt: string; completedAt: string | null; outcome: AttemptOutcome; errorCode?: string | null; warnings: string[]; }
export type ImportRecoveryAction = "install_capability" | "retry" | "switch_parser" | "enable_ocr" | "invoke_agent" | "skip" | "view_log" | "retry_route" | "switch_route" | "begin_login" | "authorize_private_target" | "install_browser_capability" | "install_media_capability" | "install_ocr_capability" | "authorize_local_asr" | "select_subtitle";
export interface ImportIssue { code: string; message: string; stage: ImportStage; retryable: boolean; userActionRequired: boolean; recoveryActions: ImportRecoveryAction[]; availableActions: AgentRecoveryAction[]; subtitleCandidates?: string[]; }
export interface ImportPreviewArtifact {
  markdown: ImportArtifact;
  assets: ImportArtifact[];
  sourceSnapshot: ImportArtifact;
  quality: QualityReport;
  title: string;
  resolution?: ImportResolutionContext;
  manualMerge?: ImportArtifact;
}
export interface ImportItem { itemId: string; itemRevision?: number; input: ImportInput; status: ImportItemStatus; selected: boolean; selectedSubtitle?: string | null; taskId: string | null; progress: TaskProgress | null; attempts: AttemptRecord[]; preview: ImportPreviewArtifact | null; issue: ImportIssue | null; authenticatedRetry?: boolean; authenticatedIdentitySummary?: string | null; restrictedContent?: boolean; restrictedIdentitySummary?: string | null; }
export type ImportMediaAuthorizationKind = "ocr" | "asr";
export type ImportAsrProfile = "fast" | "balanced" | "accurate";
export interface ImportMediaAuthorization { itemId: string; kind: ImportMediaAuthorizationKind; authorizedAt: string; asrProfile?: ImportAsrProfile | null; language?: string | null; }
export interface ImportCollectionChildRelation { itemId: string; canonicalUrl: string; discoveryFingerprint: string; }
export interface ImportCollectionRelation { relationId: string; sourceUrl: string; platform: string; title: string; childItemIds: string[]; children?: ImportCollectionChildRelation[]; addedAt: string; }
export interface ImportSession { schemaVersion: 2; sessionId: string; projectId: string; status: ImportSessionStatus; resourceMode: ImportResourceMode; createdAt: string; updatedAt: string; discoveryTaskId?: string | null; mediaAuthorizations?: ImportMediaAuthorization[]; collectionRelations?: ImportCollectionRelation[]; items: ImportItem[]; }
export type ImportSessionIndexState = "ready" | "rebuild_required";
export interface ImportSessionCounts { all: number; active: number; ready: number; needsAction: number; failed: number; completed: number; waiting: number; processed: number; cancelled: number; }
export interface ImportSelectionSummary { selected: number; newSources: number; updates: number; warnings: number; pending: number; restricted: number; }
export type ImportSessionActionGroupKind = "login" | "ocr" | "asr" | "capability" | "conflict" | "resume";
export interface ImportSessionActionGroup { groupKey: string; kind: ImportSessionActionGroupKind; subjectId?: string | null; itemCount: number; itemIds: string[]; }
export interface ImportSessionStatusCounts { queued: number; inspecting: number; waitingCapability: number; waitingLogin: number; waitingAuthorization: number; extracting: number; validating: number; previewReady: number; needsMerge: number; committing: number; completed: number; paused: number; cancelled: number; skipped: number; failed: number; }
export interface ImportOperationTaskSummary { taskId: string; status: TaskStatus; progress?: TaskProgress | null; }
export interface ImportSessionOverview { schemaVersion: 2; sessionId: string; projectId: string; status: ImportSessionStatus; resourceMode: ImportResourceMode; createdAt: string; updatedAt: string; discoveryTaskId?: string | null; itemCount: number; semanticRevision: number; selectionRevision: number; confirmationDigest: string; counts: ImportSessionCounts; statusCounts: ImportSessionStatusCounts; selection: ImportSelectionSummary; indexState: ImportSessionIndexState; actionGroups?: ImportSessionActionGroup[]; unresolvedCount?: number; remainingCount?: number; operationTask?: ImportOperationTaskSummary | null; nextCursor?: string | null; }
export type ImportItemPageFilter = "all" | "active" | "ready" | "needs_action" | "failed" | "completed";
export interface ImportItemPage { sessionId: string; snapshotRevision: number; items: ImportItem[]; nextCursor?: string | null; total: number; }

export interface CreateImportSessionV2Request { projectId: string; projectRootPath: string; resourceMode: ImportResourceMode; }
export interface GetImportSessionV2Request { projectId: string; projectRootPath: string; sessionId: string; historyBatchId?: string | null; }
export interface ListImportSessionItemsV2Request { projectId: string; projectRootPath: string; sessionId: string; filter: ImportItemPageFilter; cursor?: string | null; limit?: number; }
export interface AddImportItemsV2Request { projectId: string; projectRootPath: string; sessionId: string; inputs: ImportInput[]; }
export interface SetImportItemSelectionV2Request { projectId: string; projectRootPath: string; sessionId: string; itemId: string; selected: boolean; }
export interface SelectImportSubtitleV2Request { projectId: string; projectRootPath: string; sessionId: string; itemId: string; fileName: string; }
export interface StartImportItemsV2Request { projectId: string; projectRootPath: string; sessionId: string; itemIds: string[]; recoveryAction?: ImportRecoveryAction | null; }
export type StartImportBatchV2Request = StartImportItemsV2Request;
export interface CancelImportBatchV2Request { projectId: string; projectRootPath: string; sessionId: string; batchId: string; }
export interface CancelImportItemV2Request { projectId: string; projectRootPath: string; sessionId: string; itemId: string; }
export interface ImportMergeContextV2Request { projectId: string; projectRootPath: string; sessionId: string; itemId: string; }
export interface SetImportItemResolutionV2Request extends ImportMergeContextV2Request { resolution: ImportItemResolution; }
export interface StageImportManualMergeV2Request extends ImportMergeContextV2Request { mergedMarkdown: string; }

export interface CommitItemDecision { itemId: string; resolution?: ImportItemResolution | null; }
export interface CommitImportSessionRequest { projectId: string; projectRootPath: string; sessionId: string; batchTaskId?: string | null; acknowledgeRestrictedContent?: boolean; expectedSelectionRevision?: number | null; expectedConfirmationDigest?: string | null; decisions?: CommitItemDecision[]; }
export type ImportItemCommitResult =
  | { itemId: string; sourceId: string; versionId: string; wikiPath: string; committed: true; errorCode: null }
  | { itemId: string; sourceId: null; versionId: null; wikiPath: null; committed: false; errorCode: string };
export interface ImportBatchResult { batchId: string; sessionId: string; batchTaskId?: string | null; committedCount: number; failedCount: number; items: ImportItemCommitResult[]; }

export interface ImportSessionPatchCounts {
  total: number;
  processed: number;
  succeeded: number;
  waiting: number;
  failed: number;
  cancelled: number;
}

export interface ImportSessionPatchEvent {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  batchId: string;
  items: ImportItem[];
  counts: ImportSessionPatchCounts;
}
