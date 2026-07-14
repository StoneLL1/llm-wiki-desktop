use crate::errors::{BackendError, IMPORT_V2_ENGINE_OUTPUT_INVALID};

pub fn decode_utf8(bytes: &[u8]) -> Result<&str, BackendError> {
    std::str::from_utf8(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes)).map_err(|_| {
        BackendError::new(
            IMPORT_V2_ENGINE_OUTPUT_INVALID,
            "The lightweight document is not valid UTF-8.",
            false,
            true,
        )
    })
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
