use llm_wiki_desktop_lib::{
    models::import_v2_web::{NormalizedWebUrl, WebRouteKind},
    services::import_v2::domain_router::{ConnectorAvailability, DomainRouter},
};
fn u(host: &str) -> NormalizedWebUrl {
    NormalizedWebUrl {
        public_url: format!("https://{host}/a"),
        host: host.into(),
        scheme: "https".into(),
    }
}
#[test]
fn platform_routes_precede_generic_and_matching_is_boundary_safe() {
    let a = ConnectorAvailability {
        browser: true,
        wechat: true,
        zhihu: true,
        bilibili: true,
        phase_two: false,
    };
    let p = DomainRouter::plan(&u("mp.weixin.qq.com"), &a);
    assert_eq!(p.primary, WebRouteKind::Wechat);
    assert_eq!(
        p.fallbacks,
        vec![WebRouteKind::GenericHttp, WebRouteKind::GenericBrowser]
    );
    assert_eq!(
        DomainRouter::plan(&u("mp.weixin.qq.com.example"), &a).primary,
        WebRouteKind::GenericHttp
    );
    assert!(!DomainRouter::plan(&u("www.xiaohongshu.com"), &a).release_enabled);
}
