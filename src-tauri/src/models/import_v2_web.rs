use serde::{Deserialize, Serialize};

use super::import_v2::MediaSaveMode;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedWebUrl {
    pub public_url: String,
    pub host: String,
    pub scheme: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WebRouteKind {
    GenericHttp,
    GenericBrowser,
    Wechat,
    Zhihu,
    Bilibili,
    Xiaohongshu,
    Douyin,
    X,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RedirectDecision {
    Allowed,
    PrivateAuthorizationRequired,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WebContentKind {
    Article,
    Video,
    Note,
    Post,
    Challenge,
    Login,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct WebMetadata {
    pub title: String,
    pub author: Option<String>,
    pub published_at: Option<String>,
    pub public_url: String,
    pub fetched_at: String,
    #[serde(default)]
    pub images: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WebAuthState {
    Public,
    WaitingLogin,
    Authenticated,
    CaptchaRequired,
    Revoked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WebRecoveryAction {
    RetryRoute,
    SwitchRoute,
    BeginLogin,
    AuthorizePrivateTarget,
    InstallBrowserCapability,
    InstallMediaCapability,
    AuthorizeLocalAsr,
    InvokeAgent,
    Skip,
    ViewLog,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AddImportUrlV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub url: String,
    #[serde(default)]
    pub media_save_mode: MediaSaveMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverImportCollectionV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportCollectionItemPreview {
    pub item_ref: String,
    pub title: String,
    pub public_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportCollectionPreview {
    pub task_id: String,
    pub collection_ref: String,
    pub source_url: String,
    pub platform: String,
    pub title: String,
    pub total_duration_seconds: Option<u64>,
    pub estimated_login_count: usize,
    pub estimated_asr_count: usize,
    pub discovered_total: usize,
    pub loaded_count: usize,
    pub has_more: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub items: Vec<ImportCollectionItemPreview>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LoadImportCollectionPageV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub collection_ref: String,
    pub cursor: String,
    #[serde(default)]
    pub load_all: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportCollectionPage {
    pub items: Vec<ImportCollectionItemPreview>,
    pub discovered_total: usize,
    pub loaded_count: usize,
    pub has_more: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AddImportCollectionItemsV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub collection_ref: String,
    pub item_refs: Vec<String>,
    #[serde(default)]
    pub media_save_mode: MediaSaveMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteMediaRetentionRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub item_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmRemoteMediaRetentionV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub item_id: String,
    pub acknowledge_size_and_disk: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteMediaRetentionPlan {
    pub item_id: String,
    pub estimated_bytes: u64,
    pub available_disk_bytes: Option<u64>,
    pub enough_disk: Option<bool>,
    pub quality: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizeLocalAsrV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub item_id: String,
    #[serde(default)]
    pub profile: crate::models::import_v2::ImportAsrProfile,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

/// Compatibility alias for sessions created before local ASR was generalized
/// from Bilibili to every supported media platform.
pub type AuthorizeBilibiliAsrV2Request = AuthorizeLocalAsrV2Request;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizeLocalOcrV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub item_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WebImportErrorCode {
    UrlRejected,
    PrivateTargetBlocked,
    RedirectRejected,
    TlsFailed,
    ResponseTooLarge,
    ChallengeDetected,
    LoginRequired,
    CaptchaRequired,
    ContentRemoved,
    StructureChanged,
    SubtitleUnavailable,
    ConnectorRateLimited,
}
