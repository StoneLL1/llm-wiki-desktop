use llm_wiki_desktop_lib::services::import_v2::connectors::x;
#[test]
fn imports_only_exact_same_author_thread_and_is_gated() {
    let json = include_str!("../../tests/fixtures/import-v2/web/x/thread.json");
    assert!(x::extract_json(json, "https://x.com/alice/status/1", false).is_err());
    let d = x::extract_json(json, "https://x.com/alice/status/1?utm_source=x", true).unwrap();
    assert_eq!(d.posts.len(), 2);
    let persisted = serde_json::to_string(&d).unwrap();
    assert!(
        !persisted.contains("secret")
            && !persisted.contains("无关回复")
            && !persisted.contains("utm_source")
    );
}
