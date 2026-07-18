use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use chrono::{DateTime, Utc};
use url::Url;

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
        if resolved.is_empty() || !resolved.contains(&connected) {
            return Err(rejected(
                "Connected address was not in the validated DNS result.",
            ));
        }
        let unique = resolved.iter().copied().collect::<HashSet<_>>();
        for ip in unique {
            if is_blocked(ip) && !grant_allows(grant, target, ip, item_id) {
                return Err(rejected(
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
