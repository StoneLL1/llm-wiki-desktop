use std::collections::HashSet;

use serde::{Deserialize, Serialize};
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
#[serde(rename_all = "camelCase")]
pub struct PlatformSubtitle {
    pub url: String,
    pub automatic: bool,
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
    pub cover_url: Option<String>,
    pub subtitles: Vec<PlatformSubtitle>,
    pub chapters: Vec<String>,
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
        .next();
    let cover_url = images.first().cloned();
    let subtitle_keys = &[
        "subtitles",
        "subtitle",
        "captions",
        "captionUrl",
        "subtitleUrl",
    ][..];
    let subtitles = collect_key_urls(value, subtitle_keys, base_url, 20)
        .into_iter()
        .map(|url| PlatformSubtitle {
            url,
            automatic: false,
            language: None,
            label: None,
        })
        .collect::<Vec<_>>();
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
    } else if !images.is_empty() {
        "image_post"
    } else {
        "article"
    };
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
        cover_url,
        subtitles,
        chapters,
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
    use super::{extract_platform_document, Platform};

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
        assert_eq!(
            Platform::from_url("https://xhslink.com/a/abc"),
            Some(Platform::Xiaohongshu)
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
