use std::collections::BTreeMap;
use std::{
    io::Read,
    time::{SystemTime, UNIX_EPOCH},
};

use flate2::read::GzDecoder;
use md5::{Digest, Md5};
use serde_json::Value;

use crate::errors::BackendError;
use crate::models::import_v2::MediaSaveMode;
use crate::services::import_v2::engine::EngineRequest;
use crate::services::import_v2::markdown_normalizer::decode_text;
use crate::services::import_v2::platform_provider::{
    extract_platform_document, Platform, PlatformDocument, PlatformSubtitle, PlatformSubtitleKind,
};
use crate::services::import_v2::url_policy::UrlPolicy;
use crate::services::import_v2::web_fetch::{WebFetchContent, WebFetchPolicy, WebFetchService};
use crate::tasks::task_model::CancellationToken;

const WBI_MIXIN_KEY_ENC_TAB: [usize; 64] = [
    46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49, 33, 9, 42, 19, 29,
    28, 14, 39, 12, 38, 41, 13, 37, 48, 7, 16, 24, 55, 40, 61, 26, 17, 0, 1, 60, 51, 30, 4, 22, 25,
    54, 21, 56, 59, 6, 63, 57, 62, 11, 36, 20, 34, 44, 52,
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct VideoRef {
    bvid: Option<String>,
    aid: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct BilibiliApiResult {
    pub source_body: String,
    pub document: PlatformDocument,
}

/// Uses Bilibili's public metadata/player endpoints as the deterministic
/// fallback for pages that are client-rendered or rejected by the HTML edge.
/// The browser capability remains the preferred route for authenticated/private
/// pages and short links that do not expose a video id in the pasted URL.
pub(crate) fn fetch(
    request: &EngineRequest,
    cancellation: &CancellationToken,
) -> Result<Option<BilibiliApiResult>, BackendError> {
    let page_url = request
        .input
        .normalized_locator
        .as_deref()
        .unwrap_or(&request.input.locator);
    let Some(mut video_ref) = extract_video_ref(page_url) else {
        return Ok(None);
    };
    if cancellation.is_cancelled() {
        return Err(cancelled());
    }

    let view_url = api_url("x/web-interface/view", &video_ref.view_params());
    let (view_body, view) = fetch_json(&view_url, page_url, &request.item_id, cancellation)?;
    if let Some(error) = classify_view_api_error(&view) {
        return Err(error);
    }
    if video_ref.aid.is_none() {
        video_ref.aid = view
            .pointer("/data/aid")
            .and_then(Value::as_u64)
            .map(|aid| aid.to_string());
    }
    let mut document = extract_platform_document(Platform::Bilibili, &view_body, page_url)
        .or_else(|| {
            let wrapped = format!("<script type=\"application/json\">{view_body}</script>");
            extract_platform_document(Platform::Bilibili, &wrapped, page_url)
        })
        .ok_or_else(|| api_unavailable("Bilibili metadata changed or is unavailable."))?;

    let cid = selected_cid(&view, page_url);
    let mut player_evidence = None;
    if let Some(cid) = cid {
        document.content_type = "video".into();
        match fetch_player_metadata(&video_ref, cid, page_url, &request.item_id, cancellation) {
            Ok(player) => {
                document.subtitles = extract_player_subtitles(&player);
                player_evidence = Some(player);
            }
            Err(error) if error.code == "IMPORT_V2_CANCELLED" => return Err(error),
            Err(_) => {}
        }
    }
    let need_media =
        request.media_save_mode == MediaSaveMode::PreserveOriginal || request.local_asr_authorized;
    if need_media {
        if let Some(cid) = cid {
            match fetch_play_url(
                &video_ref,
                cid,
                page_url,
                &request.item_id,
                cancellation,
                request.media_save_mode == MediaSaveMode::PreserveOriginal,
            ) {
                Ok(Some(media_url)) => document.media_url = Some(media_url),
                Ok(None) => {}
                Err(error) if error.code == "IMPORT_V2_CANCELLED" => return Err(error),
                Err(_) => {}
            }
        }
    }

    let source_body = serde_json::to_string_pretty(&serde_json::json!({
        "view": view,
        "player": player_evidence,
    }))
    .map_err(|_| api_unavailable("Bilibili evidence could not be serialized."))?;

    Ok(Some(BilibiliApiResult {
        source_body,
        document,
    }))
}

fn fetch_player_metadata(
    video_ref: &VideoRef,
    cid: u64,
    page_url: &str,
    item_id: &str,
    cancellation: &CancellationToken,
) -> Result<Value, BackendError> {
    let nav_url = "https://api.bilibili.com/x/web-interface/nav";
    let (_, nav) = fetch_json(nav_url, page_url, item_id, cancellation)?;
    let Some(mixin_key) = wbi_mixin_key(&nav) else {
        return Err(api_unavailable("Bilibili did not expose a WBI player key."));
    };
    let mut params = video_ref.player_params(cid);
    params.insert(
        "wts".into(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_else(|_| "0".into()),
    );
    let unsigned_query = encode_params(&params);
    let digest = Md5::digest(format!("{unsigned_query}{mixin_key}").as_bytes());
    params.insert("w_rid".into(), hex_lower(&digest));
    let player_url = api_url("x/player/wbi/v2", &params);
    let (_, response) = fetch_json(&player_url, page_url, item_id, cancellation)?;
    if response.get("code").and_then(Value::as_i64) != Some(0) {
        return Err(api_unavailable(
            "Bilibili did not return public player metadata.",
        ));
    }
    Ok(response)
}

fn extract_player_subtitles(value: &Value) -> Vec<PlatformSubtitle> {
    let mut subtitles = value
        .pointer("/data/subtitle/subtitles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|subtitle| {
            let raw_url = ["subtitle_url", "subtitleUrl", "url"]
                .iter()
                .find_map(|key| subtitle.get(*key).and_then(Value::as_str))?;
            let url = if raw_url.starts_with("//") {
                format!("https:{raw_url}")
            } else {
                raw_url.to_owned()
            };
            if !url.starts_with("https://") {
                return None;
            }
            let automatic = subtitle
                .get("ai_status")
                .and_then(Value::as_i64)
                .is_some_and(|status| status != 0)
                || subtitle
                    .get("ai_type")
                    .and_then(Value::as_i64)
                    .is_some_and(|kind| kind != 0)
                || subtitle
                    .get("type")
                    .and_then(Value::as_i64)
                    .is_some_and(|kind| kind != 0);
            let machine_translation = subtitle
                .get("is_translate")
                .or_else(|| subtitle.get("isTranslate"))
                .and_then(Value::as_bool)
                .unwrap_or(false)
                || subtitle
                    .get("ai_type")
                    .and_then(Value::as_i64)
                    .is_some_and(|kind| kind >= 2)
                || subtitle
                    .get("type")
                    .and_then(Value::as_i64)
                    .is_some_and(|kind| kind >= 2)
                || subtitle
                    .get("lan_doc")
                    .and_then(Value::as_str)
                    .is_some_and(is_translation_label);
            Some((
                PlatformSubtitle {
                    url,
                    automatic,
                    kind: PlatformSubtitleKind::AuthorOriginal,
                    language: subtitle
                        .get("lan")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    label: subtitle
                        .get("lan_doc")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                },
                machine_translation,
            ))
        })
        .collect::<Vec<_>>();
    // Bilibili usually returns the primary-language track first, except that
    // some responses contain only an explicitly marked machine translation.
    // Never infer an explicitly translated track to be the source language.
    let original_language = subtitles
        .iter()
        .find(|(_, machine_translation)| !machine_translation)
        .and_then(|(subtitle, _)| subtitle.language.clone());
    for (subtitle, machine_translation) in &mut subtitles {
        let original_track = match (original_language.as_deref(), subtitle.language.as_deref()) {
            (Some(original), Some(language)) => language.eq_ignore_ascii_case(original),
            _ => !*machine_translation,
        };
        subtitle.kind = if *machine_translation || (subtitle.automatic && !original_track) {
            PlatformSubtitleKind::MachineTranslation
        } else if original_track && subtitle.automatic {
            PlatformSubtitleKind::PlatformAutoOriginal
        } else if original_track {
            PlatformSubtitleKind::AuthorOriginal
        } else {
            PlatformSubtitleKind::AuthorOther
        };
    }
    subtitles.sort_by_key(|(subtitle, _)| subtitle.kind.rank());
    subtitles
        .into_iter()
        .map(|(subtitle, _)| subtitle)
        .collect()
}

fn is_translation_label(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("translation")
        || lower.contains("translated")
        || value.contains("翻译")
        || value.contains("机翻")
        || value.contains("译制")
}

fn fetch_play_url(
    video_ref: &VideoRef,
    cid: u64,
    page_url: &str,
    item_id: &str,
    cancellation: &CancellationToken,
    preserve_original: bool,
) -> Result<Option<String>, BackendError> {
    let mut public_params = video_ref.player_params(cid);
    public_params.insert("qn".into(), "64".into());
    public_params.insert("fnval".into(), "0".into());
    public_params.insert("fnver".into(), "0".into());
    public_params.insert("fourk".into(), "0".into());
    let public_url = api_url("x/player/playurl", &public_params);
    let (_, public_response) = fetch_json(&public_url, page_url, item_id, cancellation)?;
    if public_response.get("code").and_then(Value::as_i64) == Some(0) {
        if let Some(media_url) = select_media_url(public_response.get("data"), preserve_original) {
            return Ok(Some(media_url));
        }
    }

    let nav_url = "https://api.bilibili.com/x/web-interface/nav";
    let (_, nav) = fetch_json(nav_url, page_url, item_id, cancellation)?;
    let Some(mixin_key) = wbi_mixin_key(&nav) else {
        return Ok(None);
    };

    let mut params = video_ref.player_params(cid);
    params.insert("qn".into(), "64".into());
    params.insert("fnval".into(), "0".into());
    params.insert("fnver".into(), "0".into());
    params.insert("fourk".into(), "1".into());
    params.insert(
        "wts".into(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_else(|_| "0".into()),
    );
    let unsigned_query = encode_params(&params);
    let digest = Md5::digest(format!("{unsigned_query}{mixin_key}").as_bytes());
    params.insert("w_rid".into(), hex_lower(&digest));
    let signed_url = api_url("x/player/wbi/playurl", &params);
    let (_, response) = fetch_json(&signed_url, page_url, item_id, cancellation)?;
    if response.get("code").and_then(Value::as_i64) != Some(0) {
        return Ok(None);
    }
    Ok(select_media_url(response.get("data"), preserve_original))
}

fn fetch_json(
    url: &str,
    referer: &str,
    item_id: &str,
    cancellation: &CancellationToken,
) -> Result<(String, Value), BackendError> {
    let target = UrlPolicy::default().normalize_for_session(url)?;
    let referer = referer.to_owned();
    let item_id = item_id.to_owned();
    let token = cancellation.clone();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| api_unavailable("The Bilibili API runtime could not be started."))?;
        let artifact = runtime.block_on(WebFetchService::default().fetch(
            target,
            &UrlPolicy::default(),
            &WebFetchPolicy {
                max_response_bytes: 8 * 1024 * 1024,
                content: WebFetchContent::Page,
                referer: Some(referer),
                ..WebFetchPolicy::default()
            },
            None,
            &item_id,
            |_| {},
            || token.is_cancelled(),
        ))?;
        let decoded = decode_json_transport(&artifact.bytes)?;
        let body = decode_text(&decoded)?;
        let value = serde_json::from_str(&body)
            .map_err(|_| api_unavailable("Bilibili returned an invalid API response."))?;
        Ok((body, value))
    })
    .join()
    .map_err(|_| api_unavailable("The Bilibili API worker stopped unexpectedly."))?
}

