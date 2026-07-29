import type { QualityReport } from "./importV2";

export type SourceStatus = "current" | "candidate_ready" | "needs_attention";
import type { AgentKind } from "./agent";
import type { LlmProviderKind } from "./llm";

export type SourceCandidateKind = "ocr" | "asr" | "subtitle" | "refresh" | "ai_organize";
export type SourceAiOrganizeRoute = "agent" | "byok";
export type SourceEvidenceRetention = "immutable_originals_retained";
export type SourcePrimaryAction =
  | "review_candidate"
  | "reprocess_ocr"
  | "reprocess_asr"
  | "refresh_source"
  | "none";

export interface SourceBinding {
  sourceId: string;
  versionId: string;
  status: SourceStatus;
  quality: QualityReport;
}

export interface SourceArtifactSummary {
  path: string;
  kind: string;
  sizeBytes: number;
}

export interface SourceVersionSummary {
  versionId: string;
  createdAt: string;
  eventKind: string;
  quality: QualityReport;
  current: boolean;
  restorable: boolean;
  checkpoint: string | null;
}

export interface SourceTimelineItem {
  eventId: string;
  kind: string;
  versionId: string | null;
  createdAt: string;
  checkpoint: string | null;
  restorable: boolean;
}

export interface SourceCandidateSummary {
  candidateId: string;
  kind: SourceCandidateKind;
  createdAt: string;
  baseVersionId: string;
  baseMarkdownHash: string;
  candidateMarkdownHash: string;
  quality: QualityReport;
  aiOrganize?: {
    taskId: string;
    route: SourceAiOrganizeRoute;
    engine: string;
    model: string;
    engineVersion?: string | null;
  };
}

export interface StartSourceAiOrganizeInput {
  route: "auto" | "agent" | "byok";
  agent: AgentKind | null;
  provider: LlmProviderKind | null;
  customInstructions: string | null;
}

export interface SourceAiOrganizeBinding {
  sourceId: string;
  versionId: string;
  markdownHash: string;
}

export interface SourceDetail {
  sourceId: string;
  versionId: string;
  title: string;
  sourceKind: string;
  status: SourceStatus;
  currentPath: string;
  currentMarkdownHash: string;
  primaryAction: SourcePrimaryAction;
  candidate: SourceCandidateSummary | null;
  targetPath: string;
  evidenceRetention: SourceEvidenceRetention;
  evidence: SourceArtifactSummary[];
  quality: QualityReport;
  originalDraft: string;
  originalDraftTruncated: boolean;
  versions: SourceVersionSummary[];
  timeline: SourceTimelineItem[];
  relatedWikiPaths: string[];
  technicalDetails: {
    route: string;
    engine: string;
    engineVersion: string;
    locator: string;
    manifestPath: string;
  };
  availableActions: SourceCandidateKind[];
}

export interface SourceUpdatePreview {
  sourceId: string;
  candidateId: string;
  mode: "two_way" | "three_way";
  baseMarkdown: string;
  currentMarkdown: string;
  candidateMarkdown: string;
  diff: string;
  currentMarkdownHash: string;
  candidateMarkdownHash: string;
  guardToken: string;
}

export interface SourceMutationResult {
  sourceId: string;
  versionId: string;
  wikiPath: string;
  checkpoint: string | null;
}

export interface MoveSourcePreview {
  sourceId: string;
  oldWikiPath: string;
  newWikiPath: string;
  affectedPaths: string[];
  guardToken: string;
}

export interface DeleteSourcePreview {
  sourceId: string;
  title: string;
  paths: SourceArtifactSummary[];
  versions: SourceVersionSummary[];
  referencedBy: string[];
  referenceCount: number;
  expectedFreedBytes: number;
  guardToken: string;
}
