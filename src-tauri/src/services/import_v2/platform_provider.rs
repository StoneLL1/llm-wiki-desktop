use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

use crate::services::import_v2::url_policy::UrlPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Bilibili,
    Xiaohongshu,
    Douyin,
}

impl Platform {
    pub fn from_url(value: &str) -> Option<Self> {
        let host = Url::parse(value)
            .ok()?
            .host_str()?
            .trim_end_matches('.')
            .to_ascii_lowercase();
        if host == "b23.tv" || host == "bilibili.com" || host.ends_with(".bilibili.com") {
            Some(Self::Bilibili)
        } else if host == "xiaohongshu.com"
            || host.ends_with(".xiaohongshu.com")
            || host == "xhslink.com"
            || host.ends_with(".xhslink.com")
            || host == "xhslink.cn"
            || host.ends_with(".xhslink.cn")
        {
            Some(Self::Xiaohongshu)
        } else if host == "douyin.com"
            || host.ends_with(".douyin.com")
            || host == "iesdouyin.com"
            || host.ends_with(".iesdouyin.com")
        {
            Some(Self::Douyin)
        } else {
            None
        }
    }

    pub const fn id(self) -> &'static str {
        match self {
            Self::Bilibili => "bilibili",
            Self::Xiaohongshu => "xiaohongshu",
            Self::Douyin => "douyin",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlatformSubtitleKind {
    AuthorOriginal,
    PlatformAutoOriginal,
    AuthorOther,
    MachineTranslation,
}

impl Default for PlatformSubtitleKind {
    fn default() -> Self {
        Self::AuthorOriginal
    }
}

impl PlatformSubtitleKind {
    pub fn is_reliable_source(&self) -> bool {
        !matches!(self, Self::MachineTranslation)
    }

    pub fn rank(&self) -> u8 {
        match self {
            Self::AuthorOriginal => 0,
            Self::PlatformAutoOriginal => 1,
            Self::AuthorOther => 2,
            Self::MachineTranslation => 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformSubtitle {
    pub url: String,
    pub automatic: bool,
    #[serde(default)]
    pub kind: PlatformSubtitleKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformDocument {
    pub platform: String,
    pub platform_id: Option<String>,
    pub content_type: String,
    pub canonical_url: String,
    pub title: String,
    pub title_source: String,
    pub author: Option<String>,
    pub published_at: Option<String>,
    pub description: String,
    pub hashtags: Vec<String>,
    pub images: Vec<String>,
    pub media_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_size_bytes: Option<u64>,
    pub cover_url: Option<String>,
    pub subtitles: Vec<PlatformSubtitle>,
    pub chapters: Vec<String>,
    #[serde(default)]
    pub restricted_content: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformCollectionItem {
    pub title: String,
    pub url: String,
    pub duration_seconds: Option<u64>,
    pub estimated_login_required: bool,
    pub estimated_asr_required: bool,
    pub discovery_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformCollection {
    pub platform: String,
    pub title: String,
    pub items: Vec<PlatformCollectionItem>,
}

pub fn looks_like_collection_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    let path = url.path().to_ascii_lowercase();
    match Platform::from_url(value) {
        Some(Platform::Bilibili) => {
            url.host_str()
                .is_some_and(|host| host.eq_ignore_ascii_case("space.bilibili.com"))
                || path.contains("/medialist/")
                || path.contains("/list/")
                || path.contains("/favlist")
                || path.contains("/channel/collectiondetail")
        }
        Some(Platform::Xiaohongshu) => {
            path.contains("/board/")
                || path.contains("/collection/")
                || path.contains("/user/profile/")
        }
        Some(Platform::Douyin) => {
            path.contains("/collection/") || path.contains("/user/") || path.contains("/channel/")
        }
        None => false,
    }
}

pub fn extract_platform_collection(
    platform: Platform,
    html: &str,
    base_url: &str,
) -> Option<PlatformCollection> {
    let values = collect_json_values(html);
    let mut best: Vec<PlatformCollectionItem> = Vec::new();
    let mut title = None;
    for value in &values {
        find_collection_arrays(value, &mut |owner, array| {
            let items = collection_items_from_array(platform, array, base_url);
            if items.len() > best.len() {
                title = first_string(
                    owner,
                    &[
                        "title",
                        "name",
                        "collectionTitle",
                        "collection_title",
                        "listName",
                    ],
                );
                best = items;
            }
        });
    }
    if best.len() < 2 {
        return None;
    }
    Some(PlatformCollection {
        platform: platform.id().into(),
        title: title.unwrap_or_else(|| format!("{} collection", platform.id())),
        items: best,
    })
}

fn find_collection_arrays<F>(value: &serde_json::Value, visit: &mut F)
where
    F: FnMut(&serde_json::Value, &[serde_json::Value]),
{
    if let Some(object) = value.as_object() {
        for (key, child) in object {
            if matches!(
                key.to_ascii_lowercase().as_str(),
                "episodes"
                    | "archives"
                    | "medias"
                    | "items"
                    | "videolist"
                    | "video_list"
                    | "notelist"
                    | "note_list"
                    | "awemelist"
                    | "aweme_list"
            ) {
                if let Some(array) = child.as_array() {
                    visit(value, array);
                }
            }
            find_collection_arrays(child, visit);
        }
    } else if let Some(array) = value.as_array() {
        for child in array {
            find_collection_arrays(child, visit);
        }
    }
}

fn collection_items_from_array(
    platform: Platform,
    values: &[serde_json::Value],
    base_url: &str,
) -> Vec<PlatformCollectionItem> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for value in values.iter().take(5_000) {
        let title =
            first_direct_string(value, &["title", "name", "desc", "description", "caption"])
                .unwrap_or_else(|| format!("Item {}", result.len() + 1));
        let direct = first_direct_string(
            value,
            &["url", "shareUrl", "share_url", "shortLink", "link"],
        );
        let derived = match platform {
            Platform::Bilibili => first_direct_string(value, &["bvid", "bvId", "bv_id"])
                .map(|id| format!("https://www.bilibili.com/video/{id}")),
            Platform::Xiaohongshu => first_direct_string(value, &["noteId", "note_id", "id"])
                .map(|id| format!("https://www.xiaohongshu.com/explore/{id}")),
            Platform::Douyin => first_direct_string(value, &["awemeId", "aweme_id", "id"])
                .map(|id| format!("https://www.douyin.com/video/{id}")),
        };
        let Some(raw_url) = direct.or(derived) else {
            continue;
        };
        let Ok(base) = Url::parse(base_url) else {
            continue;
        };
        let Ok(url) = base.join(&raw_url).or_else(|_| Url::parse(&raw_url)) else {
            continue;
        };
        let Ok(target) = UrlPolicy.normalize_for_session(url.as_str()) else {
            continue;
        };
        if Platform::from_url(&target.public.public_url) != Some(platform)
            || !seen.insert(target.public.public_url.clone())
        {
            continue;
        }
        let duration_seconds = collection_item_duration_seconds(platform, value);
        let estimated_login_required = has_truthy_key(
            value,
            &[
                "isPrivate",
                "is_private",
                "needLogin",
                "need_login",
                "loginRequired",
                "login_required",
                "isPay",
                "is_pay",
            ],
        );
        let estimated_asr_required = collection_item_estimates_asr(platform, value);
        let updated_marker = first_direct_string(
            value,
            &[
                "updatedAt",
                "updated_at",
                "pubdate",
                "publishTime",
                "publish_time",
                "mtime",
                "version",
            ],
        )
        .unwrap_or_default();
        let discovery_fingerprint = format!(
            "{:x}",
            Sha256::digest(
                format!(
                    "{}\n{}\n{:?}\n{}\n{}\n{}",
                    title,
                    target.public.public_url,
                    duration_seconds,
                    estimated_login_required,
                    estimated_asr_required,
                    updated_marker
                )
                .as_bytes()
            )
        );
        result.push(PlatformCollectionItem {
            title: title.chars().take(160).collect(),
            url: target.request_url.to_string(),
            duration_seconds,
            estimated_login_required,
            estimated_asr_required,
            discovery_fingerprint,
        });
    }
    result
}

fn collection_item_duration_seconds(platform: Platform, value: &serde_json::Value) -> Option<u64> {
    let duration = first_u64(
        value,
        &["duration", "durationSeconds", "duration_seconds", "length"],
    )?;
    Some(if platform == Platform::Douyin && duration > 24 * 60 * 60 {
        duration / 1_000
    } else {
        duration
    })
}

fn collection_item_estimates_asr(platform: Platform, value: &serde_json::Value) -> bool {
    let video_like = matches!(platform, Platform::Bilibili | Platform::Douyin)
        || has_non_empty_key(
            value,
            &["video", "videoUrl", "video_url", "playAddr", "play_addr"],
        );
    video_like
        && !has_non_empty_key(
            value,
            &[
                "subtitles",
                "subtitle",
                "subtitleList",
                "subtitle_list",
                "captions",
            ],
        )
}

fn first_direct_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    let object = value.as_object()?;
    keys.iter()
        .find_map(|key| object.get(*key).and_then(value_as_string))
}

fn first_u64(value: &serde_json::Value, keys: &[&str]) -> Option<u64> {
    let object = value.as_object()?;
    keys.iter().find_map(|key| {
        let value = object.get(*key)?;
        value
            .as_u64()
            .or_else(|| value.as_f64().map(|number| number.max(0.0) as u64))
            .or_else(|| value.as_str()?.parse().ok())
    })
}

fn has_truthy_key(value: &serde_json::Value, keys: &[&str]) -> bool {
    if let Some(object) = value.as_object() {
        for (key, child) in object {
            if keys.iter().any(|candidate| candidate == key)
                && (child.as_bool() == Some(true)
                    || child.as_u64().is_some_and(|number| number > 0)
                    || child.as_str().is_some_and(|text| {
                        matches!(
                            text.trim().to_ascii_lowercase().as_str(),
                            "true" | "yes" | "required" | "private" | "paid"
                        )
                    }))
            {
                return true;
            }
            if has_truthy_key(child, keys) {
                return true;
            }
        }
    } else if let Some(array) = value.as_array() {
        return array.iter().any(|child| has_truthy_key(child, keys));
    }
    false
}

fn has_non_empty_key(value: &serde_json::Value, keys: &[&str]) -> bool {
    if let Some(object) = value.as_object() {
        for (key, child) in object {
            if keys.iter().any(|candidate| candidate == key)
                && match child {
                    serde_json::Value::Null => false,
                    serde_json::Value::Bool(value) => *value,
                    serde_json::Value::String(value) => !value.trim().is_empty(),
                    serde_json::Value::Array(value) => !value.is_empty(),
                    serde_json::Value::Object(value) => !value.is_empty(),
                    serde_json::Value::Number(_) => true,
                }
            {
                return true;
            }
            if has_non_empty_key(child, keys) {
                return true;
            }
        }
    } else if let Some(array) = value.as_array() {
        return array.iter().any(|child| has_non_empty_key(child, keys));
    }
    false
}

pub fn extract_platform_document(
    platform: Platform,
    html: &str,
    base_url: &str,
) -> Option<PlatformDocument> {
    let values = collect_json_values(html);
    let expected_id = extract_platform_id(platform, base_url)
        .map(|value| normalize_platform_id(platform, &value));
    if platform == Platform::Xiaohongshu && expected_id.is_none() {
        return None;
    }
    let value = if let Some(expected_id) = expected_id.as_deref() {
        values
            .iter()
            .find_map(|value| find_platform_scope(platform, value, expected_id))?
    } else {
        values
            .iter()
            .find(|value| looks_like_platform_value(platform, value))?
    };
    let canonical_url = UrlPolicy
        .normalize_for_session(base_url)
        .ok()?
        .public
        .public_url;
    let explicit_title = first_string(value, &["title", "noteTitle", "videoTitle"])
        .filter(|value| !value.trim().is_empty());
    let description = first_string(
        value,
        &["desc", "description", "content", "text", "caption"],
    )
    .unwrap_or_default();
    let (title, title_source) = if let Some(title) = explicit_title {
        (title, "platform".to_string())
    } else if matches!(platform, Platform::Xiaohongshu | Platform::Douyin) {
        (infer_title(&description)?, "inferred".to_string())
    } else {
        return None;
    };
    let author = if platform == Platform::Xiaohongshu {
        first_nested_string(
            value,
            &["user", "author"],
            &["nickname", "nickName", "username", "name"],
        )
    } else {
        first_string(
            value,
            &[
                "author", "nickname", "nickName", "uname", "username", "name",
            ],
        )
    };
    let published_at = first_string(
        value,
        &[
            "publishedAt",
            "publishTime",
            "createTime",
            "create_time",
            "time",
            "timestamp",
            "pubdate",
        ],
    )
    .map(|value| normalize_published_at(&value));
    let image_keys = match platform {
        Platform::Bilibili => &["pic", "cover", "thumbnail", "coverUrl", "cover_url"][..],
        Platform::Xiaohongshu => &[
            "imageList",
            "image_list",
            "urlDefault",
            "url_default",
            "images",
        ][..],
        Platform::Douyin => &[
            "images",
            "imageList",
            "displayImage",
            "display_image",
            "cover",
        ][..],
    };
    let images = if platform == Platform::Xiaohongshu {
        collect_xiaohongshu_images(value, base_url, 100)
            .into_iter()
            .map(upgrade_xiaohongshu_cdn_url)
            .collect()
    } else {
        collect_key_urls(value, image_keys, base_url, 100)
    };
    let media_keys = match platform {
        Platform::Bilibili => &[
            "durl",
            "baseUrl",
            "base_url",
            "playUrl",
            "play_url",
            "videoUrl",
            "video_url",
        ][..],
        // Avoid treating the whole `video` object as a URL container: both
        // XHS and Douyin put cover images beside playable streams, and map
        // iteration can otherwise select a cover as the ASR input.
        Platform::Xiaohongshu => &[
            "masterUrl",
            "master_url",
            "videoUrl",
            "video_url",
            "playUrl",
            "play_url",
        ][..],
        Platform::Douyin => &["playAddr", "play_addr", "playUrl", "play_url"][..],
    };
    let media_url = collect_key_urls(value, media_keys, base_url, 1)
        .into_iter()
        .next()
        .map(|url| {
            if platform == Platform::Xiaohongshu {
                upgrade_xiaohongshu_cdn_url(url)
            } else {
                url
            }
        });
    let cover_url = images.first().cloned();
    let subtitle_keys = &[
        "subtitles",
        "subtitle",
        "captions",
        "captionUrl",
        "subtitleUrl",
    ][..];
    let subtitles = if platform == Platform::Xiaohongshu {
        collect_xiaohongshu_subtitles(value, subtitle_keys, base_url, 20)
    } else {
        collect_key_urls(value, subtitle_keys, base_url, 20)
            .into_iter()
            .map(|url| PlatformSubtitle {
                url,
                automatic: false,
                kind: PlatformSubtitleKind::AuthorOriginal,
                language: None,
                label: None,
            })
            .collect::<Vec<_>>()
    };
    let chapters = collect_key_strings(value, &["chapters", "pages", "part"][..], 100);
    let mut hashtags = extract_hashtags(&description);
    if platform == Platform::Xiaohongshu {
        for hashtag in collect_xiaohongshu_tags(value, 100) {
            if !hashtags.contains(&hashtag) {
                hashtags.push(hashtag);
            }
        }
    }
    let platform_id = extract_platform_id(platform, base_url);
    let declared_video = platform == Platform::Xiaohongshu
        && first_string(value, &["type", "noteType", "note_type"])
            .is_some_and(|value| value.eq_ignore_ascii_case("video"));
    let content_type = if declared_video || media_url.is_some() {
        "video"
    } else if platform == Platform::Xiaohongshu || !images.is_empty() {
        "image_post"
    } else {
        "article"
    };
    let restricted_content = has_truthy_key(
        value,
        &[
            "isPrivate",
            "is_private",
            "isPay",
            "is_pay",
            "isPaid",
            "is_paid",
            "membersOnly",
            "members_only",
            "subscriberOnly",
            "subscriber_only",
            "vipOnly",
            "vip_only",
        ],
    );
    let media_size_bytes = first_u64(
        value,
        &[
            "mediaSizeBytes",
            "media_size_bytes",
            "fileSize",
            "file_size",
            "filesize",
            "contentLength",
            "content_length",
        ],
    )
    .filter(|size| *size > 0);
    Some(PlatformDocument {
        platform: platform.id().into(),
        platform_id,
        content_type: content_type.into(),
        canonical_url,
        title,
        title_source,
        author,
        published_at,
        description,
        hashtags,
        images,
        media_url,
        media_size_bytes,
        cover_url,
        subtitles,
        chapters,
        restricted_content,
    })
}

fn infer_title(description: &str) -> Option<String> {
    let line = description
        .lines()
        .find(|line| !line.trim().is_empty())?
        .trim();
    let title = line.chars().take(80).collect::<String>();
    (!title.is_empty()).then_some(title)
}

fn first_nested_string(
    value: &serde_json::Value,
    container_keys: &[&str],
    value_keys: &[&str],
) -> Option<String> {
    if let Some(object) = value.as_object() {
        for container_key in container_keys {
            if let Some((_, nested)) = object
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(container_key))
            {
                if let Some(value) = first_string(nested, value_keys) {
                    return Some(value);
                }
            }
        }
        for child in object.values() {
            if let Some(value) = first_nested_string(child, container_keys, value_keys) {
                return Some(value);
            }
        }
    } else if let Some(array) = value.as_array() {
        for child in array {
            if let Some(value) = first_nested_string(child, container_keys, value_keys) {
                return Some(value);
            }
        }
    }
    None
}

fn collect_xiaohongshu_tags(value: &serde_json::Value, limit: usize) -> Vec<String> {
    let mut tags = Vec::new();
    collect_key_values(value, &["tagList", "tag_list", "tags"], &mut |candidate| {
        let values = candidate.as_array().map(Vec::as_slice).unwrap_or_default();
        for value in values {
            let raw = value.as_str().map(str::to_string).or_else(|| {
                value.as_object().and_then(|object| {
                    ["tagName", "tag_name", "name", "title"]
                        .iter()
                        .find_map(|key| object.get(*key).and_then(value_as_string))
                })
            });
            let Some(raw) = raw.map(|value| value.trim().to_string()) else {
                continue;
            };
            if raw.is_empty() {
                continue;
            }
            let tag = if raw.starts_with('#') {
                raw
            } else {
                format!("#{raw}")
            };
            if !tags.contains(&tag) && tags.len() < limit {
                tags.push(tag);
            }
        }
    });
    tags
}

fn normalize_published_at(value: &str) -> String {
    let Ok(timestamp) = value.parse::<i64>() else {
        return value.to_string();
    };
    let datetime = if timestamp.abs() >= 100_000_000_000 {
        chrono::DateTime::from_timestamp_millis(timestamp)
    } else {
        chrono::DateTime::from_timestamp(timestamp, 0)
    };
    datetime
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(|| value.to_string())
}

fn collect_xiaohongshu_images(
    value: &serde_json::Value,
    base_url: &str,
    limit: usize,
) -> Vec<String> {
    let mut lists = Vec::new();
    collect_key_values(value, &["imageList", "image_list"], &mut |candidate| {
        if let Some(array) = candidate.as_array() {
            lists.push(array.clone());
        }
    });
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for list in lists {
        for image in list {
            if result.len() >= limit {
                return result;
            }
            let candidates = preferred_xiaohongshu_image_urls(&image);
            let Some(raw) = candidates.first() else {
                continue;
            };
            let Ok(base) = Url::parse(base_url) else {
                continue;
            };
            let Ok(url) = base.join(raw).or_else(|_| Url::parse(raw)) else {
                continue;
            };
            let Ok(target) = UrlPolicy.normalize_for_session(url.as_str()) else {
                continue;
            };
            if seen.insert(target.public.public_url) {
                result.push(target.request_url.to_string());
            }
        }
        if !result.is_empty() {
            break;
        }
    }
    if result.is_empty() {
        collect_key_urls(
            value,
            &["urlDefault", "url_default", "images"],
            base_url,
            limit,
        )
    } else {
        result
    }
}

fn collect_xiaohongshu_subtitles(
    value: &serde_json::Value,
    fallback_keys: &[&str],
    base_url: &str,
    limit: usize,
) -> Vec<PlatformSubtitle> {
    const MAX_MEDIA_V2_BYTES: usize = 2 * 1024 * 1024;

    let mut candidates = Vec::new();
    collect_key_values(value, &["mediaV2", "media_v2"], &mut |candidate| {
        let parsed = match candidate {
            serde_json::Value::String(raw) if raw.len() <= MAX_MEDIA_V2_BYTES => {
                serde_json::from_str::<serde_json::Value>(raw).ok()
            }
            serde_json::Value::Object(_) | serde_json::Value::Array(_) => Some(candidate.clone()),
            _ => None,
        };
        let Some(parsed) = parsed else {
            return;
        };
        collect_key_values(&parsed, &["subtitles"], &mut |container| {
            let Some(languages) = container.as_object() else {
                return;
            };
            for (label, entries) in languages {
                collect_xiaohongshu_subtitle_entries(
                    entries,
                    label,
                    base_url,
                    limit,
                    &mut candidates,
                );
            }
        });
    });

    for url in collect_key_urls(value, fallback_keys, base_url, limit) {
        if candidates.len() >= limit {
            break;
        }
        candidates.push(PlatformSubtitle {
            url: upgrade_xiaohongshu_cdn_url(url),
            automatic: false,
            kind: PlatformSubtitleKind::AuthorOriginal,
            language: None,
            label: None,
        });
    }

    candidates.sort_by_key(|subtitle| {
        let language = subtitle
            .language
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let label = subtitle
            .label
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let language_priority = if label == "source" {
            0
        } else if language == "zh-cn" || label == "zh-cn" {
            1
        } else if language.starts_with("zh") || label.starts_with("zh") {
            2
        } else {
            3
        };
        (subtitle.kind.rank(), language_priority)
    });
    let mut seen = HashSet::new();
    candidates.retain(|subtitle| seen.insert(subtitle.url.clone()));
    candidates.truncate(limit);
    candidates
}

fn collect_xiaohongshu_subtitle_entries(
    value: &serde_json::Value,
    label: &str,
    base_url: &str,
    limit: usize,
    output: &mut Vec<PlatformSubtitle>,
) {
    if output.len() >= limit {
        return;
    }
    if let Some(entries) = value.as_array() {
        for entry in entries {
            collect_xiaohongshu_subtitle_entries(entry, label, base_url, limit, output);
        }
        return;
    }
    let Some(entry) = value.as_object() else {
        return;
    };
    let Some(raw_url) = direct_string(entry, &["url", "subtitleUrl", "subtitle_url"]) else {
        for child in entry.values() {
            collect_xiaohongshu_subtitle_entries(child, label, base_url, limit, output);
        }
        return;
    };
    let Ok(base) = Url::parse(base_url) else {
        return;
    };
    let Ok(url) = base.join(&raw_url).or_else(|_| Url::parse(&raw_url)) else {
        return;
    };
    let Ok(target) = UrlPolicy.normalize_for_session(url.as_str()) else {
        return;
    };
    let language = direct_string(
        entry,
        &["language", "languageCode", "language_code", "lang"],
    )
    .or_else(|| (!label.eq_ignore_ascii_case("source")).then(|| label.to_string()));
    let automatic = direct_bool(entry, &["automatic", "isAuto", "is_auto"]).unwrap_or(true);
    let source_track = label.eq_ignore_ascii_case("source");
    output.push(PlatformSubtitle {
        url: upgrade_xiaohongshu_cdn_url(target.request_url.to_string()),
        automatic,
        kind: match (source_track, automatic) {
            (true, false) => PlatformSubtitleKind::AuthorOriginal,
            (true, true) => PlatformSubtitleKind::PlatformAutoOriginal,
            (false, false) => PlatformSubtitleKind::AuthorOther,
            (false, true) => PlatformSubtitleKind::MachineTranslation,
        },
        language,
        label: Some(label.to_string()),
    });
}

fn direct_string(
    value: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    value.iter().find_map(|(key, value)| {
        keys.iter()
            .any(|candidate| key.eq_ignore_ascii_case(candidate))
            .then(|| value_as_string(value))
            .flatten()
    })
}

fn direct_bool(value: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<bool> {
    value.iter().find_map(|(key, value)| {
        if keys
            .iter()
            .any(|candidate| key.eq_ignore_ascii_case(candidate))
        {
            value.as_bool()
        } else {
            None
        }
    })
}

fn upgrade_xiaohongshu_cdn_url(value: String) -> String {
    let Ok(mut url) = Url::parse(&value) else {
        return value;
    };
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let trusted_cdn = host == "xhscdn.com"
        || host.ends_with(".xhscdn.com")
        || host == "xhscdn.net"
        || host.ends_with(".xhscdn.net");
    if url.scheme() == "http" && trusted_cdn && url.port().is_none_or(|port| port == 80) {
        let _ = url.set_scheme("https");
        let _ = url.set_port(None);
        return url.to_string();
    }
    value
}

fn preferred_xiaohongshu_image_urls(value: &serde_json::Value) -> Vec<String> {
    let Some(object) = value.as_object() else {
        return value.as_str().map(str::to_string).into_iter().collect();
    };
    for key in ["urlDefault", "url_default", "urlPre", "url_pre", "url"] {
        if let Some(url) = object.get(key).and_then(serde_json::Value::as_str) {
            if !url.trim().is_empty() {
                return vec![url.to_string()];
            }
        }
    }
    for key in ["infoList", "info_list"] {
        let Some(entries) = object.get(key).and_then(serde_json::Value::as_array) else {
            continue;
        };
        let mut fallback = None;
        for entry in entries {
            let Some(entry) = entry.as_object() else {
                continue;
            };
            let Some(url) = entry.get("url").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let scene = entry
                .get("imageScene")
                .or_else(|| entry.get("image_scene"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if scene.eq_ignore_ascii_case("WB_DFT") {
                return vec![url.to_string()];
            }
            fallback.get_or_insert_with(|| url.to_string());
        }
        if let Some(url) = fallback {
            return vec![url];
        }
    }
    Vec::new()
}

fn normalize_platform_id(platform: Platform, value: &str) -> String {
    if platform == Platform::Bilibili && value.to_ascii_lowercase().starts_with("av") {
        value[2..].to_ascii_lowercase()
    } else {
        value.to_ascii_lowercase()
    }
}

fn find_platform_scope<'a>(
    platform: Platform,
    value: &'a serde_json::Value,
    expected_id: &str,
) -> Option<&'a serde_json::Value> {
    if let Some(object) = value.as_object() {
        let keys = match platform {
            Platform::Bilibili => &["bvid", "aid"][..],
            Platform::Xiaohongshu => &["noteId", "note_id", "id"][..],
            Platform::Douyin => &["awemeId", "aweme_id", "itemId", "item_id"][..],
        };
        if keys.iter().any(|key| {
            object
                .iter()
                .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
                .and_then(|(_, value)| value_as_string(value))
                .is_some_and(|value| normalize_platform_id(platform, &value) == expected_id)
        }) {
            return Some(value);
        }
        for child in object.values() {
            if let Some(found) = find_platform_scope(platform, child, expected_id) {
                return Some(found);
            }
        }
    } else if let Some(array) = value.as_array() {
        for child in array {
            if let Some(found) = find_platform_scope(platform, child, expected_id) {
                return Some(found);
            }
        }
    }
    None
}

fn looks_like_platform_value(platform: Platform, value: &serde_json::Value) -> bool {
    let title = first_string(value, &["title", "noteTitle", "videoTitle"]).or_else(|| {
        matches!(platform, Platform::Xiaohongshu | Platform::Douyin)
            .then(|| first_string(value, &["desc", "description"]))
            .flatten()
    });
    let platform_marker = match platform {
        Platform::Bilibili => {
            value.get("data").is_some() || first_string(value, &["bvid", "aid", "owner"]).is_some()
        }
        Platform::Xiaohongshu => {
            first_string(value, &["noteId", "note_id", "xsecToken", "userId"]).is_some()
                || value.get("noteCard").is_some()
        }
        Platform::Douyin => {
            first_string(value, &["awemeId", "aweme_id", "secUid", "sec_uid"]).is_some()
                || value.get("aweme_detail").is_some()
                || value.get("awemeDetail").is_some()
        }
    };
    title.is_some() && platform_marker
}

fn collect_json_values(html: &str) -> Vec<serde_json::Value> {
    let mut values = Vec::new();
    let mut cursor = 0;
    let lower = html.to_ascii_lowercase();
    while let Some(start_offset) = lower[cursor..].find("<script") {
        let start = cursor + start_offset;
        let Some(open_end_offset) = lower[start..].find('>') else {
            break;
        };
        let content_start = start + open_end_offset + 1;
        let Some(close_offset) = lower[content_start..].find("</script>") else {
            break;
        };
        let content_end = content_start + close_offset;
        push_json_value(&html[content_start..content_end], &mut values);
        cursor = content_end + 9;
    }
    for marker in [
        "__INITIAL_STATE__",
        "__INITIAL_SSR_STATE__",
        "__UNIVERSAL_DATA_FOR_REHYDRATION__",
        "__NEXT_DATA__",
        "RENDER_DATA",
    ] {
        let mut from = 0;
        while let Some(offset) = html[from..].find(marker) {
            let index = from + offset;
            push_json_value(&html[index + marker.len()..], &mut values);
            from = index + marker.len();
        }
    }
    values
}

fn push_json_value(input: &str, values: &mut Vec<serde_json::Value>) {
    let Some(start) = input.find(['{', '[']) else {
        return;
    };
    let Some(value) = balanced_json_value(&input[start..]) else {
        return;
    };
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn balanced_json_value(input: &str) -> Option<serde_json::Value> {
    let bytes = input.as_bytes();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (index, byte) in bytes.iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match *byte {
            b'"' => in_string = true,
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    let candidate = &input[..=index];
                    return serde_json::from_str(candidate).ok().or_else(|| {
                        serde_json::from_str(&normalize_undefined_literals(candidate)).ok()
                    });
                }
            }
            _ => {}
        }
    }
    None
}

fn normalize_undefined_literals(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut index = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    while index < input.len() {
        let rest = &input[index..];
        let character = rest.chars().next().unwrap_or_default();
        if in_string {
            output.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            index += character.len_utf8();
            continue;
        }
        if character == '"' {
            in_string = true;
            output.push(character);
            index += 1;
            continue;
        }
        if rest.starts_with("undefined") {
            let before = input[..index].bytes().next_back();
            let after = input.as_bytes().get(index + "undefined".len()).copied();
            let identifier = |byte: Option<u8>| {
                byte.is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
            };
            if !identifier(before) && !identifier(after) {
                output.push_str("null");
                index += "undefined".len();
                continue;
            }
        }
        output.push(character);
        index += character.len_utf8();
    }
    output
}

fn first_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    if let Some(object) = value.as_object() {
        for key in keys {
            if let Some(candidate) = object.get(*key).and_then(value_as_string) {
                return Some(candidate);
            }
        }
        for child in object.values() {
            if let Some(candidate) = first_string(child, keys) {
                return Some(candidate);
            }
        }
    } else if let Some(array) = value.as_array() {
        for child in array {
            if let Some(candidate) = first_string(child, keys) {
                return Some(candidate);
            }
        }
    }
    None
}

fn value_as_string(value: &serde_json::Value) -> Option<String> {
    value.as_str().map(str::to_string).or_else(|| {
        value
            .as_i64()
            .map(|number| number.to_string())
            .or_else(|| value.as_u64().map(|number| number.to_string()))
    })
}

fn collect_key_urls(
    value: &serde_json::Value,
    keys: &[&str],
    base_url: &str,
    limit: usize,
) -> Vec<String> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    collect_key_values(value, keys, &mut |candidate| {
        collect_urls(candidate, base_url, &mut found, &mut seen, limit);
    });
    found
}

fn collect_key_strings(value: &serde_json::Value, keys: &[&str], limit: usize) -> Vec<String> {
    let mut result = Vec::new();
    collect_key_values(value, keys, &mut |candidate| {
        if let Some(value) = value_as_string(candidate) {
            if !result.contains(&value) && result.len() < limit {
                result.push(value);
            }
        } else if let Some(array) = candidate.as_array() {
            for child in array {
                if let Some(value) = value_as_string(child) {
                    if !result.contains(&value) && result.len() < limit {
                        result.push(value);
                    }
                }
            }
        }
    });
    result
}

fn collect_key_values(
    value: &serde_json::Value,
    keys: &[&str],
    callback: &mut impl FnMut(&serde_json::Value),
) {
    if let Some(object) = value.as_object() {
        for (key, child) in object {
            if keys
                .iter()
                .any(|candidate| key.eq_ignore_ascii_case(candidate))
            {
                callback(child);
            }
            collect_key_values(child, keys, callback);
        }
    } else if let Some(array) = value.as_array() {
        for child in array {
            collect_key_values(child, keys, callback);
        }
    }
}

fn collect_urls(
    value: &serde_json::Value,
    base_url: &str,
    output: &mut Vec<String>,
    seen: &mut HashSet<String>,
    limit: usize,
) {
    if output.len() >= limit {
        return;
    }
    if let Some(raw) = value.as_str() {
        let Ok(base) = Url::parse(base_url) else {
            return;
        };
        let Ok(url) = base.join(raw).or_else(|_| Url::parse(raw)) else {
            return;
        };
        if !matches!(url.scheme(), "http" | "https") {
            return;
        }
        let Ok(target) = UrlPolicy.normalize_for_session(url.as_str()) else {
            return;
        };
        // Keep the request URL in memory so signed CDN parameters remain
        // available to the immediate fetch.  Redaction is applied when the
        // value is persisted to Markdown/metadata, not before downloading.
        let request_url = target.request_url.to_string();
        if seen.insert(request_url.clone()) {
            output.push(request_url);
        }
    } else if let Some(array) = value.as_array() {
        for child in array {
            collect_urls(child, base_url, output, seen, limit);
        }
    } else if let Some(object) = value.as_object() {
        for child in object.values() {
            collect_urls(child, base_url, output, seen, limit);
        }
    }
}

fn extract_hashtags(value: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    for character in value.chars() {
        if character == '#' {
            current.clear();
            current.push(character);
        } else if !current.is_empty() && (character.is_alphanumeric() || character == '_') {
            current.push(character);
        } else if !current.is_empty() {
            if current.len() > 1 && !result.contains(&current) {
                result.push(current.clone());
            }
            current.clear();
        }
    }
    if current.len() > 1 && !result.contains(&current) {
        result.push(current);
    }
    result
}

fn extract_platform_id(platform: Platform, value: &str) -> Option<String> {
    let url = Url::parse(value).ok()?;
    let segments = url
        .path_segments()?
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    match platform {
        Platform::Bilibili => segments
            .iter()
            .find(|segment| segment.starts_with("BV") || segment.starts_with("av"))
            .map(|segment| (*segment).to_string()),
        Platform::Xiaohongshu => match segments.as_slice() {
            ["explore", note_id] => Some((*note_id).to_string()),
            ["discovery", "item", note_id] => Some((*note_id).to_string()),
            ["user", "profile", _author_id, note_id] => Some((*note_id).to_string()),
            _ => None,
        },
        Platform::Douyin => segments
            .iter()
            .find(|segment| segment.chars().all(|character| character.is_ascii_digit()))
            .map(|segment| (*segment).to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        extract_platform_collection, extract_platform_document, looks_like_collection_url,
        upgrade_xiaohongshu_cdn_url, Platform,
    };

    #[test]
    fn discovers_bilibili_collection_children_in_source_order_from_real_fixture() {
        let html = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../tests/fixtures/import-v2/web/bilibili/collection.html"
        ));
        let collection = extract_platform_collection(
            Platform::Bilibili,
            html,
            "https://www.bilibili.com/medialist/play/123",
        )
        .unwrap();
        assert_eq!(collection.title, "课程合集");
        assert_eq!(
            collection
                .items
                .iter()
                .map(|item| item.title.as_str())
                .collect::<Vec<_>>(),
            ["第一讲", "第二讲", "第三讲"]
        );
        assert!(collection.items[1].url.ends_with("/video/BV2second"));
        assert_eq!(collection.items[0].duration_seconds, Some(120));
        assert!(collection.items[0].estimated_login_required);
        assert!(collection.items[0].estimated_asr_required);
        assert!(!collection.items[1].estimated_asr_required);
    }

