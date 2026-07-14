use llm_wiki_desktop_lib::services::import_v2::{
    connectors::bilibili::{self, LocalAsrPolicy},
    media_router::{MediaRouteStatus, SubtitleKind},
};

#[test]
fn metadata_uses_file_media_router_and_secrets_stay_in_memory() {
    let (document, plan) = bilibili::extract_json(
        include_str!("../../tests/fixtures/import-v2/web/bilibili/video.json"),
        "https://www.bilibili.com/video/BV1?spm=secret",
        LocalAsrPolicy::default(),
    )
    .unwrap();
    assert_eq!(plan.subtitle.unwrap().kind, SubtitleKind::HumanPlatform);
    assert!(document.subtitle_requests[0].contains("secret"));
    let persisted = serde_json::to_string(&document).unwrap();
    assert!(!persisted.contains("secret") && !persisted.contains("spm"));
}

#[test]
fn no_subtitle_requires_both_capability_and_explicit_authorization() {
    let json = r#"{"code":0,"data":{"title":"v","owner":{"name":"u"},"subtitles":[]}}"#;
    let (_, unavailable) = bilibili::extract_json(
        json,
        "https://bilibili.com/video/BV1",
        LocalAsrPolicy::default(),
    )
    .unwrap();
    let (_, unauthorized) = bilibili::extract_json(
        json,
        "https://bilibili.com/video/BV1",
        LocalAsrPolicy {
            capability_available: true,
            user_authorized: false,
        },
    )
    .unwrap();
    let (_, authorized) = bilibili::extract_json(
        json,
        "https://bilibili.com/video/BV1",
        LocalAsrPolicy {
            capability_available: true,
            user_authorized: true,
        },
    )
    .unwrap();
    assert_eq!(unavailable.status, MediaRouteStatus::WaitingCapability);
    assert_eq!(unauthorized.status, MediaRouteStatus::WaitingAuthorization);
    assert!(!unavailable.requires_asr && !unauthorized.requires_asr);
    assert!(authorized.requires_asr);
}
