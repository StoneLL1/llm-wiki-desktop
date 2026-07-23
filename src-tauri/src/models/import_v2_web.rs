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
pub struct AuthorizeLocalAsrV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub item_id: String,
}

/// Compatibility alias for sessions created before local ASR was generalized
/// from Bilibili to every supported media platform.
pub type AuthorizeBilibiliAsrV2Request = AuthorizeLocalAsrV2Request;

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
