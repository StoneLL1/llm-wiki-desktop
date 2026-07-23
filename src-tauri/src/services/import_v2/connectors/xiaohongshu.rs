use super::ConnectorFailure;
use crate::services::import_v2::platform_provider::{
    extract_platform_document, Platform, PlatformDocument,
};
use crate::services::import_v2::url_policy::UrlPolicy;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct XiaohongshuDocument {
    pub title: String,
    pub author: String,
    pub body: String,
    pub public_url: String,
    pub images: Vec<String>,
    #[serde(skip)]
    pub authenticated_request_url: String,
}

/// Parse the structured state embedded in a Xiaohongshu note page. The
/// returned document contains immediate-request asset URLs in memory; callers
/// must redact them before persisting provider evidence.
pub fn extract_page(html: &str, request_url: &str) -> Result<PlatformDocument, ConnectorFailure> {
    if let Some(document) = extract_platform_document(Platform::Xiaohongshu, html, request_url) {
        if document.content_type == "video" && document.media_url.is_none() {
            return Err(ConnectorFailure::StructureChanged);
        }
        if document
            .platform_id
            .as_deref()
            .is_some_and(|id| !id.is_empty())
            && (!document.description.trim().is_empty()
                || !document.images.is_empty()
                || document.media_url.is_some())
        {
            return Ok(document);
        }
        if let Some(failure) = classify_page(html) {
            return Err(failure);
        }
        return Err(
            if document.platform_id.as_deref().is_none_or(str::is_empty) {
                ConnectorFailure::StructureChanged
            } else {
                ConnectorFailure::EmptyBody
            },
        );
    }
    classify_page(html).map_or(Err(ConnectorFailure::StructureChanged), Err)
}

pub fn classify_page(html: &str) -> Option<ConnectorFailure> {
    let lower = html.to_ascii_lowercase();
    if contains_any(
        &lower,
        &[
            "captcha",
            "滑块验证",
            "请通过验证",
            "请完成验证",
            "安全验证",
        ],
    ) {
        return Some(ConnectorFailure::Captcha);
    }
    if contains_any(
        &lower,
        &[
            "login required",
            "signflow",
            "请先登录",
            "登录后查看",
            "登录后浏览",
        ],
    ) {
        return Some(ConnectorFailure::LoginRequired);
    }
    if contains_any(
        &lower,
        &[
            "note has been deleted",
            "note not found",
            "笔记已删除",
            "该笔记已被删除",
            "内容不存在",
            "当前内容无法展示",
        ],
    ) {
        return Some(ConnectorFailure::Removed);
    }
    None
}

fn contains_any(value: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| value.contains(marker))
}

/// Compatibility parser for normalized provider JSON used by older fixtures.
/// New page ingestion should call [`extract_page`] so it remains anchored to
/// the requested note id and handles real SSR/INITIAL_STATE shapes.
pub fn extract_json(
    json: &str,
    request_url: &str,
    release_enabled: bool,
) -> Result<XiaohongshuDocument, ConnectorFailure> {
    if !release_enabled {
        return Err(ConnectorFailure::StructureChanged);
    }
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|_| ConnectorFailure::StructureChanged)?;
    if value["captcha"].as_bool() == Some(true) {
        return Err(ConnectorFailure::Captcha);
    }
    if value["loginRequired"].as_bool() == Some(true) {
        return Err(ConnectorFailure::LoginRequired);
    }
    let target = UrlPolicy
        .normalize_for_session(request_url)
        .map_err(|_| ConnectorFailure::StructureChanged)?;
    Ok(XiaohongshuDocument {
        title: value["title"]
            .as_str()
            .ok_or(ConnectorFailure::StructureChanged)?
            .into(),
        author: value["author"].as_str().unwrap_or("Unknown").into(),
        body: value["body"].as_str().unwrap_or("").into(),
        public_url: target.public.public_url,
        images: value["images"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|image| image.as_str())
            .filter_map(|url| UrlPolicy.normalize_for_session(url).ok())
            .map(|target| target.public.public_url)
            .collect(),
        authenticated_request_url: target.request_url.to_string(),
    })
}
