import type { AgentKind } from "./agent";
import type { LlmProviderKind } from "./llm";

export type ChatRole = "user" | "assistant";
export type ChatRoute = "agent" | "byok";
export type ChatRoutePreference = "auto" | "agent" | "byok";

export interface ChatCitation {
  pagePath: string;
  title: string;
  snippet?: string;
  score: number;
}

export interface ChatMessage {
  id: string;
  role: ChatRole;
  content: string;
  createdAt: string;
  citations?: ChatCitation[];
  route?: ChatRoute;
  taskId?: string;
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
