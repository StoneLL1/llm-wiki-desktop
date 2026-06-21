use std::net::{IpAddr, Ipv4Addr};
use url::{Host, Url};

pub fn is_valid_url(input: &str) -> bool {
    match Url::parse(input) {
        Ok(url) => matches!(url.scheme(), "http" | "https"),
        Err(_) => false,
    }
}

pub fn is_safe_remote_url(input: &str) -> bool {
    let Ok(url) = Url::parse(input) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    match url.host() {
        Some(Host::Domain(host)) => {
            !host.eq_ignore_ascii_case("localhost") && !host.ends_with(".localhost")
        }
        Some(Host::Ipv4(ip)) => is_public_ip(IpAddr::V4(ip)),
        Some(Host::Ipv6(ip)) => is_public_ip(IpAddr::V6(ip)),
        None => false,
    }
}

pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_ipv4(ip),
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4() {
                return is_public_ipv4(mapped);
            }
            !(ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.is_multicast())
        }
    }
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0
        || a == 10
        || a == 127
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 192 && b == 0 && (c == 0 || c == 2))
        || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
        || (a == 203 && b == 0 && c == 113)
        || a >= 224)
}

pub fn normalize_url(input: &str) -> Option<String> {
    Url::parse(input).ok().map(|url| url.to_string())
}

pub fn extract_url_host(input: &str) -> Option<String> {
    Url::parse(input)
        .ok()
        .and_then(|url| url.host_str().map(|h| h.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_valid_urls() {
        assert!(is_valid_url("https://example.com"));
        assert!(is_valid_url("http://example.com/path?query=1"));
        assert!(is_valid_url("https://sub.example.com/page.html"));
    }

    #[test]
    fn rejects_invalid_urls() {
        assert!(!is_valid_url("not-a-url"));
        assert!(!is_valid_url(""));
    }

    #[test]
    fn rejects_dangerous_url_schemes() {
        // Only http and https are accepted.
        assert!(!is_valid_url("file:///etc/passwd"));
        assert!(!is_valid_url("javascript:alert(1)"));
        assert!(!is_valid_url("data:text/html,<script>alert(1)</script>"));
        assert!(!is_valid_url("ftp://example.com/file"));
    }

    #[test]
    fn rejects_local_and_private_network_targets() {
        for value in [
            "http://localhost/admin",
            "http://127.0.0.1/",
            "http://10.1.2.3/",
            "http://172.16.0.1/",
            "http://192.168.1.1/",
            "http://169.254.169.254/latest/meta-data/",
            "http://[::1]/",
        ] {
            assert!(!is_safe_remote_url(value), "{value} must be rejected");
        }
        assert!(is_safe_remote_url("https://example.com/article"));
    }

    #[test]
    fn normalizes_url() {
        assert_eq!(
            normalize_url("https://example.com/path"),
            Some("https://example.com/path".to_string())
        );
    }

    #[test]
    fn extracts_host() {
        assert_eq!(
            extract_url_host("https://example.com/path"),
            Some("example.com".to_string())
        );
        assert_eq!(extract_url_host("not-a-url"), None);
    }
}
