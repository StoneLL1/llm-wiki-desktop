use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use chrono::{DateTime, Utc};
use url::{Host, Url};

use crate::errors::BackendError;
use crate::models::import_v2_web::NormalizedWebUrl;

const PUBLIC_QUERY_KEYS: &[&str] = &["id", "p", "page", "article", "aid", "bvid", "mid"];

#[derive(Debug, Clone)]
pub struct SessionWebTarget {
    pub request_url: Url,
    pub public: NormalizedWebUrl,
}

#[derive(Debug, Clone)]
pub struct PrivateTargetGrant {
    pub item_id: String,
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub resolved_ips: Vec<IpAddr>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Default, Clone)]
pub struct UrlPolicy;

impl UrlPolicy {
    pub fn normalize_for_session(&self, raw: &str) -> Result<SessionWebTarget, BackendError> {
        let mut request_url = Url::parse(raw).map_err(|_| rejected("URL is invalid."))?;
        if !matches!(request_url.scheme(), "http" | "https") {
            return Err(rejected("Only HTTP and HTTPS URLs are allowed."));
        }
        if !request_url.username().is_empty() || request_url.password().is_some() {
            return Err(rejected("URL user information is not allowed."));
        }
        let host = request_url
            .host_str()
            .ok_or_else(|| rejected("URL host is required."))?
            .trim_end_matches('.')
            .to_ascii_lowercase();
        if matches!(request_url.host(), Some(Host::Domain(_))) {
            request_url
                .set_host(Some(&host))
                .map_err(|_| rejected("URL host is invalid."))?;
        }
        if host == "localhost" || host.ends_with(".localhost") {
            return Err(rejected("Local targets are blocked."));
        }
        request_url.set_fragment(None);
        let mut public = request_url.clone();
        let pairs = request_url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect::<Vec<_>>();
        public.set_query(None);
        for (key, value) in pairs {
            if is_public_key(&host, &key) {
                public.query_pairs_mut().append_pair(&key, &value);
            }
        }
        Ok(SessionWebTarget {
            request_url,
            public: NormalizedWebUrl {
                public_url: public.to_string(),
                host,
                scheme: public.scheme().into(),
            },
        })
    }

    pub fn validate_resolved_target(
        &self,
        target: &SessionWebTarget,
        resolved: &[IpAddr],
        connected: IpAddr,
        grant: Option<&PrivateTargetGrant>,
        item_id: &str,
    ) -> Result<(), BackendError> {
        self.validate_resolved_target_for_fetch(target, resolved, connected, grant, item_id, false)
    }

    pub(crate) fn validate_resolved_target_for_fetch(
        &self,
        target: &SessionWebTarget,
        resolved: &[IpAddr],
        connected: IpAddr,
        grant: Option<&PrivateTargetGrant>,
        item_id: &str,
        trusted_fake_ip_host: bool,
    ) -> Result<(), BackendError> {
        if resolved.is_empty() || !resolved.contains(&connected) {
            return Err(rejected(
                "Connected address was not in the validated DNS result.",
            ));
        }
        let tunneled_https_fake_ip =
            allows_tunneled_https_fake_ip(target, resolved, connected, trusted_fake_ip_host);
        let unique = resolved.iter().copied().collect::<HashSet<_>>();
        for ip in unique {
            if is_blocked(ip)
                && !(tunneled_https_fake_ip && is_benchmark_fake_ip(ip))
                && !grant_allows(grant, target, ip, item_id)
            {
                return Err(private_target_blocked(
                    "Private, local, metadata, multicast, and reserved targets are blocked.",
                ));
            }
        }
        Ok(())
    }

    pub fn validate_redirect(
        &self,
        current: &SessionWebTarget,
        location: &str,
    ) -> Result<SessionWebTarget, BackendError> {
        let joined = current
            .request_url
            .join(location)
            .map_err(|_| rejected("Redirect URL is invalid."))?;
        self.normalize_for_session(joined.as_str())
    }

    pub fn public_persistence_url<'a>(&self, target: &'a SessionWebTarget) -> &'a str {
        &target.public.public_url
    }
}

/// Fake-IP TUN resolvers commonly map public hostnames into the RFC 2544
/// benchmarking range and route those addresses through a local tunnel. The
/// caller may opt in only after restricting the request and every redirect to
/// a reviewed HTTPS host allowlist. Arbitrary hostnames and user-controlled
/// redirects must continue through the explicit private-target grant flow.
fn allows_tunneled_https_fake_ip(
    target: &SessionWebTarget,
    resolved: &[IpAddr],
    connected: IpAddr,
    trusted_fake_ip_host: bool,
) -> bool {
    trusted_fake_ip_host
        && target.request_url.scheme() == "https"
        && matches!(target.request_url.host(), Some(Host::Domain(_)))
        && is_benchmark_fake_ip(connected)
        && resolved.iter().copied().all(is_benchmark_fake_ip)
}

