use serde::{Deserialize, Serialize};

use super::ConnectorFailure;
use crate::services::import_v2::{
    media_router::{
        MediaInput, MediaKind, MediaRoutePlan, MediaRouter, SubtitleCandidate, SubtitleKind,
    },
    url_policy::UrlPolicy,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct LocalAsrPolicy {
    pub capability_available: bool,
    pub user_authorized: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BilibiliDocument {
    pub title: String,
    pub author: String,
    pub published_at: Option<String>,
    pub description: String,
    pub cover_public_url: Option<String>,
    pub public_url: String,
    pub chapters: Vec<String>,
    #[serde(skip)]
    pub subtitle_requests: Vec<String>,
}

pub fn extract_json(
    json: &str,
    url: &str,
    asr: LocalAsrPolicy,
) -> Result<(BilibiliDocument, MediaRoutePlan), ConnectorFailure> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|_| ConnectorFailure::StructureChanged)?;
    if value["code"].as_i64() == Some(-404) {
        return Err(ConnectorFailure::Removed);
    }
    if value["loginRequired"].as_bool() == Some(true) {
        return Err(ConnectorFailure::LoginRequired);
    }
    let data = &value["data"];
    let title = data["title"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or(ConnectorFailure::StructureChanged)?;
    let target = UrlPolicy
        .normalize_for_session(url)
        .map_err(|_| ConnectorFailure::StructureChanged)?;
    let subtitles = data["subtitles"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|subtitle| {
            let url = subtitle["url"].as_str()?;
            let target = UrlPolicy.normalize_for_session(url).ok()?;
            let kind = if subtitle["automatic"].as_bool() == Some(true) {
                SubtitleKind::Automatic
            } else {
                SubtitleKind::HumanPlatform
            };
            Some((
                SubtitleCandidate::new(kind, target.public.public_url),
                target.request_url.to_string(),
            ))
        })
        .collect::<Vec<_>>();
    let plan = MediaRouter.plan_authorized(
        &MediaInput {
            kind: MediaKind::Video,
            subtitles: subtitles.iter().map(|item| item.0.clone()).collect(),
            cover_path: None,
        },
        asr.capability_available,
        asr.user_authorized,
    );
    let document = BilibiliDocument {
        title: title.into(),
        author: data["owner"]["name"].as_str().unwrap_or("Unknown").into(),
        published_at: data["publishedAt"].as_str().map(str::to_string),
        description: data["description"].as_str().unwrap_or("").into(),
        cover_public_url: data["cover"]
            .as_str()
            .and_then(|url| UrlPolicy.normalize_for_session(url).ok())
            .map(|target| target.public.public_url),
        public_url: target.public.public_url,
        chapters: data["chapters"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect(),
        subtitle_requests: subtitles.into_iter().map(|item| item.1).collect(),
    };
    Ok((document, plan))
}
