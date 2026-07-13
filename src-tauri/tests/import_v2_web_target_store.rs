use llm_wiki_desktop_lib::services::{
    import_v2::{
        url_policy::{PrivateTargetGrant, UrlPolicy},
        web_target_store::{asr_target_sha256, BilibiliAsrGrant, WebTargetStore},
    },
    SecretService,
};

#[test]
fn exact_signed_target_lives_only_behind_an_opaque_secret_reference() {
    let store = WebTargetStore::new(SecretService::memory());
    let target = UrlPolicy
        .normalize_for_session(
            "https://www.xiaohongshu.com/explore/1?xsec_token=very-secret#fragment",
        )
        .unwrap();
    let reference = store.store(&target).unwrap();
    assert!(reference.starts_with("import-web-target:") && !reference.contains("secret"));
    let resolved = store
        .resolve(&reference, Some(&target.public.public_url))
        .unwrap();
    assert!(resolved.request_url.as_str().contains("very-secret"));
    assert!(
        !resolved.public.public_url.contains("secret")
            && !resolved.public.public_url.contains("fragment")
    );
    store.delete(&reference).unwrap();
    assert!(store
        .resolve(&reference, Some(&target.public.public_url))
        .is_err());
}

#[test]
fn private_grant_is_scoped_to_one_item_and_consumed_once() {
    let store = WebTargetStore::new(SecretService::memory());
    store.authorize_private(PrivateTargetGrant {
        item_id: "item-a".into(),
        scheme: "http".into(),
        host: "intranet.example".into(),
        port: 80,
        resolved_ips: vec!["10.0.0.8".parse().unwrap()],
        expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
    }).unwrap();
    assert!(store.take_private("item-b").unwrap().is_none());
    assert!(store.take_private("item-a").unwrap().is_some());
    assert!(store.take_private("item-a").unwrap().is_none());
}

#[test]
fn secure_reference_cannot_be_rebound_to_another_public_target() {
    let store = WebTargetStore::new(SecretService::memory());
    let target = UrlPolicy
        .normalize_for_session("https://example.com/a?token=secret")
        .unwrap();
    let reference = store.store(&target).unwrap();
    let error = match store.resolve(&reference, Some("https://example.com/other")) {
        Ok(_) => panic!("reference rebinding must fail"),
        Err(error) => error,
    };
    assert_eq!(error.code, "IMPORT_V2_URL_REFERENCE_MISMATCH");
}

#[test]
fn bilibili_asr_grant_is_exact_expiring_and_single_use() {
    let store = WebTargetStore::new(SecretService::memory());
    let exact_url = "https://www.bilibili.com/video/BV1exact?token=secret-a";
    let grant = BilibiliAsrGrant {
        project_id: "project-a".into(),
        session_id: "session-a".into(),
        item_id: "item-a".into(),
        target_sha256: asr_target_sha256(exact_url),
        expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
    };
    store.authorize_bilibili_asr(grant.clone()).unwrap();
    assert!(store
        .take_bilibili_asr(
            "project-a",
            "session-a",
            "item-b",
            exact_url,
        )
        .unwrap()
        .is_none());
    let mismatch = store
        .take_bilibili_asr(
            "project-a",
            "session-a",
            "item-a",
            "https://www.bilibili.com/video/BV1exact?token=secret-b",
        )
        .unwrap_err();
    assert_eq!(mismatch.code, "IMPORT_V2_URL_REFERENCE_MISMATCH");
    assert!(store.has_bilibili_asr("project-a", "session-a", "item-a", exact_url).unwrap());
    assert!(store.reserve_bilibili_asr("project-a", "session-a", "item-a", exact_url).unwrap());
    assert!(!store.has_bilibili_asr("project-a", "session-a", "item-a", exact_url).unwrap());
    assert!(!store.reserve_bilibili_asr("project-a", "session-a", "item-a", exact_url).unwrap());
    assert_eq!(
        store
            .take_bilibili_asr(
                "project-a",
                "session-a",
                "item-a",
                exact_url,
            )
            .unwrap(),
        Some(grant),
    );
    assert!(store
        .take_bilibili_asr(
            "project-a",
            "session-a",
            "item-a",
            exact_url,
        )
        .unwrap()
        .is_none());
}

#[test]
fn expired_bilibili_asr_grant_is_not_returned() {
    let store = WebTargetStore::new(SecretService::memory());
    store
        .authorize_bilibili_asr(BilibiliAsrGrant {
            project_id: "project-a".into(),
            session_id: "session-a".into(),
            item_id: "item-a".into(),
            target_sha256: asr_target_sha256("https://www.bilibili.com/video/BV1expired"),
            expires_at: chrono::Utc::now() - chrono::Duration::seconds(1),
        })
        .unwrap();
    assert!(store
        .take_bilibili_asr(
            "project-a",
            "session-a",
            "item-a",
            "https://www.bilibili.com/video/BV1expired",
        )
        .unwrap()
        .is_none());
}

#[test]
fn authenticated_profiles_cannot_cross_project_session_or_item() {
    let store = WebTargetStore::new(SecretService::memory());
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join("profile");
    std::fs::create_dir(&profile).unwrap();
    store.bind_authenticated_profile("project-a", "session-a", "item-a", profile.clone()).unwrap();
    assert!(store.take_authenticated_profile("project-b", "session-a", "item-a").unwrap().is_none());
    assert!(store.take_authenticated_profile("project-a", "session-b", "item-a").unwrap().is_none());
    assert!(store.take_authenticated_profile("project-a", "session-a", "item-b").unwrap().is_none());
    assert_eq!(store.take_authenticated_profile("project-a", "session-a", "item-a").unwrap(), Some(profile));
    assert!(store.take_authenticated_profile("project-a", "session-a", "item-a").unwrap().is_none());
}
