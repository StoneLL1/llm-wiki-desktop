use std::net::{IpAddr, Ipv4Addr};
use llm_wiki_desktop_lib::services::import_v2::url_policy::UrlPolicy;

#[test]
fn canonicalization_strips_secrets_and_rejects_private_connections() {
    let policy=UrlPolicy;
    let target=policy.normalize_for_session("HTTPS://Example.COM:443/a?utm_source=x&id=7&xsec_token=secret#frag").unwrap();
    assert_eq!(target.public.host,"example.com");
    assert!(target.public.public_url.contains("id=7"));
    assert!(!target.public.public_url.contains("secret") && !target.public.public_url.contains("frag"));
    let ip=IpAddr::V4(Ipv4Addr::new(127,0,0,1));
    assert!(policy.validate_resolved_target(&target,&[ip],ip,None,"item").is_err());
}

#[test]
fn connection_must_match_fresh_dns_set_and_redirect_is_reparsed() {
    let policy=UrlPolicy;
    let target=policy.normalize_for_session("https://example.com/a").unwrap();
    let connected=IpAddr::V4(Ipv4Addr::new(93,184,216,34));
    assert!(policy.validate_resolved_target(&target,&[IpAddr::V4(Ipv4Addr::new(93,184,216,35))],connected,None,"i").is_err());
    assert!(policy.validate_redirect(&target,"file:///etc/passwd").is_err());
}
