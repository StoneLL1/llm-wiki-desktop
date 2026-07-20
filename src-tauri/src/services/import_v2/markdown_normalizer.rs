use crate::errors::{BackendError, IMPORT_V2_ENGINE_OUTPUT_INVALID};
use encoding_rs::{GB18030, UTF_16BE, UTF_16LE};

/// Decode lightweight text sources into UTF-8 without silently replacing
/// malformed input. UTF-8 remains preferred; BOM-marked UTF-16 and GB18030
/// cover the common Windows-editor Markdown variants.
pub fn decode_text(bytes: &[u8]) -> Result<String, BackendError> {
    let without_utf8_bom = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    if let Ok(value) = std::str::from_utf8(without_utf8_bom) {
        return Ok(value.to_owned());
    }

    if let Some(encoded) = bytes.strip_prefix(&[0xff, 0xfe]) {
        let (value, had_errors) = UTF_16LE.decode_without_bom_handling(encoded);
        if !had_errors {
            return Ok(value.into_owned());
        }
    } else if let Some(encoded) = bytes.strip_prefix(&[0xfe, 0xff]) {
        let (value, had_errors) = UTF_16BE.decode_without_bom_handling(encoded);
        if !had_errors {
            return Ok(value.into_owned());
        }
    }

    let (value, had_errors) = GB18030.decode_without_bom_handling(bytes);
    if !had_errors {
        return Ok(value.into_owned());
    }

    Err(BackendError::new(
        IMPORT_V2_ENGINE_OUTPUT_INVALID,
        "The lightweight document encoding is not recognized. Save it as UTF-8, UTF-16, or GB18030 and try again.",
        false,
        true,
    ))
}

pub fn normalize_markdown(value: &str) -> String {
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    format!("{}\n", normalized.trim_end_matches('\n'))
}

pub fn csv_to_gfm(value: &str) -> Result<String, BackendError> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(value.as_bytes());
    let headers = reader.headers().map_err(csv_error)?.clone();
    if headers.is_empty() {
        return Err(invalid_csv());
    }
    let mut output = row(headers.iter());
    output.push_str(&row((0..headers.len()).map(|_| "---")));
    for record in reader.records() {
        let record = record.map_err(csv_error)?;
        output.push_str(&row(
            (0..headers.len()).map(|index| record.get(index).unwrap_or(""))
        ));
    }
    Ok(output)
}

fn row<'a>(cells: impl IntoIterator<Item = &'a str>) -> String {
    let cells = cells
        .into_iter()
        .map(|cell| cell.replace('\n', "<br>").replace('|', "\\|"))
        .collect::<Vec<_>>();
    format!("| {} |\n", cells.join(" | "))
}
fn csv_error(_: csv::Error) -> BackendError {
    invalid_csv()
}
fn invalid_csv() -> BackendError {
    BackendError::new(
        IMPORT_V2_ENGINE_OUTPUT_INVALID,
        "The CSV file could not be parsed safely.",
        false,
        true,
    )
}

pub fn html_to_markdown(value: &str) -> (String, Vec<String>) {
    let lower = value.to_ascii_lowercase();
    let mut warnings = Vec::new();
    if lower.contains("<script") {
        warnings.push("HTML_SCRIPT_REMOVED".into());
    }
    if lower.contains("<style") {
        warnings.push("HTML_STYLE_REMOVED".into());
    }
    if lower.split('<').skip(1).any(|tag| {
        tag.split('>')
            .next()
            .unwrap_or("")
            .split_ascii_whitespace()
            .any(|part| part.starts_with("on") && part.contains('='))
    }) {
        warnings.push("HTML_EVENT_HANDLER_REMOVED".into());
    }
    if ["javascript:", "vbscript:", "data:"]
        .iter()
        .any(|scheme| lower.contains(scheme))
    {
        warnings.push("HTML_UNSAFE_URI_REMOVED".into());
    }
    let clean = remove_element(&remove_element(value, "script"), "style");
    let mut output = String::new();
    let mut cursor = 0;
    while let Some(relative) = clean[cursor..].find('<') {
        let start = cursor + relative;
        output.push_str(&decode_entities(&clean[cursor..start]));
        let Some(relative_end) = clean[start..].find('>') else {
            break;
        };
        let end = start + relative_end;
        render_tag(clean[start + 1..end].trim(), &mut output);
        cursor = end + 1;
    }
    output.push_str(&decode_entities(&clean[cursor..]));
    (normalize_markdown(&collapse_blank_lines(&output)), warnings)
}

