use llm_wiki_desktop_lib::services::import_v2::url_policy::UrlPolicy;
use std::net::{IpAddr, Ipv4Addr};

#[test]
fn canonicalization_strips_secrets_and_rejects_private_connections() {
    let policy = UrlPolicy;
    let target = policy
        .normalize_for_session("HTTPS://Example.COM:443/a?utm_source=x&id=7&xsec_token=secret#frag")
        .unwrap();
    assert_eq!(target.public.host, "example.com");
    assert!(target.public.public_url.contains("id=7"));
    assert!(
        !target.public.public_url.contains("secret") && !target.public.public_url.contains("frag")
    );
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    assert!(policy
        .validate_resolved_target(&target, &[ip], ip, None, "item")
        .is_err());
}

#[test]
fn connection_must_match_fresh_dns_set_and_redirect_is_reparsed() {
    let policy = UrlPolicy;
    let target = policy
        .normalize_for_session("https://example.com/a")
        .unwrap();
    let connected = IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34));
    assert!(policy
        .validate_resolved_target(
            &target,
            &[IpAddr::V4(Ipv4Addr::new(93, 184, 216, 35))],
            connected,
            None,
            "i"
        )
        .is_err());
    assert!(policy
        .validate_redirect(&target, "file:///etc/passwd")
        .is_err());
}

#[test]
fn public_query_is_allowlist_based_not_a_secret_denylist() {
    let target=UrlPolicy.normalize_for_session("https://example.com/a?id=7&api_key=a&client_secret=b&sessionid=c&X-Amz-Credential=d&code=e&Expires=9").unwrap();
    assert_eq!(target.public.public_url, "https://example.com/a?id=7");
}

#[test]
fn wechat_public_identity_keeps_account_mid_and_index_but_not_signature() {
    let target = UrlPolicy
        .normalize_for_session(
            "https://mp.weixin.qq.com/s?__biz=MzA1&mid=123&idx=2&sn=secret-signature",
        )
        .unwrap();
    assert!(target.public.public_url.contains("__biz=MzA1"));
    assert!(target.public.public_url.contains("mid=123"));
    assert!(target.public.public_url.contains("idx=2"));
    assert!(
        !target.public.public_url.contains("sn=")
            && !target.public.public_url.contains("secret-signature")
    );
}
