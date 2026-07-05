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
  pagePath: string;
  title: string;
  snippet?: string;
  score: number;
  isPinned?: boolean;
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
}

export interface ChatSession {
  id: string;
  title: string;
  projectId: string;
  createdAt: string;
  updatedAt: string;
  messages: ChatMessage[];
}

export interface ChatSessionSummary {
  id: string;
  title: string;
  createdAt: string;
  updatedAt: string;
  messageCount: number;
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
