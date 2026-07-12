use llm_wiki_desktop_lib::models::import_v2_web::{
    NormalizedWebUrl, WebRecoveryAction, WebRouteKind,
};

#[test]
fn web_contracts_are_public_only_and_stable() {
    let value = serde_json::to_value(NormalizedWebUrl {
        public_url: "https://example.com/article?id=7".into(),
        host: "example.com".into(),
        scheme: "https".into(),
    }).unwrap();
    assert_eq!(value["publicUrl"], "https://example.com/article?id=7");
    assert_eq!(serde_json::to_value(WebRouteKind::Xiaohongshu).unwrap(), "xiaohongshu");
    assert_eq!(serde_json::to_value(WebRecoveryAction::BeginLogin).unwrap(), "begin_login");
    let text = value.to_string().to_ascii_lowercase();
    assert!(!text.contains("token") && !text.contains("fragment"));
}