fn decode_json_transport(bytes: &[u8]) -> Result<Vec<u8>, BackendError> {
    if !bytes.starts_with(&[0x1f, 0x8b]) {
        return Ok(bytes.to_vec());
    }
    let mut decoder = GzDecoder::new(bytes);
    let mut output = Vec::new();
    decoder
        .by_ref()
        .take((8 * 1024 * 1024 + 1) as u64)
        .read_to_end(&mut output)
        .map_err(|_| api_unavailable("Bilibili returned an invalid compressed API response."))?;
    if output.len() > 8 * 1024 * 1024 {
        return Err(api_unavailable(
            "Bilibili returned an oversized API response.",
        ));
    }
    Ok(output)
}

fn selected_cid(value: &Value, page_url: &str) -> Option<u64> {
    let page = url::Url::parse(page_url)
        .ok()
        .and_then(|url| {
            url.query_pairs()
                .find_map(|(key, value)| (key == "p").then(|| value.into_owned()))
        })
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|page| *page > 0)
        .unwrap_or(1);
    let data = value.get("data")?;
    data.get("pages")
        .and_then(Value::as_array)
        .and_then(|pages| pages.get(page.saturating_sub(1)))
        .and_then(|page| page.get("cid"))
        .and_then(Value::as_u64)
        .or_else(|| data.get("cid").and_then(Value::as_u64))
}

