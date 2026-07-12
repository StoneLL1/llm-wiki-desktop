export type WebRouteKind = "generic_http" | "generic_browser" | "wechat" | "zhihu" | "bilibili" | "xiaohongshu" | "x";
export type RedirectDecision = "allowed" | "private_authorization_required" | "rejected";
export type WebContentKind = "article" | "video" | "note" | "post" | "challenge" | "login";
export type WebAuthState = "public" | "waiting_login" | "authenticated" | "captcha_required" | "revoked";
export type WebRecoveryAction = "retry_route" | "switch_route" | "begin_login" | "authorize_private_target" | "install_browser_capability" | "install_media_capability" | "invoke_agent" | "skip" | "view_log";
export interface NormalizedWebUrl { publicUrl: string; host: string; scheme: "http" | "https"; }
export interface WebMetadata { title: string; author: string | null; publishedAt: string | null; publicUrl: string; fetchedAt: string; images: string[]; }
export interface AddImportUrlV2Request { projectId: string; projectRootPath: string; sessionId: string; url: string; }
