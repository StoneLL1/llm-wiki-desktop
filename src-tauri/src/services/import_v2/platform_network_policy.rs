use crate::errors::BackendError;
use crate::services::import_v2::platform_provider::Platform;
use crate::services::import_v2::url_policy::{SessionWebTarget, UrlPolicy};

/// Hosts that may participate in page navigation for a reviewed platform.
/// Asset/CDN hosts deliberately live outside this list and are fetched through
/// the separately constrained remote-asset path.
pub fn trusted_platform_page_host_suffixes(value: &str) -> &'static [&'static str] {
    if let Some(platform) = Platform::from_url(value) {
        return match platform {
            Platform::Bilibili => &["bilibili.com", "b23.tv"],
            Platform::Xiaohongshu => &["xiaohongshu.com", "xhslink.com", "xhslink.cn"],
            Platform::Douyin => &["douyin.com", "iesdouyin.com"],
        };
    }
    let Some(host) = url::Url::parse(value)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
    else {
        return &[];
    };
    if host == "mp.weixin.qq.com" {
        &["mp.weixin.qq.com"]
    } else if host == "zhihu.com" || host.ends_with(".zhihu.com") {
        &["zhihu.com"]
    } else if host == "x.com"
        || host.ends_with(".x.com")
        || host == "twitter.com"
        || host.ends_with(".twitter.com")
    {
        &["x.com", "twitter.com", "t.co"]
    } else {
        &[]
    }
}

/// Public platform links are accepted in HTTP form for compatibility, but the
/// reviewed navigation boundary itself is HTTPS-only. Upgrade before DNS
/// validation so short links such as `http://xhslink.cn/...` do not regress.
pub fn upgrade_trusted_platform_page_to_https(
    target: SessionWebTarget,
) -> Result<SessionWebTarget, BackendError> {
    if target.request_url.scheme() != "http"
        || target.request_url.port().is_some()
        || trusted_platform_page_host_suffixes(target.request_url.as_str()).is_empty()
    {
        return Ok(target);
    }
    let mut upgraded = target.request_url;
    upgraded
        .set_scheme("https")
        .expect("HTTP and HTTPS are hierarchical URL schemes");
    UrlPolicy.normalize_for_session(upgraded.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_hosts_exclude_platform_asset_cdns() {
        let bilibili =
            trusted_platform_page_host_suffixes("https://www.bilibili.com/video/BV1example");
        assert!(bilibili.contains(&"bilibili.com"));
        assert!(!bilibili.contains(&"bilivideo.com"));
        assert!(!bilibili.contains(&"hdslb.com"));
    }

    #[test]
    fn reviewed_http_shortlink_is_upgraded_before_fetch() {
        let target = UrlPolicy
            .normalize_for_session("http://xhslink.cn/o/example")
            .unwrap();
        let upgraded = upgrade_trusted_platform_page_to_https(target).unwrap();
        assert_eq!(upgraded.request_url.scheme(), "https");
        assert_eq!(upgraded.public.public_url, "https://xhslink.cn/o/example");
    }
}