    #[test]
    fn ordinary_platform_item_does_not_enter_collection_discovery() {
        assert!(!looks_like_collection_url(
            "https://www.bilibili.com/video/BV1single"
        ));
        assert!(looks_like_collection_url(
            "https://www.bilibili.com/medialist/play/123"
        ));
        assert!(looks_like_collection_url("https://space.bilibili.com/42"));
    }

    #[test]
    fn extracts_bilibili_embedded_json_metadata_and_media() {
        let html = r##"<script>window.__INITIAL_STATE__={"data":{"title":"标题","owner":{"name":"作者"},"desc":"#AI","bvid":"BV1abc","durl":[{"url":"https://cdn.example/video.mp4"}]}};</script>"##;
        let document = extract_platform_document(
            Platform::Bilibili,
            html,
            "https://www.bilibili.com/video/BV1abc",
        )
        .unwrap();
        assert_eq!(document.title, "标题");
        assert_eq!(document.author.as_deref(), Some("作者"));
        assert_eq!(
            document.media_url.as_deref(),
            Some("https://cdn.example/video.mp4")
        );
    }

    #[test]
    fn explicit_provider_metadata_marks_restricted_content() {
        let html = r#"<script type="application/json">{"data":{"title":"Private lesson","bvid":"BV1private","isPrivate":true}}</script>"#;
        let document = extract_platform_document(
            Platform::Bilibili,
            html,
            "https://www.bilibili.com/video/BV1private",
        )
        .unwrap();

        assert!(document.restricted_content);
    }

