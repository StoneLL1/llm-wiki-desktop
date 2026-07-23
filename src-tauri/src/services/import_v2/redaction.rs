const SENSITIVE_KEYS: &[&str] = &[
    "access_token",
    "authorization",
    "cookie",
    "expires",
    "expire",
    "msToken",
    "mstoken",
    "secret",
    "sessionid",
    "sid_guard",
    "web_session",
    "sessdata",
    "auth_key",
    "hdnts",
    "sign",
    "signature",
    "token",
    "xsec_token",
    "xsec-token",
    "data-token",
    "data-xsec-token",
    "x-bogus",
    "a_bogus",
    "verifyFp",
    "verify_fp",
    "ttwid",
    "odin_tt",
    "passport_csrf_token",
    "credential",
    "key-pair-id",
    "policy",
    "x-amz-credential",
    "x-amz-security-token",
    "x-amz-signature",
];

const PERSISTED_QUERY_KEYS: &[&str] = &["aid", "bvid", "cid", "id", "lang", "locale", "p", "page"];

pub(crate) fn redact_sensitive_text(input: &str) -> String {
    let meta_redacted = redact_sensitive_meta_tags(input);
    let input = meta_redacted.as_str();
    let lower = input.to_ascii_lowercase();
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    while cursor < input.len() {
        let found = SENSITIVE_KEYS
            .iter()
            .filter_map(|key| {
                lower[cursor..]
                    .find(&key.to_ascii_lowercase())
                    .map(|offset| (cursor + offset, *key))
            })
            .min_by_key(|(index, _)| *index);
        let Some((index, key)) = found else {
            output.push_str(&input[cursor..]);
            break;
        };
        let before = index
            .checked_sub(1)
            .and_then(|value| lower.as_bytes().get(value))
            .copied();
        let mut after_key = index + key.len();
        if before.is_some_and(|value| value.is_ascii_alphanumeric() || value == b'_') {
            let bytes = input.as_bytes();
            let mut token_start = index;
            while token_start > cursor && bytes[token_start - 1].is_ascii_alphanumeric() {
                token_start -= 1;
            }
            let mut token_end = after_key;
            while token_end < bytes.len() && bytes[token_end].is_ascii_alphanumeric() {
                token_end += 1;
            }
            if is_sensitive_json_key(&input[token_start..token_end]) {
                after_key = token_end;
            } else {
                output.push_str(&input[cursor..after_key]);
                cursor = after_key;
                continue;
            }
        }
        let Some((value_start, quoted)) = sensitive_value_start(input, after_key) else {
            output.push_str(&input[cursor..after_key]);
            cursor = after_key;
            continue;
        };
        let content_start = if quoted {
            value_start + input[value_start..].chars().next().unwrap().len_utf8()
        } else {
            value_start
        };
        let end = if quoted {
            input[content_start..]
                .char_indices()
                .find_map(|(offset, value)| {
                    (value == '\'' || value == '"').then_some(content_start + offset)
                })
                .unwrap_or(input.len())
        } else {
            input[content_start..]
                .char_indices()
                .find_map(|(offset, value)| {
                    matches!(
                        value,
                        '&' | '"' | '\'' | '<' | '>' | ' ' | '\r' | '\n' | ',' | '}'
                    )
                    .then_some(content_start + offset)
                })
                .unwrap_or(input.len())
        };
        output.push_str(&input[cursor..content_start]);
        output.push_str("REDACTED");
        cursor = end;
    }
    redact_unknown_url_queries(&output)
}

fn redact_sensitive_meta_tags(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    while let Some(offset) = lower[cursor..].find("<meta") {
        let start = cursor + offset;
        let Some(relative_end) = input[start..].find('>') else {
            break;
        };
        let end = start + relative_end + 1;
        output.push_str(&input[cursor..start]);
        let tag = &input[start..end];
        let sensitive = attribute_value_range(tag, "name")
            .or_else(|| attribute_value_range(tag, "property"))
            .is_some_and(|range| is_sensitive_json_key(&tag[range]));
        if sensitive {
            if let Some(range) = attribute_value_range(tag, "content") {
                output.push_str(&tag[..range.start]);
                output.push_str("REDACTED");
                output.push_str(&tag[range.end..]);
            } else {
                output.push_str(tag);
            }
        } else {
            output.push_str(tag);
        }
        cursor = end;
    }
    output.push_str(&input[cursor..]);
    output
}

