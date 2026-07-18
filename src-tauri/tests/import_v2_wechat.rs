use llm_wiki_desktop_lib::services::import_v2::connectors::{wechat, ConnectorFailure};
use llm_wiki_desktop_lib::services::import_v2::ImportV2Service;
#[test]
fn extracts_article_and_keeps_signed_image_only_in_memory() {
    let html = include_str!("../../tests/fixtures/import-v2/web/wechat/article.html");
    let d = wechat::extract(html, "https://mp.weixin.qq.com/s/id?utm_source=x").unwrap();
    assert_eq!(d.title, "微信样例");
    assert!(d.image_requests[0].request_url.contains("secret"));
    let persisted = serde_json::to_string(&d).unwrap();
    assert!(!persisted.contains("secret") && !persisted.contains("utm_source"));
}
#[test]
fn detects_challenge_before_success() {
    assert_eq!(
        wechat::extract(
            include_str!("../../tests/fixtures/import-v2/web/wechat/challenge.html"),
            "https://mp.weixin.qq.com/s/id"
        )
        .unwrap_err(),
        ConnectorFailure::Challenge
    );
}
#[test]
fn extracts_the_complete_nested_article_body_instead_of_the_first_closing_div() {
    let html = r#"<h1 id="activity-name">Nested article</h1><div id="js_content"><p>First paragraph.</p><div><p>Second paragraph.</p></div></div>"#;
    let document = wechat::extract(html, "https://mp.weixin.qq.com/s/nested").unwrap();
    assert!(document.body_html.contains("First paragraph."));
    assert!(document.body_html.contains("Second paragraph."));
}

#[test]
fn default_import_service_registers_the_specialized_wechat_route() {
    assert!(ImportV2Service::default()
        .registered_engine_routes()
        .unwrap()
        .contains(&"web.wechat.article".to_string()));
}