fn select_media_url(value: Option<&Value>, preserve_original: bool) -> Option<String> {
    let data = value?;
    data.get("durl")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("url"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            (!preserve_original)
                .then(|| {
                    data.get("dash")
                        .and_then(|dash| dash.get("audio"))
                        .and_then(Value::as_array)
                        .and_then(|items| items.first())
                        .and_then(stream_url)
                })
                .flatten()
        })
        .or_else(|| {
            (!preserve_original)
                .then(|| {
                    data.get("dash")
                        .and_then(|dash| dash.get("video"))
                        .and_then(Value::as_array)
                        .and_then(|items| items.first())
                        .and_then(stream_url)
                })
                .flatten()
        })
}

fn stream_url(value: &Value) -> Option<String> {
    ["baseUrl", "base_url", "url"]
        .iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str).map(str::to_owned))
}

fn wbi_mixin_key(value: &Value) -> Option<String> {
    let data = value.get("data")?.get("wbi_img")?;
    let img = data.get("img_url").and_then(Value::as_str)?;
    let sub = data.get("sub_url").and_then(Value::as_str)?;
    let lookup = format!("{}{}", file_stem(img)?, file_stem(sub)?);
    let mixed = WBI_MIXIN_KEY_ENC_TAB
        .iter()
        .filter_map(|index| lookup.chars().nth(*index))
        .take(32)
        .collect::<String>();
    (!mixed.is_empty()).then_some(mixed)
}

