use llm_wiki_desktop_lib::models::import_v2_presentation::GetImportPreviewContentV2Request;

#[test]
fn preview_content_request_is_identity_bound_and_never_accepts_a_path() {
    let request = GetImportPreviewContentV2Request {
        project_id: "p1".into(),
        project_root_path: "D:/wiki".into(),
        session_id: "s1".into(),
        item_id: "i1".into(),
        candidate_id: None,
        history_batch_id: None,
    };
    let value = serde_json::to_value(request).unwrap();
    assert_eq!(value["sessionId"], "s1");
    assert_eq!(value["itemId"], "i1");
    assert!(value.get("relativePath").is_none());
    assert!(value.get("absolutePath").is_none());
    assert!(value.get("stagingPath").is_none());
}
