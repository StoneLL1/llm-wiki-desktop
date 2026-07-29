use llm_wiki_desktop_lib::services::import_v2::connector_session::ConnectorSessionService;
#[test]
fn sessions_use_dedicated_opaque_profiles_and_revoke_them() {
    let root = tempfile::tempdir().unwrap();
    let s = ConnectorSessionService::default();
    let r = s.create("wechat", root.path()).unwrap();
    let serialized = serde_json::to_value(&r).unwrap();
    assert!(serialized.get("profileRef").is_none());
    assert!(!serialized
        .to_string()
        .contains(root.path().to_string_lossy().as_ref()));
    s.revoke(&r.session_id).unwrap();
}
#[test]
fn daily_profile_paths_are_denied() {
    let s = ConnectorSessionService::default();
    assert!(s
        .create(
            "x",
            std::path::Path::new("C:/Users/a/AppData/Google/Chrome/User Data")
        )
        .is_err());
}
#[test]
fn resume_cannot_spoof_authentication_before_browser_attestation() {
    let root = tempfile::tempdir().unwrap();
    let s = ConnectorSessionService::default();
    let r = s.create("zhihu", root.path()).unwrap();
    assert!(s.resume(&r.session_id).is_err());
    s.revoke(&r.session_id).unwrap();
}