fn file_stem(value: &str) -> Option<&str> {
    value.rsplit('/').next()?.split('.').next()
}

fn api_url(path: &str, params: &BTreeMap<String, String>) -> String {
    let query = encode_params(params);
    if query.is_empty() {
        format!("https://api.bilibili.com/{path}")
    } else {
        format!("https://api.bilibili.com/{path}?{query}")
    }
}

fn encode_params(params: &BTreeMap<String, String>) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in params {
        serializer.append_pair(key, value);
    }
    serializer.finish()
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn extract_video_ref(value: &str) -> Option<VideoRef> {
    let url = url::Url::parse(value).ok()?;
    let mut result = VideoRef {
        bvid: None,
        aid: None,
    };
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "bvid" if value.starts_with("BV") => result.bvid = Some(value.into_owned()),
            "aid" if value.chars().all(|character| character.is_ascii_digit()) => {
                result.aid = Some(value.into_owned())
            }
            _ => {}
        }
    }
    for segment in url.path_segments().into_iter().flatten() {
        let segment = segment.trim_end_matches('/');
        if result.bvid.is_none() && segment.starts_with("BV") && segment.len() >= 3 {
            result.bvid = Some(segment.to_owned());
        }
        if result.aid.is_none()
            && segment.len() > 2
            && segment.starts_with("av")
            && segment[2..]
                .chars()
                .all(|character| character.is_ascii_digit())
        {
            result.aid = Some(segment[2..].to_owned());
        }
    }
    (result.bvid.is_some() || result.aid.is_some()).then_some(result)
}