fn remove_element(value: &str, name: &str) -> String {
    let mut output = String::new();
    let mut rest = value;
    loop {
        let lower = rest.to_ascii_lowercase();
        let Some(start) = lower.find(&format!("<{name}")) else {
            output.push_str(rest);
            break;
        };
        output.push_str(&rest[..start]);
        let Some(close) = lower[start..].find(&format!("</{name}>")) else {
            break;
        };
        rest = &rest[start + close + name.len() + 3..];
    }
    output
}

fn render_tag(tag: &str, output: &mut String) {
    let closing = tag.starts_with('/');
    let body = tag.trim_start_matches('/').trim();
    let name = body
        .split_ascii_whitespace()
        .next()
        .unwrap_or("")
        .trim_end_matches('/')
        .to_ascii_lowercase();
    match (closing, name.as_str()) {
        (false, "h1") => output.push_str("\n# "),
        (false, "h2") => output.push_str("\n## "),
        (false, "h3") => output.push_str("\n### "),
        (false, "h4") => output.push_str("\n#### "),
        (false, "h5") => output.push_str("\n##### "),
        (false, "h6") => output.push_str("\n###### "),
        (false, "li") => output.push_str("\n- "),
        (false, "br") => output.push('\n'),
        (false, "img") => {
            let uri = attribute(body, "data-src")
                .or_else(|| attribute(body, "src"))
                .filter(|uri| safe_uri(uri));
            if let Some(uri) = uri {
                let alt = attribute(body, "alt")
                    .unwrap_or_default()
                    .replace(']', "\\]")
                    .replace(['\r', '\n'], " ");
                output.push_str(&format!("![{alt}]({uri})"));
            }
        }
        (false, "a") => {
            if let Some(uri) = attribute(body, "href").filter(|uri| safe_uri(uri)) {
                output.push_str(&format!("[link]({uri})"));
            }
        }
        (
            true,
            "p" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "ul" | "ol" | "table" | "tr",
        ) => output.push_str("\n\n"),
        _ => {}
    }
}
fn attribute(tag: &str, wanted: &str) -> Option<String> {
    for quote in ['\'', '"'] {
        let needle = format!("{wanted}={quote}");
        if let Some(start) = tag.to_ascii_lowercase().find(&needle) {
            let value = &tag[start + needle.len()..];
            return value.find(quote).map(|end| value[..end].to_string());
        }
    }
    None
}
fn safe_uri(uri: &str) -> bool {
    !["javascript:", "vbscript:", "data:"]
        .iter()
        .any(|scheme| uri.trim().to_ascii_lowercase().starts_with(scheme))
}
fn decode_entities(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}
fn collapse_blank_lines(value: &str) -> String {
    let mut output = String::new();
    let mut blank = false;
    for line in value.lines() {
        if line.trim().is_empty() {
            if blank {
                continue;
            }
            blank = true;
        } else {
            blank = false;
        }
        output.push_str(line.trim_end());
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{decode_text, html_to_markdown};
    use encoding_rs::GB18030;

    #[test]
    fn decodes_utf8_with_or_without_bom() {
        assert_eq!(decode_text(b"\xef\xbb\xbf# title\n").unwrap(), "# title\n");
        assert_eq!(decode_text("# 标题\n".as_bytes()).unwrap(), "# 标题\n");
    }

    #[test]
    fn decodes_utf16_with_bom() {
        assert_eq!(
            decode_text(b"\xff\xfe#\0 \0t\0i\0t\0l\0e\0\n\0").unwrap(),
            "# title\n"
        );
    }

    #[test]
    fn decodes_gb18030_markdown() {
        let (encoded, _, had_errors) = GB18030.encode("# 标题\n");
        assert!(!had_errors);
        assert_eq!(decode_text(&encoded).unwrap(), "# 标题\n");
    }

    #[test]
    fn rejects_unrecognized_binary_bytes() {
        assert!(decode_text(&[0xff, 0xfe, 0x00]).is_err());
    }

    #[test]
    fn converts_images_to_markdown_and_prefers_lazy_data_source() {
        let (markdown, warnings) = html_to_markdown(
            r#"<p>Before</p><img data-src="https://cdn.example/image.jpg" src="placeholder.gif" alt="封面图"><p>After</p>"#,
        );

        assert!(warnings.is_empty());
        assert!(markdown.contains("![封面图](https://cdn.example/image.jpg)"));
        assert!(!markdown.contains("placeholder.gif"));
    }

    #[test]
    fn drops_unsafe_image_urls() {
        let (markdown, warnings) = html_to_markdown(
            r#"<img src="data:image/png;base64,not-for-wiki" alt="unsafe"><p>text</p>"#,
        );

        assert!(warnings.contains(&"HTML_UNSAFE_URI_REMOVED".to_string()));
        assert!(!markdown.contains("![unsafe]"));
        assert!(markdown.contains("text"));
    }
}