fn is_benchmark_fake_ip(ip: IpAddr) -> bool {
    matches!(ip, IpAddr::V4(v) if {
        let octets = v.octets();
        octets[0] == 198 && matches!(octets[1], 18 | 19)
    })
}

fn is_public_key(host: &str, key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    PUBLIC_QUERY_KEYS.contains(&k.as_str())
        || (host == "mp.weixin.qq.com" && matches!(k.as_str(), "__biz" | "idx"))
}
fn grant_allows(
    grant: Option<&PrivateTargetGrant>,
    target: &SessionWebTarget,
    ip: IpAddr,
    item_id: &str,
) -> bool {
    grant.is_some_and(|g| {
        g.item_id == item_id
            && g.scheme == target.public.scheme
            && g.host == target.public.host
            && g.port == target.request_url.port_or_known_default().unwrap_or(0)
            && g.expires_at > Utc::now()
            && g.resolved_ips.contains(&ip)
    })
}
pub fn is_blocked(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => blocked_v4(v),
        IpAddr::V6(v) => blocked_v6(v),
    }
}
fn blocked_v4(v: Ipv4Addr) -> bool {
    let o = v.octets();
    v.is_private()
        || v.is_loopback()
        || v.is_link_local()
        || v.is_multicast()
        || v.is_broadcast()
        || v.is_unspecified()
        || o[0] == 0
        || o[0] >= 224
        || o == [169, 254, 169, 254]
        || o[0] == 100 && (64..=127).contains(&o[1])
        || o[0] == 198 && (o[1] == 18 || o[1] == 19)
}
fn blocked_v6(v: Ipv6Addr) -> bool {
    let s = v.segments();
    v.is_loopback()
        || v.is_unspecified()
        || v.is_multicast()
        || (s[0] & 0xfe00) == 0xfc00
        || (s[0] & 0xffc0) == 0xfe80
        || (s[0] & 0xffc0) == 0xfec0
        || v.to_ipv4_mapped().is_some_and(blocked_v4)
}
fn rejected(message: &str) -> BackendError {
    BackendError::new("IMPORT_V2_URL_REJECTED", message, false, true)
}

fn private_target_blocked(message: &str) -> BackendError {
    BackendError::new("IMPORT_V2_PRIVATE_TARGET_BLOCKED", message, false, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    #[test]
    fn reviewed_https_host_accepts_an_all_fake_ip_tunnel_dns_set() {
        let target = UrlPolicy
            .normalize_for_session("https://www.bilibili.com/video/BV1example")
            .unwrap();
        let resolved = [ip(198, 18, 0, 50), ip(198, 19, 0, 51)];

        assert!(
            UrlPolicy
                .validate_resolved_target_for_fetch(
                    &target,
                    &resolved,
                    resolved[0],
                    None,
                    "item",
                    true,
                )
                .is_ok()
        );
    }

    #[test]
    fn generic_https_fake_ip_requires_explicit_private_authorization() {
        let target = UrlPolicy
            .normalize_for_session("https://example.com/article")
            .unwrap();
        let fake = ip(198, 18, 0, 50);

        let error = UrlPolicy
            .validate_resolved_target(&target, &[fake], fake, None, "item")
            .unwrap_err();

        assert_eq!(error.code, "IMPORT_V2_PRIVATE_TARGET_BLOCKED");
    }

    #[test]
    fn fake_ip_exception_rejects_http_literals_and_mixed_dns_sets() {
        let https_literal = UrlPolicy
            .normalize_for_session("https://198.18.0.50/article")
            .unwrap();
        let http_domain = UrlPolicy
            .normalize_for_session("http://example.com/article")
            .unwrap();
        let https_domain = UrlPolicy
            .normalize_for_session("https://example.com/article")
            .unwrap();
        let fake = ip(198, 18, 0, 50);
        let public = ip(93, 184, 216, 34);

        for (target, resolved) in [
            (&https_literal, vec![fake]),
            (&http_domain, vec![fake]),
            (&https_domain, vec![fake, public]),
        ] {
            assert!(UrlPolicy
                .validate_resolved_target_for_fetch(target, &resolved, fake, None, "item", true,)
                .is_err());
        }
    }

    #[test]
    fn fake_ip_exception_does_not_relax_other_private_ranges() {
        let target = UrlPolicy
            .normalize_for_session("https://example.com/article")
            .unwrap();
        let private = ip(192, 168, 1, 10);

        assert!(UrlPolicy
            .validate_resolved_target_for_fetch(&target, &[private], private, None, "item", true,)
            .is_err());
    }

    #[test]
    fn trailing_dot_domain_is_canonicalized_for_dns_pinning() {
        let target = UrlPolicy
            .normalize_for_session("https://Example.COM./article")
            .unwrap();

        assert_eq!(target.public.host, "example.com");
        assert_eq!(target.request_url.host_str(), Some("example.com"));
        assert_eq!(target.public.public_url, "https://example.com/article");
    }
}
