import type { ImportAsrProfile } from "./importV2";

export type WebRouteKind = "generic_http" | "generic_browser" | "wechat" | "zhihu" | "bilibili" | "xiaohongshu" | "douyin" | "x";
export type RedirectDecision = "allowed" | "private_authorization_required" | "rejected";
export type WebContentKind = "article" | "video" | "note" | "post" | "challenge" | "login";
export type WebAuthState = "public" | "waiting_login" | "authenticated" | "captcha_required" | "revoked";
export type WebRecoveryAction = "retry_route" | "switch_route" | "begin_login" | "authorize_private_target" | "install_browser_capability" | "install_media_capability" | "authorize_local_asr" | "invoke_agent" | "skip" | "view_log";
export interface NormalizedWebUrl { publicUrl: string; host: string; scheme: "http" | "https"; }
export interface WebMetadata { title: string; author: string | null; publishedAt: string | null; publicUrl: string; fetchedAt: string; images: string[]; }
export type MediaSaveMode = "preserve_original" | "extract_only";
export interface AddImportUrlV2Request { projectId: string; projectRootPath: string; sessionId: string; url: string; mediaSaveMode?: MediaSaveMode; }
export interface DiscoverImportCollectionV2Request { projectId: string; projectRootPath: string; sessionId: string; url: string; }
export interface ImportCollectionItemPreview { itemRef: string; title: string; publicUrl: string; }
export interface ImportCollectionPage { discoveredTotal: number; loadedCount: number; hasMore: boolean; nextCursor: string | null; items: ImportCollectionItemPreview[]; }
export interface ImportCollectionPreview extends ImportCollectionPage { taskId: string; collectionRef: string; sourceUrl: string; platform: string; title: string; totalDurationSeconds: number | null; estimatedLoginCount: number; estimatedAsrCount: number; }
export interface LoadImportCollectionPageV2Request { projectId: string; projectRootPath: string; sessionId: string; collectionRef: string; cursor: string; loadAll?: boolean; }
export interface AddImportCollectionItemsV2Request { projectId: string; projectRootPath: string; sessionId: string; collectionRef: string; itemRefs: string[]; mediaSaveMode?: MediaSaveMode; }
export interface RemoteMediaRetentionRequest { projectId: string; projectRootPath: string; sessionId: string; itemId: string; }
export interface ConfirmRemoteMediaRetentionV2Request extends RemoteMediaRetentionRequest { acknowledgeSizeAndDisk: boolean; }
export interface RemoteMediaRetentionPlan { itemId: string; estimatedBytes: number; availableDiskBytes: number | null; enoughDisk: boolean | null; quality: "best_available"; }
export interface CompleteImportLoginV2Request { projectId: string; projectRootPath: string; importSessionId: string; itemId: string; connectorSessionId: string; }
export interface AuthorizePrivateTargetV2Request { projectId: string; projectRootPath: string; sessionId: string; itemId: string; url: string; }
export interface AuthorizeLocalAsrV2Request { projectId: string; projectRootPath: string; sessionId: string; itemId: string; profile: ImportAsrProfile; language?: string | null; }
export interface AuthorizeLocalOcrV2Request { projectId: string; projectRootPath: string; sessionId: string; itemId: string; }
export type AuthorizeBilibiliAsrV2Request = AuthorizeLocalAsrV2Request;
