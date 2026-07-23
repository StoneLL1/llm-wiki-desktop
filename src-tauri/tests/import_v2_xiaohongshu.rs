use llm_wiki_desktop_lib::services::import_v2::connectors::{xiaohongshu, ConnectorFailure};
#[test]
fn phase_gate_and_signed_url_secrecy_are_enforced() {
    let json = include_str!("../../tests/fixtures/import-v2/web/xiaohongshu/note.json");
    let url = "https://www.xiaohongshu.com/explore/1?xsec_token=target-secret";
    assert!(xiaohongshu::extract_json(json, url, false).is_err());
    let d = xiaohongshu::extract_json(json, url, true).unwrap();
    assert!(d.authenticated_request_url.contains("target-secret"));
    let persisted = serde_json::to_string(&d).unwrap();
    assert!(!persisted.contains("secret") && !persisted.contains("xsec_token"));
}

#[test]
fn real_initial_state_shape_preserves_text_order_and_private_asset_requests() {
    let html = include_str!("../../tests/fixtures/import-v2/web/xiaohongshu/image-note.html");
    let document = xiaohongshu::extract_page(
        html,
        "https://www.xiaohongshu.com/explore/67f00abc1234?xsec_token=target-secret",
    )
    .unwrap();
    assert_eq!(document.platform_id.as_deref(), Some("67f00abc1234"));
    assert_eq!(document.title, "周末读书记录");
    assert_eq!(document.title_source, "platform");
    assert_eq!(document.author.as_deref(), Some("作者甲"));
    assert_eq!(document.content_type, "image_post");
    assert_eq!(document.hashtags, vec!["#读书", "#知识库", "#结构化话题"]);
    assert!(document
        .published_at
        .as_deref()
        .is_some_and(|value| value.contains('T') && value.ends_with("+00:00")));
    assert_eq!(
        document.images.len(),
        2,
        "duplicate image identities are removed"
    );
    assert!(document.images[0].contains("001.jpg"));
    assert!(document.images[1].contains("002.jpg"));
    assert!(document.images[0].contains("first-secret"));
    assert!(!document.canonical_url.contains("target-secret"));
}

#[test]
fn declared_video_without_a_playable_stream_fails_closed() {
    let html = r#"<script>{"note":{"noteId":"video1","type":"video","title":"视频笔记","desc":"正文","imageList":[{"urlDefault":"https://sns-img-qc.xhscdn.com/cover.jpg"}],"video":{"changedStreamShape":true}}}</script>"#;
    assert_eq!(
        xiaohongshu::extract_page(html, "https://www.xiaohongshu.com/explore/video1").unwrap_err(),
        ConnectorFailure::StructureChanged
    );
}

#[test]
fn video_note_keeps_cover_separate_from_playable_media() {
    let html = include_str!("../../tests/fixtures/import-v2/web/xiaohongshu/video-note.html");
    let document = xiaohongshu::extract_page(
        html,
        "https://www.xiaohongshu.com/discovery/item/67f00video1234",
    )
    .unwrap();
    assert_eq!(document.content_type, "video");
    assert_eq!(
        document.cover_url.as_deref(),
        Some("https://sns-webpic-qc.xhscdn.com/video-cover.jpg")
    );
    assert!(document
        .media_url
        .as_deref()
        .is_some_and(|url| url.contains("video.mp4") && url.contains("video-secret")));
    assert_ne!(document.media_url, document.cover_url);
}

#[test]
fn missing_title_is_explicitly_marked_as_inferred() {
    let html = include_str!("../../tests/fixtures/import-v2/web/xiaohongshu/inferred-title.html");
    let document =
        xiaohongshu::extract_page(html, "https://www.xiaohongshu.com/explore/67f00infer1234")
            .unwrap();
    assert_eq!(document.title, "没有独立标题的第一行正文");
    assert_eq!(document.title_source, "inferred");
}

#[test]
fn requested_note_id_never_falls_back_to_recommendations() {
    let html = r#"<script>{"feed":[{"noteId":"other","title":"推荐内容","desc":"不应导入","imageList":[{"urlDefault":"https://sns-webpic-qc.xhscdn.com/wrong.jpg"}]}]}</script>"#;
    assert_eq!(
        xiaohongshu::extract_page(html, "https://www.xiaohongshu.com/explore/requested-note")
            .unwrap_err(),
        ConnectorFailure::StructureChanged
    );
}

#[test]
fn a_user_profile_without_a_note_id_never_imports_its_first_feed_item() {
    let html = r#"<script>{"user":{"id":"author-1","title":"作者主页","desc":"个人简介"},"feed":[{"noteId":"recommended","title":"推荐内容","desc":"不应导入"}]}</script>"#;
    assert_eq!(
        xiaohongshu::extract_page(html, "https://www.xiaohongshu.com/user/profile/author-1")
            .unwrap_err(),
        ConnectorFailure::StructureChanged
    );
}

#[test]
fn captcha_login_and_removed_pages_have_distinct_failures() {
    for (fixture, expected) in [
        (
            include_str!("../../tests/fixtures/import-v2/web/xiaohongshu/captcha.html"),
            ConnectorFailure::Captcha,
        ),
        (
            include_str!("../../tests/fixtures/import-v2/web/xiaohongshu/login-required.html"),
            ConnectorFailure::LoginRequired,
        ),
        (
            include_str!("../../tests/fixtures/import-v2/web/xiaohongshu/removed.html"),
            ConnectorFailure::Removed,
        ),
    ] {
        assert_eq!(xiaohongshu::classify_page(fixture), Some(expected));
    }
}
