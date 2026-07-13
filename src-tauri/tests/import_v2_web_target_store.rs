use llm_wiki_desktop_lib::services::{
    import_v2::{url_policy::{PrivateTargetGrant, UrlPolicy}, web_target_store::WebTargetStore},
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
