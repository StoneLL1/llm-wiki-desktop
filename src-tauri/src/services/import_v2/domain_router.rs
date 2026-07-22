use crate::models::import_v2_web::{NormalizedWebUrl, WebRouteKind};
#[derive(Debug, Default, Clone)]
pub struct ConnectorAvailability {
    pub browser: bool,
    pub wechat: bool,
    pub zhihu: bool,
    pub bilibili: bool,
    pub xiaohongshu: bool,
    pub douyin: bool,
    pub phase_two: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebRoutePlan {
    pub primary: WebRouteKind,
    pub fallbacks: Vec<WebRouteKind>,
    pub concurrency_key: String,
    pub max_attempts_per_route: u8,
    pub release_enabled: bool,
}
pub struct DomainRouter;
impl DomainRouter {
    pub fn plan(url: &NormalizedWebUrl, a: &ConnectorAvailability) -> WebRoutePlan {
        let h = url.host.trim_end_matches('.').to_ascii_lowercase();
        let mut enabled = true;
        let primary = if host(&h, "mp.weixin.qq.com") && a.wechat {
            WebRouteKind::Wechat
        } else if (host(&h, "zhihu.com") || host(&h, "zhuanlan.zhihu.com")) && a.zhihu {
            WebRouteKind::Zhihu
        } else if (host(&h, "bilibili.com") || host(&h, "b23.tv")) && a.bilibili {
            WebRouteKind::Bilibili
        } else if (host(&h, "xiaohongshu.com") || host(&h, "xhslink.com")) && a.xiaohongshu {
            WebRouteKind::Xiaohongshu
        } else if (host(&h, "douyin.com") || host(&h, "iesdouyin.com")) && a.douyin {
            WebRouteKind::Douyin
        } else if host(&h, "x.com") || host(&h, "twitter.com") {
            enabled = a.phase_two;
            WebRouteKind::X
        } else {
            WebRouteKind::GenericHttp
        };
        let mut fallbacks = Vec::new();
        if primary != WebRouteKind::GenericHttp {
            fallbacks.push(WebRouteKind::GenericHttp);
        }
        if a.browser {
            fallbacks.push(WebRouteKind::GenericBrowser);
        }
        WebRoutePlan {
            primary,
            fallbacks,
            concurrency_key: h,
            max_attempts_per_route: 2,
            release_enabled: enabled,
        }
    }
}
fn host(actual: &str, expected: &str) -> bool {
    actual == expected
        || actual
            .strip_suffix(expected)
            .is_some_and(|p| p.ends_with('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(host: &str) -> NormalizedWebUrl {
        NormalizedWebUrl {
            public_url: format!("https://{host}/post/1"),
            host: host.into(),
            scheme: "https".into(),
        }
    }

    #[test]
    fn routes_media_platforms_to_dedicated_connectors_with_generic_fallbacks() {
        let availability = ConnectorAvailability {
            bilibili: true,
            xiaohongshu: true,
            douyin: true,
            browser: true,
            ..Default::default()
        };

        let bilibili = DomainRouter::plan(&url("www.bilibili.com"), &availability);
        assert_eq!(bilibili.primary, WebRouteKind::Bilibili);
        assert_eq!(
            bilibili.fallbacks,
            vec![WebRouteKind::GenericHttp, WebRouteKind::GenericBrowser]
        );

        assert_eq!(
            DomainRouter::plan(&url("www.xiaohongshu.com"), &availability).primary,
            WebRouteKind::Xiaohongshu
        );
        assert_eq!(
            DomainRouter::plan(&url("xhslink.com"), &availability).primary,
            WebRouteKind::Xiaohongshu
        );
        assert_eq!(
            DomainRouter::plan(&url("www.douyin.com"), &availability).primary,
            WebRouteKind::Douyin
        );
    }

    #[test]
    fn disabled_platform_connector_falls_back_to_generic_http() {
        let plan = DomainRouter::plan(&url("www.douyin.com"), &ConnectorAvailability::default());
        assert_eq!(plan.primary, WebRouteKind::GenericHttp);
        assert!(plan.release_enabled);
    }
}