impl VideoRef {
    fn view_params(&self) -> BTreeMap<String, String> {
        let mut params = BTreeMap::new();
        if let Some(bvid) = &self.bvid {
            params.insert("bvid".into(), bvid.clone());
        } else if let Some(aid) = &self.aid {
            params.insert("aid".into(), aid.clone());
        }
        params
    }

    fn player_params(&self, cid: u64) -> BTreeMap<String, String> {
        let mut params = BTreeMap::new();
        if let Some(bvid) = &self.bvid {
            params.insert("bvid".into(), bvid.clone());
        }
        if let Some(aid) = &self.aid {
            params.insert("aid".into(), aid.clone());
        }
        params.insert("cid".into(), cid.to_string());
        params
    }
}

fn api_unavailable(message: &'static str) -> BackendError {
    BackendError::new("IMPORT_WEB_STRUCTURE_CHANGED", message, true, true)
}

fn classify_view_api_error(view: &Value) -> Option<BackendError> {
    match view.get("code").and_then(Value::as_i64) {
        Some(0) => None,
        Some(-101) => Some(BackendError::new(
            "IMPORT_WEB_LOGIN_REQUIRED",
            "Bilibili requires login before this video can be accessed.",
            false,
            true,
        )),
        Some(-352 | -412) => Some(BackendError::new(
            "IMPORT_WEB_CHALLENGE_DETECTED",
            "Bilibili blocked the request with a verification challenge.",
            false,
            true,
        )),
        Some(-404 | 62002 | 62004) => Some(BackendError::new(
            "IMPORT_WEB_CONTENT_REMOVED",
            "The Bilibili video is unavailable, private, or no longer published.",
            false,
            true,
        )),
        Some(_) | None => Some(api_unavailable(
            "Bilibili did not return public video metadata.",
        )),
    }
}