fn attribute_value_range(tag: &str, attribute: &str) -> Option<std::ops::Range<usize>> {
    let lower = tag.to_ascii_lowercase();
    let mut search = 0;
    while let Some(offset) = lower[search..].find(attribute) {
        let start = search + offset;
        let before = start
            .checked_sub(1)
            .and_then(|index| lower.as_bytes().get(index));
        let after = lower.as_bytes().get(start + attribute.len());
        if before.is_some_and(|value| value.is_ascii_alphanumeric() || *value == b'-')
            || after.is_some_and(|value| value.is_ascii_alphanumeric() || *value == b'-')
        {
            search = start + attribute.len();
            continue;
        }
        let mut cursor = skip_ascii_whitespace(tag, start + attribute.len());
        if !tag[cursor..].starts_with('=') {
            search = start + attribute.len();
            continue;
        }
        cursor = skip_ascii_whitespace(tag, cursor + 1);
        let quote = tag[cursor..].chars().next()?;
        if quote != '"' && quote != '\'' {
            search = start + attribute.len();
            continue;
        }
        let value_start = cursor + quote.len_utf8();
        let value_end = tag[value_start..]
            .find(quote)
            .map(|offset| value_start + offset)?;
        return Some(value_start..value_end);
    }
    None
}

pub(crate) fn redact_json_snapshot(input: &str) -> Option<Vec<u8>> {
    let mut value: serde_json::Value = serde_json::from_str(input).ok()?;
    redact_json_value(&mut value);
    serde_json::to_vec(&value).ok()
}

fn redact_json_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(values) => {
            for (key, value) in values {
                if is_sensitive_json_key(key) {
                    *value = serde_json::Value::String("REDACTED".into());
                } else {
                    redact_json_value(value);
                }
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                redact_json_value(value);
            }
        }
        serde_json::Value::String(value) => {
            *value = redact_sensitive_text(value);
        }
        _ => {}
    }
}

fn is_sensitive_json_key(key: &str) -> bool {
    let normalized = key
        .bytes()
        .filter(|value| value.is_ascii_alphanumeric())
        .map(|value| value.to_ascii_lowercase() as char)
        .collect::<String>();
    SENSITIVE_KEYS.iter().any(|candidate| {
        let candidate = candidate
            .bytes()
            .filter(|value| value.is_ascii_alphanumeric())
            .map(|value| value.to_ascii_lowercase() as char)
            .collect::<String>();
        normalized == candidate || normalized.ends_with(&candidate)
    })
}

fn redact_unknown_url_queries(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    while cursor < input.len() {
        let next = ["https://", "http://"]
            .into_iter()
            .filter_map(|prefix| lower[cursor..].find(prefix).map(|offset| cursor + offset))
            .min();
        let Some(start) = next else {
            output.push_str(&input[cursor..]);
            break;
        };
        output.push_str(&input[cursor..start]);
        let end = input[start..]
            .char_indices()
            .find_map(|(offset, value)| {
                (offset > 0
                    && matches!(
                        value,
                        ' ' | '\t' | '\r' | '\n' | '"' | '\'' | '<' | '>' | '(' | ')' | '`'
                    ))
                .then_some(start + offset)
            })
            .unwrap_or(input.len());
        let candidate = &input[start..end];
        let trimmed_end = candidate.trim_end_matches([',', ';', '}', ']']);
        let suffix = &candidate[trimmed_end.len()..];
        if let Ok(mut url) = url::Url::parse(trimmed_end) {
            let retained = url
                .query_pairs()
                .filter(|(key, _)| {
                    PERSISTED_QUERY_KEYS
                        .iter()
                        .any(|allowed| key.eq_ignore_ascii_case(allowed))
                })
                .map(|(key, value)| (key.into_owned(), value.into_owned()))
                .collect::<Vec<_>>();
            url.set_query(None);
            url.set_fragment(None);
            if !retained.is_empty() {
                url.query_pairs_mut().extend_pairs(retained);
            }
            output.push_str(url.as_str());
            output.push_str(suffix);
        } else {
            output.push_str(candidate);
        }
        cursor = end;
    }
    output
}