    #[test]
    fn extracts_douyin_image_post_and_hashtags() {
        let html = r##"<script type="application/json">{"aweme_detail":{"awemeId":"123","desc":"正文 #知识库","author":{"nickname":"作者"},"images":[{"url_list":["https://cdn.example/1.jpg"]}]}}</script>"##;
        let document =
            extract_platform_document(Platform::Douyin, html, "https://www.douyin.com/video/123")
                .unwrap();
        assert_eq!(document.content_type, "image_post");
        assert_eq!(document.images.len(), 1);
        assert_eq!(document.hashtags, vec!["#知识库"]);
    }

    #[test]
    fn recognizes_xhs_short_link_platform() {
        for url in ["https://xhslink.com/a/abc", "http://xhslink.cn/o/abc"] {
            assert_eq!(Platform::from_url(url), Some(Platform::Xiaohongshu));
        }
    }

    #[test]
    fn upgrades_only_exact_trusted_xhs_cdn_http_hosts() {
        assert_eq!(
            upgrade_xiaohongshu_cdn_url("http://sns-video-v6.xhscdn.com/video.mp4".to_string()),
            "https://sns-video-v6.xhscdn.com/video.mp4"
        );
        assert_eq!(
            upgrade_xiaohongshu_cdn_url(
                "http://sns-video-v6.xhscdn.com.evil.example/video.mp4".to_string()
            ),
            "http://sns-video-v6.xhscdn.com.evil.example/video.mp4"
        );
        assert_eq!(
            upgrade_xiaohongshu_cdn_url(
                "http://sns-video-v6.xhscdn.com:8080/video.mp4".to_string()
            ),
            "http://sns-video-v6.xhscdn.com:8080/video.mp4"
        );
    }