fn cancelled() -> BackendError {
    BackendError::new(
        "IMPORT_V2_CANCELLED",
        "Bilibili import was cancelled.",
        false,
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        classify_view_api_error, decode_json_transport, extract_platform_document,
        extract_player_subtitles, extract_video_ref, select_media_url, wbi_mixin_key,
    };
    use crate::services::import_v2::platform_provider::Platform;
    use crate::services::import_v2::platform_provider::PlatformSubtitleKind;
    use crate::services::import_v2::{
        engine::{EngineOperation, EngineRequest},
        subtitle::render_subtitle_markdown,
    };
    use crate::{
        models::import_v2::{ImportInput, ImportInputKind, MediaSaveMode},
        tasks::task_model::CancellationToken,
    };
    use serde_json::json;
    use std::io::Write;

    #[test]
    fn decodes_gzip_transport_used_by_bilibili_json_endpoints() {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(br#"{"code":0}"#).unwrap();
        let compressed = encoder.finish().unwrap();
        assert_eq!(
            decode_json_transport(&compressed).unwrap(),
            br#"{"code":0}"#
        );
    }

    #[test]
    fn extracts_bilibili_video_ids_from_bv_and_av_urls() {
        assert_eq!(
            extract_video_ref("https://www.bilibili.com/video/BV1abc123?p=2"),
            Some(super::VideoRef {
                bvid: Some("BV1abc123".into()),
                aid: None,
            })
        );
        assert_eq!(
            extract_video_ref("https://www.bilibili.com/video/av12345"),
            Some(super::VideoRef {
                bvid: None,
                aid: Some("12345".into()),
            })
        );
    }

    #[test]
    fn preserves_actionable_view_api_failures() {
        assert_eq!(
            classify_view_api_error(&json!({"code": -101}))
                .unwrap()
                .code,
            "IMPORT_WEB_LOGIN_REQUIRED"
        );
        assert_eq!(
            classify_view_api_error(&json!({"code": 62002}))
                .unwrap()
                .code,
            "IMPORT_WEB_CONTENT_REMOVED"
        );
        assert_eq!(
            classify_view_api_error(&json!({"code": -412}))
                .unwrap()
                .code,
            "IMPORT_WEB_CHALLENGE_DETECTED"
        );
        assert!(classify_view_api_error(&json!({"code": 0})).is_none());
    }

    #[test]
    fn selects_progressive_media_before_dash_tracks() {
        let value = json!({
            "durl": [{"url": "https://upos.example/video.mp4"}],
            "dash": {"audio": [{"baseUrl": "https://upos.example/audio.m4s"}]}
        });
        assert_eq!(
            select_media_url(Some(&value), true).as_deref(),
            Some("https://upos.example/video.mp4")
        );
    }

    #[test]
    fn dash_audio_is_only_used_for_extract_only_asr() {
        let value = json!({
            "dash": {
                "audio": [{"baseUrl": "https://upos.example/audio.m4s"}],
                "video": [{"baseUrl": "https://upos.example/video.m4s"}]
            }
        });
        assert_eq!(select_media_url(Some(&value), true), None);
        assert_eq!(
            select_media_url(Some(&value), false).as_deref(),
            Some("https://upos.example/audio.m4s")
        );
    }

    #[test]
    fn extracts_and_prioritizes_human_bilibili_subtitles() {
        let value = json!({
            "data": {"subtitle": {"subtitles": [
                {"lan": "zh-CN", "lan_doc": "中文", "subtitle_url": "https://sub.example/zh.json", "ai_status": 0},
                {"lan": "en", "lan_doc": "English (AI)", "subtitle_url": "//aisub.example/en.json", "ai_status": 1}
            ]}}
        });
        let subtitles = extract_player_subtitles(&value);
        assert_eq!(subtitles.len(), 2);
        assert_eq!(subtitles[0].language.as_deref(), Some("zh-CN"));
        assert!(!subtitles[0].automatic);
        assert_eq!(subtitles[1].url, "https://aisub.example/en.json");
        assert!(subtitles[1].automatic);
    }

    #[test]
    fn original_language_auto_subtitle_precedes_human_translation() {
        let value = json!({
            "data": {"subtitle": {"subtitles": [
                {"lan": "zh-CN", "lan_doc": "中文（自动生成）", "subtitle_url": "https://sub.example/zh-auto.json", "ai_status": 1},
                {"lan": "en", "lan_doc": "English", "subtitle_url": "https://sub.example/en-human.json", "ai_status": 0}
            ]}}
        });
        let subtitles = extract_player_subtitles(&value);
        assert_eq!(subtitles[0].language.as_deref(), Some("zh-CN"));
        assert!(subtitles[0].automatic);
        assert_eq!(subtitles[1].language.as_deref(), Some("en"));
        assert!(!subtitles[1].automatic);
    }

    #[test]
    fn subtitle_track_order_is_original_human_auto_then_other_languages() {
        let value = json!({
            "data": {"subtitle": {"subtitles": [
                {"lan": "zh-CN", "subtitle_url": "https://sub.example/zh-human.json", "ai_status": 0},
                {"lan": "en", "subtitle_url": "https://sub.example/en-auto.json", "ai_status": 1},
                {"lan": "ja", "subtitle_url": "https://sub.example/ja-human.json", "ai_status": 0},
                {"lan": "zh-CN", "subtitle_url": "https://sub.example/zh-auto.json", "ai_status": 1}
            ]}}
        });
        let subtitles = extract_player_subtitles(&value);
        assert_eq!(
            subtitles
                .iter()
                .map(|subtitle| (subtitle.language.as_deref().unwrap(), subtitle.automatic))
                .collect::<Vec<_>>(),
            vec![
                ("zh-CN", false),
                ("zh-CN", true),
                ("ja", false),
                ("en", true),
            ]
        );
    }

    #[test]
    fn only_machine_translated_subtitle_is_not_a_reliable_source_transcript() {
        let value = json!({
            "data": {"subtitle": {"subtitles": [
                {"lan": "en", "lan_doc": "English translation", "subtitle_url": "https://sub.example/en-auto.json", "ai_status": 1, "ai_type": 2}
            ]}}
        });
        let subtitles = extract_player_subtitles(&value);
        assert_eq!(subtitles.len(), 1);
        assert!(subtitles[0].automatic);
        assert_eq!(subtitles[0].kind, PlatformSubtitleKind::MachineTranslation);
        assert!(!subtitles[0].kind.is_reliable_source());
    }

    #[test]
    #[ignore = "requires the public Bilibili API"]
    fn public_bilibili_video_exposes_subtitles_or_an_asr_media_fallback() {
        let url = std::env::var("LLM_WIKI_BILIBILI_TEST_URL")
            .unwrap_or_else(|_| "https://www.bilibili.com/video/BV1N7411A7WU/".into());
        let request = EngineRequest {
            protocol_version: "2".into(),
            request_id: "network-fixture".into(),
            project_id: "network-fixture".into(),
            session_id: "network-fixture".into(),
            item_id: "network-fixture".into(),
            task_id: "network-fixture".into(),
            operation: EngineOperation::Extract,
            input: ImportInput {
                kind: ImportInputKind::Url,
                display_name: "Bilibili subtitle fixture".into(),
                locator: url.clone(),
                normalized_locator: Some(url.clone()),
                source_identity: None,
                media_save_mode: MediaSaveMode::ExtractOnly,
            },
            project_root: String::new(),
            staging_root: String::new(),
            chained_input: None,
            local_asr_authorized: true,
            asr_probe_only: false,
            asr_profile: None,
            recognition_language: None,
            selected_subtitle: None,
            local_ocr_authorized: false,
            media_save_mode: MediaSaveMode::ExtractOnly,
        };
        let token = CancellationToken::new();
        let result = super::fetch(&request, &token)
            .expect("the public Bilibili API request should succeed")
            .expect("the Bilibili URL should resolve to a video");
        if let Some(subtitle) = result.document.subtitles.first() {
            let (body, _) =
                super::fetch_json(&subtitle.url, &url, "network-fixture-subtitle", &token)
                    .expect("the subtitle evidence should download through the Rust broker");
            let markdown = render_subtitle_markdown(body.as_bytes(), "json")
                .expect("the Bilibili subtitle JSON should render as Markdown");
            assert!(markdown.contains("[00:"));
            assert!(markdown.lines().count() > 2);
        } else {
            assert_eq!(result.document.content_type, "video");
            assert!(
                result.document.media_url.is_some(),
                "a subtitle-free public video must expose media for authorized local ASR"
            );
        }
    }

    #[test]
    fn derives_the_wbi_mixin_key_from_nav_assets() {
        let value = json!({
            "data": {
                "wbi_img": {
                    "img_url": "https://i.example/01234567890123456789012345678901.png",
                    "sub_url": "https://i.example/abcdefghijklmnopqrstuvwxyzabcdef.png"
                }
            }
        });
        assert_eq!(wbi_mixin_key(&value).unwrap().len(), 32);
    }

    #[test]
    fn parses_a_raw_view_api_response_after_wrapping_it_as_json_script() {
        let raw = serde_json::json!({
            "code": 0,
            "data": {
                "bvid": "BV1abc123",
                "title": "API 标题",
                "desc": "API 描述",
                "owner": {"name": "作者"},
                "pages": [{"cid": 99, "part": "第一集"}],
                "pic": "https://i0.hdslb.com/bfs/archive/cover.jpg"
            }
        })
        .to_string();
        let wrapped = format!("<script type=\"application/json\">{raw}</script>");
        let document = extract_platform_document(
            Platform::Bilibili,
            &wrapped,
            "https://www.bilibili.com/video/BV1abc123",
        )
        .unwrap();
        assert_eq!(document.title, "API 标题");
        assert_eq!(document.author.as_deref(), Some("作者"));
        assert_eq!(document.content_type, "image_post");
    }
}