fn sensitive_value_start(input: &str, after_key: usize) -> Option<(usize, bool)> {
    let mut cursor = skip_ascii_whitespace(input, after_key);
    if input[cursor..]
        .chars()
        .next()
        .is_some_and(|value| value == '"' || value == '\'')
    {
        cursor += input[cursor..].chars().next()?.len_utf8();
        cursor = skip_ascii_whitespace(input, cursor);
        if !input[cursor..].starts_with(':') {
            return None;
        }
        cursor += 1;
        cursor = skip_ascii_whitespace(input, cursor);
    } else if input[cursor..]
        .chars()
        .next()
        .is_some_and(|value| value == ':' || value == '=')
    {
        cursor += 1;
        cursor = skip_ascii_whitespace(input, cursor);
    } else {
        return None;
    }
    let quoted = input[cursor..]
        .chars()
        .next()
        .is_some_and(|value| value == '"' || value == '\'');
    Some((cursor, quoted))
}

fn skip_ascii_whitespace(input: &str, mut cursor: usize) -> usize {
    while let Some(value) = input[cursor..].chars().next() {
        if !value.is_ascii_whitespace() {
            break;
        }
        cursor += value.len_utf8();
    }
    cursor
}

#[cfg(test)]
mod tests {
    use super::{redact_json_snapshot, redact_sensitive_text};

    #[test]
    fn redacts_query_json_assignment_and_html_attribute_values() {
        let value = redact_sensitive_text(
            r#"https://cdn.example/a?xsec_token=signed&x=1 {"sign":"json-secret"} token = 'script-secret' data-token="attribute-secret""#,
        );
        assert!(!value.contains("signed"));
        assert!(!value.contains("json-secret"));
        assert!(!value.contains("script-secret"));
        assert!(!value.contains("attribute-secret"));
        // Unknown URL query fields are removed entirely, while secrets in
        // structured text retain an explicit marker.
        assert!(value.matches("REDACTED").count() >= 3);
    }

    #[test]
    fn strips_unknown_cloud_signing_query_fields_from_every_persisted_url() {
        let value = redact_sensitive_text(
            "https://cdn.example/video.mp4?id=42&Policy=secret&Key-Pair-Id=K123&X-Amz-Credential=AKIA&future-sig=unknown",
        );
        assert!(value.contains("id=42"));
        for secret in ["secret", "K123", "AKIA", "unknown", "future-sig"] {
            assert!(!value.contains(secret), "{secret}");
        }
    }

    #[test]
    fn json_snapshot_redaction_preserves_valid_json_and_catches_camel_case_keys() {
        let redacted = redact_json_snapshot(
            r#"{"accessToken":"top-secret","expires":123,"nested":{"mediaUrl":"https://cdn.example/video.mp4?id=42&future-signature=secret"},"title":"keep"}"#,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&redacted).unwrap();
        assert_eq!(value["accessToken"], "REDACTED");
        assert_eq!(value["expires"], "REDACTED");
        assert_eq!(value["title"], "keep");
        let media_url = value["nested"]["mediaUrl"].as_str().unwrap();
        assert!(media_url.contains("id=42"));
        assert!(!media_url.contains("secret"));
        assert!(!media_url.contains("future-signature"));
    }

    #[test]
    fn html_snapshot_redaction_catches_camel_case_and_meta_credentials() {
        let value = redact_sensitive_text(
            r#"<script>{"accessToken":"top-secret","csrfToken":"csrf-secret"}</script><meta name="csrf-token" content="meta-secret">"#,
        );
        for secret in ["top-secret", "csrf-secret", "meta-secret"] {
            assert!(!value.contains(secret), "{secret}");
        }
        assert_eq!(value.matches("REDACTED").count(), 3);
    }
}