    #[test]
    fn extracts_xhs_note_without_mistaking_the_cover_for_video() {
        let html = r#"<script type="application/json">{"noteCard":{"noteId":"note-1","title":"XHS title","desc":"Body #topic","user":{"nickname":"Author"},"imageList":[{"urlDefault":"https://sns-webpic-qc.xhscdn.com/cover.jpg"}],"video":{"media":{"stream":{"h264":[{"masterUrl":"https://sns-video-qc.xhscdn.com/video.mp4"}]}}}}}</script>"#;
        let document = extract_platform_document(
            Platform::Xiaohongshu,
            html,
            "https://www.xiaohongshu.com/explore/note-1",
        )
        .unwrap();
        assert_eq!(document.title, "XHS title");
        assert_eq!(document.author.as_deref(), Some("Author"));
        assert_eq!(
            document.media_url.as_deref(),
            Some("https://sns-video-qc.xhscdn.com/video.mp4")
        );
        assert_eq!(
            document.cover_url.as_deref(),
            Some("https://sns-webpic-qc.xhscdn.com/cover.jpg")
        );
    }

    #[test]
    fn douyin_video_prefers_play_addr_over_cover_urls() {
        let html = r#"<script type="application/json">{"aweme_detail":{"awemeId":"123","desc":"Video","video":{"cover":{"url_list":["https://p3-sign.douyinpic.com/cover.jpg"]},"play_addr":{"url_list":["https://v3-dy-o-abtest.zjcdn.com/video.mp4"]}}}}</script>"#;
        let document =
            extract_platform_document(Platform::Douyin, html, "https://www.douyin.com/video/123")
                .unwrap();
        assert_eq!(
            document.media_url.as_deref(),
            Some("https://v3-dy-o-abtest.zjcdn.com/video.mp4")
        );
        assert_ne!(document.media_url, document.cover_url);
    }

