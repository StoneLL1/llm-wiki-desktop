use url::Url;

pub fn is_valid_url(input: &str) -> bool {
    Url::parse(input).is_ok()
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
