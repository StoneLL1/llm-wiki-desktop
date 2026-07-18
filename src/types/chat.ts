import type { AgentKind } from "./agent";
import type { LlmProviderKind } from "./llm";

export type ChatRole = "user" | "assistant";
export type ChatRoute = "agent" | "byok";
export type ChatRoutePreference = "auto" | "agent" | "byok";
export type ChatConvenienceEditStatus =
  | "applied"
  | "soft_violation_pending"
  | "kept_after_soft_violation"
  | "rolled_back"
  | "rollback_failed";

export interface ChatCitation {
  sourceId?: string | null;
  pagePath: string;
  title: string;
  snippet?: string;
  score: number;
  isPinned?: boolean;
}

export interface ChatRetrievalHit {
  path: string;
  title: string;
  snippet?: string | null;
  score: number;
  excerpt?: string | null;
  isPinned?: boolean;
}

export type ChatSourceSelectionReason =
  | "index"
  | "pinned"
  | "keyword_hit"
  | "graph_neighbor"
  | "source_overlap";

export interface ChatExpandedPage {
  path: string;
  reason: ChatSourceSelectionReason;
}

export interface ChatRetrievalDiagnostics {
  route: ChatRoute;
  retrievalHits?: ChatRetrievalHit[];
  expandedPages?: ChatExpandedPage[];
  selectedPages?: string[];
  omittedPages?: string[];
  budgetChars: number;
  sourceBudgetChars: number;
  historyBudgetChars: number;
  invalidCitationIds?: string[];
  hasUnverified?: boolean;
}

export interface ChatMessage {
  id: string;
  role: ChatRole;
  content: string;
  createdAt: string;
  citations?: ChatCitation[];
  route?: ChatRoute;
  /** BYOK provider that produced this answer (absent for Agent route). */
  provider?: LlmProviderKind | null;
  taskId?: string;
  convenienceEdit?: ChatConvenienceEdit | null;
  retrievalDiagnostics?: ChatRetrievalDiagnostics | null;
  /** Relative query-page path after Save to Wiki succeeds. */
  savedPath?: string | null;
}

export interface ChatConvenienceEdit {
  status: ChatConvenienceEditStatus;
  checkpointHash?: string | null;
  affectedPaths: string[];
  diffSummary: string;
  diffText?: string | null;
  violationReason?: string | null;
  rollbackTaskId?: string | null;
  ignoredBaselinePaths?: string[];
  affectedPathHashes?: Array<{ path: string; hash: string | null }>;
}

export interface ChatSession {
  id: string;
  title: string;
  projectId: string;
  createdAt: string;
  updatedAt: string;
  messages: ChatMessage[];
  /** Wiki page this session is scoped to (Wiki "Ask AI" sidebar). Absent for
   *  global Chat-view sessions. Persisted as typed metadata on the session
   *  JSON, never as a separate database. */
  contextPagePath?: string | null;
}

export interface ChatSessionSummary {
  id: string;
  title: string;
  createdAt: string;
  updatedAt: string;
  messageCount: number;
  /** Mirrors {@link ChatSession.contextPagePath} so the session list can group
   *  page-scoped chats without loading each full session. */
  contextPagePath?: string | null;
}

export interface SaveAnswerResult {
  path: string;
  created: boolean;
  checkpoint?: string;
}

export interface SendChatMessageRequest {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  content: string;
  route: ChatRoutePreference;
  agent?: AgentKind | null;
  provider?: LlmProviderKind | null;
  pinnedPagePath?: string | null;
  convenienceEnabled?: boolean;
}

export interface SaveAnswerToWikiRequest {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  messageId: string;
  targetPath?: string | null;
  expectedHash?: string | null;
  allowOverwrite: boolean;
  actionId?: string | null;
}

/** Inline overwrite confirm surfaced when saving would overwrite an existing page. */
export interface ChatOverwriteRequest {
  sessionId: string;
  messageId: string;
  path: string;
  currentHash: string;
  actionId: string;
}

export interface ResolveChatConvenienceEditRequest {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  messageId: string;
  keep: boolean;
}

export interface RollbackLastChatConvenienceEditRequest {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
}
