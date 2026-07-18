use super::{between, ConnectorDocument, ConnectorFailure, ImageRequest};
use crate::services::import_v2::url_policy::UrlPolicy;

const WECHAT_HOST: &str = "mp.weixin.qq.com";

pub fn is_wechat_target(url: &str) -> bool {
    url::Url::parse(url)
        .ok()
        .and_then(|value| value.host_str().map(str::to_ascii_lowercase))
        .is_some_and(|host| host == WECHAT_HOST)
}

pub fn is_challenge_html(html: &str) -> bool {
    let lower = html.to_ascii_lowercase();
    lower.contains("环境异常")
        || lower.contains("访问过于频繁")
        || lower.contains("请完成验证")
        || (lower.contains("去验证") && lower.contains("验证"))
        || lower.contains("weixin.qq.com/cgi-bin/verify")
        || lower.contains("verify_wxpay")
}

pub fn extract(html: &str, url: &str) -> Result<ConnectorDocument, ConnectorFailure> {
    if is_challenge_html(html) {
        return Err(ConnectorFailure::Challenge);
    }
    let lower = html.to_ascii_lowercase();
    if lower.contains("文章已被删除") {
        return Err(ConnectorFailure::Removed);
    }

    let title = element_body_by_id(html, "activity-name")
        .map(|value| strip_markup(value).trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or(ConnectorFailure::StructureChanged)?;
    let body = element_body_by_id(html, "js_content")
        .filter(|value| value.trim().len() >= 8)
        .ok_or(ConnectorFailure::EmptyBody)?;
    let target = UrlPolicy
        .normalize_for_session(url)
        .map_err(|_| ConnectorFailure::StructureChanged)?;
    let (clean, image_requests) = sanitize_images(body);

    Ok(ConnectorDocument {
        title,
        author: element_body_by_id(html, "js_name")
            .map(strip_markup)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        published_at: between(html, "var ct = \"", "\"").map(str::to_string),
        body_html: clean,
        public_url: target.public.public_url,
        image_requests,
    })
}

fn sanitize_images(body: &str) -> (String, Vec<ImageRequest>) {
    let mut image_requests = Vec::new();
    let mut clean = body.to_string();
    for attribute in ["data-src=\"", "data-src='", "src=\"", "src='"] {
        let mut rest = body;
        while let Some(start) = rest.find(attribute) {
            let value_start = start + attribute.len();
            let quote = attribute.chars().last().unwrap_or('"');
            let Some(end) = rest[value_start..].find(quote) else {
                break;
            };
            let raw = &rest[value_start..value_start + end];
            let normalized = if raw.starts_with("//") {
                format!("https:{raw}")
            } else {
                raw.to_string()
            };
            if let Ok(target) = UrlPolicy.normalize_for_session(&normalized) {
                clean = clean.replace(raw, &target.public.public_url);
                if !image_requests
                    .iter()
                    .any(|image: &ImageRequest| image.public_url == target.public.public_url)
                {
                    image_requests.push(ImageRequest {
                        request_url: target.request_url.to_string(),
                        public_url: target.public.public_url,
                    });
                }
            }
            rest = &rest[value_start + end + 1..];
        }
    }
    (clean, image_requests)
}

fn element_body_by_id<'a>(html: &'a str, id: &str) -> Option<&'a str> {
    let lower = html.to_ascii_lowercase();
    let marker = format!("id=\"{id}\"");
    let marker_start = lower
        .find(&marker)
        .or_else(|| lower.find(&format!("id='{id}'")))?;
    let open_start = html[..marker_start].rfind('<')?;
    let open_end = html[marker_start..].find('>')? + marker_start;
    let open_tag = &html[open_start..=open_end];
    let tag_name = open_tag
        .trim_start_matches('<')
        .split_ascii_whitespace()
        .next()?
        .trim_end_matches('>')
        .trim_end_matches('/')
        .to_ascii_lowercase();
    if tag_name.is_empty() || open_tag.starts_with("</") || open_tag.ends_with("/>") {
        return None;
    }

    let lower_tail = &lower[open_end + 1..];
    let open_marker = format!("<{tag_name}");
    let close_marker = format!("</{tag_name}");
    let mut cursor = 0;
    let mut depth = 1usize;
    while cursor < lower_tail.len() {
        let next_open = lower_tail[cursor..].find(&open_marker);
        let next_close = lower_tail[cursor..].find(&close_marker);
        let (offset, closing) = match (next_open, next_close) {
            (None, None) => return None,
            (Some(open), None) => (open, false),
            (None, Some(close)) => (close, true),
            (Some(open), Some(close)) if open < close => (open, false),
            (Some(_), Some(close)) => (close, true),
        };
        let absolute = cursor + offset;
        if closing {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(&html[open_end + 1..open_end + 1 + absolute]);
            }
        } else if lower_tail[absolute..].find('>').is_some_and(|end| {
            !lower_tail[absolute..=absolute + end].ends_with("/>")
                && !is_void_tag(&lower_tail[absolute..absolute + tag_name.len() + 1])
        }) {
            depth += 1;
        }
        cursor = absolute + tag_name.len() + 1;
    }
    None
}

fn is_void_tag(tag: &str) -> bool {
    matches!(tag, "<img" | "<br" | "<hr" | "<meta" | "<link" | "<input")
}

fn strip_markup(value: &str) -> String {
    let (markdown, _) = crate::services::import_v2::markdown_normalizer::html_to_markdown(value);
    markdown.trim().to_string()
}