    #[test]
    fn keeps_signed_asset_query_for_the_immediate_fetch() {
        let html = r##"<script type="application/json">{"title":"Signed video","bvid":"BV1abc","durl":[{"url":"https://cdn.example/video.mp4?xsec_token=signed&expires=123"}]}</script>"##;
        let document = extract_platform_document(
            Platform::Bilibili,
            html,
            "https://www.bilibili.com/video/BV1abc",
        )
        .unwrap();
        assert_eq!(
            document.media_url.as_deref(),
            Some("https://cdn.example/video.mp4?xsec_token=signed&expires=123")
        );
    }

    #[test]
    fn anchors_platform_json_to_the_requested_url_id() {
        let html = r#"<script type="application/json">{"feed":[{"aweme_id":"999","desc":"Recommended","video":{"play_addr":{"url_list":["https://v3-dy-o-abtest.zjcdn.com/wrong.mp4"]}}},{"aweme_id":"123","desc":"Requested","video":{"play_addr":{"url_list":["https://v3-dy-o-abtest.zjcdn.com/right.mp4"]}}}]}</script>"#;
        let document =
            extract_platform_document(Platform::Douyin, html, "https://www.douyin.com/video/123")
                .unwrap();
        assert_eq!(document.title, "Requested");
        assert_eq!(
            document.media_url.as_deref(),
            Some("https://v3-dy-o-abtest.zjcdn.com/right.mp4")
        );
        assert!(extract_platform_document(
            Platform::Douyin,
            html,
            "https://www.douyin.com/video/456",
        )
        .is_none());
    }
}
