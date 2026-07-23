import type { TaskProgress } from "./task";
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
export type ImportInputKind = "file" | "folder" | "url";
export type MediaSaveMode = "preserve_original" | "extract_only";
export type ImportStage = "inspect" | "route" | "extract" | "validate" | "commit";
export type QualityLevel = "pass" | "warning" | "fail";
export type ArtifactKind = "source_snapshot" | "source_evidence" | "markdown" | "image" | "attachment" | "subtitle" | "transcript" | "metadata";
export type AttemptOutcome = "succeeded" | "failed" | "cancelled";
export type ImportItemStatus = "queued" | "inspecting" | "waiting_capability" | "waiting_login" | "waiting_authorization" | "extracting" | "validating" | "preview_ready" | "needs_merge" | "committing" | "completed" | "paused" | "cancelled" | "skipped" | "failed";
export type ImportSessionStatus = "draft" | "processing" | "waiting_for_confirmation" | "partially_committed" | "completed" | "cancelled";

export interface ImportInput { kind: ImportInputKind; displayName: string; locator: string; normalizedLocator: string | null; mediaSaveMode?: MediaSaveMode; }
export interface QualityMetric { code: string; actual: number; minimum: number; passed: boolean; }
export interface QualityReport { level: QualityLevel; metrics: QualityMetric[]; warnings: string[]; sheetCountExact?: number; slideCountExact?: number; nonEmptyCellCoverage?: number; formulaValuePairs?: number; meaningfulImageCoverage?: number; }
export interface ImportArtifact { kind: ArtifactKind; relativePath: string; sha256: string; sizeBytes: number; }
export interface AttemptRecord { route: string; engineId: string; engineVersion: string; stage: ImportStage; startedAt: string; completedAt: string | null; outcome: AttemptOutcome; warnings: string[]; }
export type ImportRecoveryAction = "install_capability" | "retry" | "switch_parser" | "enable_ocr" | "invoke_agent" | "skip" | "view_log" | "retry_route" | "switch_route" | "begin_login" | "authorize_private_target" | "install_browser_capability" | "install_media_capability" | "install_ocr_capability" | "authorize_local_asr" | "preview_without_transcript";
export interface ImportIssue { code: string; message: string; stage: ImportStage; retryable: boolean; userActionRequired: boolean; recoveryActions: ImportRecoveryAction[]; availableActions: AgentRecoveryAction[]; }
export interface ImportPreviewArtifact { markdown: ImportArtifact; assets: ImportArtifact[]; sourceSnapshot: ImportArtifact; quality: QualityReport; title: string; }
export interface ImportItem { itemId: string; input: ImportInput; status: ImportItemStatus; selected: boolean; taskId: string | null; progress: TaskProgress | null; attempts: AttemptRecord[]; preview: ImportPreviewArtifact | null; issue: ImportIssue | null; }
export interface ImportSession { schemaVersion: 2; sessionId: string; projectId: string; status: ImportSessionStatus; resourceMode: ImportResourceMode; createdAt: string; updatedAt: string; discoveryTaskId?: string | null; items: ImportItem[]; }

export interface CreateImportSessionV2Request { projectId: string; projectRootPath: string; resourceMode: ImportResourceMode; }
export interface GetImportSessionV2Request { projectId: string; projectRootPath: string; sessionId: string; historyBatchId?: string | null; }
export interface AddImportItemsV2Request { projectId: string; projectRootPath: string; sessionId: string; inputs: ImportInput[]; }
export interface SetImportItemSelectionV2Request { projectId: string; projectRootPath: string; sessionId: string; itemId: string; selected: boolean; }
export interface StartImportItemsV2Request { projectId: string; projectRootPath: string; sessionId: string; itemIds: string[]; recoveryAction?: ImportRecoveryAction | null; }
export interface CancelImportBatchV2Request { projectId: string; projectRootPath: string; sessionId: string; batchId: string; }
export interface CancelImportItemV2Request { projectId: string; projectRootPath: string; sessionId: string; itemId: string; }

export type CommitConflictAction = "create_new" | "keep_wiki" | "apply_merged_candidate";
export interface CommitItemDecision { itemId: string; conflictAction: CommitConflictAction | null; expectedWikiHash: string | null; }
export interface CommitImportSessionRequest { projectId: string; projectRootPath: string; sessionId: string; batchTaskId?: string | null; decisions: CommitItemDecision[]; }
export interface ImportItemCommitResult { itemId: string; sourceId: string | null; versionId: string | null; wikiPath: string | null; committed: boolean; errorCode: string | null; }
export interface ImportBatchResult { batchId: string; sessionId: string; batchTaskId?: string | null; committedCount: number; failedCount: number; items: ImportItemCommitResult[]; }
