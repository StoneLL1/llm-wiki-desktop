use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::errors::{BackendError, IMPORT_V2_QUALITY_FAILED};
use crate::models::import_v2::{
    ArtifactKind, ImportArtifact, ImportPreviewArtifact, QualityLevel, QualityMetric, QualityReport,
};
use crate::services::import_v2::engine::EngineResult;

#[cfg(test)]
thread_local! {
    static BEFORE_ARTIFACT_OPEN: std::cell::RefCell<Option<Box<dyn FnOnce(&Path)>>> =
        std::cell::RefCell::new(None);
}

#[cfg(test)]
fn set_before_artifact_open(hook: impl FnOnce(&Path) + 'static) {
    BEFORE_ARTIFACT_OPEN.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

#[cfg(test)]
fn run_before_artifact_open(path: &Path) {
    BEFORE_ARTIFACT_OPEN.with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook(path);
        }
    });
}

pub const MIN_TEXT_COVERAGE: f64 = 0.98;
pub const MIN_TABLE_CELL_ACCURACY: f64 = 0.95;
const MAX_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Default)]
pub struct QualityGate;

impl QualityGate {
    pub fn validate_agent_text_fields<'a>(
        values: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), BackendError> {
        for value in values {
            validate_agent_secret_text(value)?;
        }
        Ok(())
    }

    pub fn validate_agent_asset(relative_path: &str, bytes: &[u8]) -> Result<(), BackendError> {
        let extension = Path::new(relative_path)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match extension.as_str() {
            "png" if bytes.starts_with(b"\x89PNG\r\n\x1a\n") => Ok(()),
            "jpg" | "jpeg" if bytes.starts_with(&[0xff, 0xd8, 0xff]) => Ok(()),
            "gif" if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") => Ok(()),
            "webp" if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" => {
                Ok(())
            }
            "json" => {
                let text = std::str::from_utf8(bytes).map_err(|_| quality_error())?;
                let _: serde_json::Value =
                    serde_json::from_str(text).map_err(|_| quality_error())?;
                validate_agent_secret_text(text)
            }
            "txt" | "csv" => {
                validate_agent_secret_text(std::str::from_utf8(bytes).map_err(|_| quality_error())?)
            }
            _ => Err(quality_error()),
        }
    }

    pub fn evaluate_agent_candidate(
        &self,
        staging_root: &Path,
        result: &EngineResult,
    ) -> Result<ImportPreviewArtifact, BackendError> {
        let preview = self.evaluate(staging_root, result)?;
        let markdown_path = staging_root.join(&preview.markdown.relative_path);
        let markdown = std::fs::read_to_string(markdown_path).map_err(|_| quality_error())?;
        validate_agent_secret_text(&markdown)?;
        if has_unclosed_fenced_block(&markdown) {
            return Err(quality_error());
        }
        Ok(preview)
    }

    pub fn evaluate(
        &self,
        staging_root: &Path,
        result: &EngineResult,
    ) -> Result<ImportPreviewArtifact, BackendError> {
        let markdown = read_artifact(staging_root, &result.markdown_path, ArtifactKind::Markdown)?;
        let markdown_content =
            String::from_utf8(markdown.bytes.clone()).map_err(|_| quality_error())?;
        let rendered_markdown = strip_code_contexts(&markdown_content);
        validate_markdown_content(&markdown_content, &rendered_markdown)?;

        let source_snapshot = read_artifact(
            staging_root,
            &result.source_snapshot_path,
            ArtifactKind::SourceSnapshot,
        )?;
        let mut assets = Vec::with_capacity(result.asset_paths.len());
        let declared_assets: HashSet<String> = result
            .asset_paths
            .iter()
            .map(|path| normalize_relative(path))
            .collect::<Result<_, _>>()?;
        if declared_assets.len() != result.asset_paths.len() {
            return Err(quality_error());
        }
        for asset_path in &result.asset_paths {
            assets.push(
                read_artifact(staging_root, asset_path, classify_asset(asset_path))?.artifact,
            );
        }
        if let Some(metadata_path) = &result.metadata_path {
            assets
                .push(read_artifact(staging_root, metadata_path, ArtifactKind::Metadata)?.artifact);
        }

        let mut warnings = result.warnings.clone();
        let local_images = image_destinations_from_rendered(&rendered_markdown);
        for destination in local_images {
            if is_remote(&destination) {
                push_warning(&mut warnings, "REMOTE_IMAGE");
            } else {
                let normalized = normalize_relative(destination.split('#').next().unwrap_or(""))?;
                if !declared_assets.contains(&normalized) {
                    return Err(quality_error());
                }
            }
        }

        let mut metrics = Vec::new();
        push_metric(
            &mut metrics,
            &mut warnings,
            "TEXT_COVERAGE",
            result.text_coverage,
            MIN_TEXT_COVERAGE,
        );
        push_metric(
            &mut metrics,
            &mut warnings,
            "TABLE_CELL_ACCURACY",
            result.table_cell_accuracy,
            MIN_TABLE_CELL_ACCURACY,
        );
        for (code, actual, minimum) in [
            ("SHEET_COUNT_EXACT", result.sheet_count_exact, 1.0),
            ("SLIDE_COUNT_EXACT", result.slide_count_exact, 1.0),
            (
                "NON_EMPTY_CELL_COVERAGE",
                result.non_empty_cell_coverage,
                0.95,
            ),
            ("FORMULA_VALUE_PAIRS", result.formula_value_pairs, 1.0),
            (
                "MEANINGFUL_IMAGE_COVERAGE",
                result.meaningful_image_coverage,
                0.95,
            ),
        ] {
            push_metric(&mut metrics, &mut warnings, code, actual, minimum);
        }
        let level = if warnings.is_empty() {
            QualityLevel::Pass
        } else {
            QualityLevel::Warning
        };

        Ok(ImportPreviewArtifact {
            markdown: markdown.artifact,
            assets,
            source_snapshot: source_snapshot.artifact,
            quality: QualityReport {
                level,
                metrics,
                warnings,
                sheet_count_exact: result.sheet_count_exact,
                slide_count_exact: result.slide_count_exact,
                non_empty_cell_coverage: result.non_empty_cell_coverage,
                formula_value_pairs: result.formula_value_pairs,
                meaningful_image_coverage: result.meaningful_image_coverage,
            },
            title: result.title.clone(),
            resolution: None,
            manual_merge: None,
        })
    }
}

fn validate_agent_secret_text(markdown: &str) -> Result<(), BackendError> {
    let lower = markdown.to_ascii_lowercase();
    let markers = [
        "-----begin private key-----",
        "authorization: bearer ",
        "api_key=",
        "api-key=",
        "ghp_",
    ];
    if markers.iter().any(|marker| lower.contains(marker))
        || markdown
            .split(|character: char| {
                !character.is_ascii_alphanumeric() && character != '-' && character != '_'
            })
            .any(|token| token.starts_with("sk-") && token.len() >= 20)
        || markdown
            .split_whitespace()
            .any(|token| token.starts_with("AKIA") && token.len() == 20)
    {
        return Err(quality_error());
    }
    Ok(())
}

fn has_unclosed_fenced_block(markdown: &str) -> bool {
    let mut fence: Option<(char, usize)> = None;
    for line in markdown.lines() {
        let Some((character, length, whitespace_tail)) = fence_marker(line) else {
            continue;
        };
        match fence {
            Some((active, active_len))
                if character == active && length >= active_len && whitespace_tail =>
            {
                fence = None
            }
            None => fence = Some((character, length)),
            _ => {}
        }
    }
    fence.is_some()
}

struct ReadArtifact {
    artifact: ImportArtifact,
    bytes: Vec<u8>,
}

fn read_artifact(
    staging_root: &Path,
    relative_path: &str,
    kind: ArtifactKind,
) -> Result<ReadArtifact, BackendError> {
    let normalized = normalize_relative(relative_path)?;
    let root = staging_root.canonicalize().map_err(|_| quality_error())?;
    let path = root.join(Path::new(&normalized));
    let mut component = root.clone();
    for part in Path::new(&normalized).components() {
        component.push(part.as_os_str());
        let metadata = std::fs::symlink_metadata(&component).map_err(|_| quality_error())?;
        if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
            return Err(quality_error());
        }
    }
    let canonical = path.canonicalize().map_err(|_| quality_error())?;
    if !canonical.starts_with(&root) || !canonical.is_file() {
        return Err(quality_error());
    }
    let before = std::fs::metadata(&canonical).map_err(|_| quality_error())?;
    if before.len() > MAX_ARTIFACT_BYTES {
        return Err(quality_error());
    }
    let validated_handle = same_file::Handle::from_path(&canonical).map_err(|_| quality_error())?;
    #[cfg(test)]
    run_before_artifact_open(&canonical);
    let file = std::fs::File::open(&canonical).map_err(|_| quality_error())?;
    let opened = file.metadata().map_err(|_| quality_error())?;
    let opened_handle =
        same_file::Handle::from_file(file.try_clone().map_err(|_| quality_error())?)
            .map_err(|_| quality_error())?;
    if validated_handle != opened_handle || !same_file(&before, &opened) {
        return Err(quality_error());
    }
    let mut bytes = Vec::with_capacity(opened.len() as usize);
    use std::io::Read;
    file.take(MAX_ARTIFACT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| quality_error())?;
    let after = std::fs::metadata(&canonical).map_err(|_| quality_error())?;
    let current_handle = same_file::Handle::from_path(&canonical).map_err(|_| quality_error())?;
    if bytes.len() as u64 > MAX_ARTIFACT_BYTES
        || before.len() != opened.len()
        || before.len() != bytes.len() as u64
        || before.len() != after.len()
        || !same_file(&opened, &after)
        || opened_handle != current_handle
    {
        return Err(quality_error());
    }
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    Ok(ReadArtifact {
        artifact: ImportArtifact {
            kind,
            relative_path: normalized,
            sha256,
            size_bytes: bytes.len() as u64,
        },
        bytes,
    })
}

#[cfg(unix)]
fn same_file(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(windows)]
fn same_file(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    left.creation_time() == right.creation_time()
        && left.last_write_time() == right.last_write_time()
        && left.file_size() == right.file_size()
}

#[cfg(not(any(unix, windows)))]
fn same_file(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    left.len() == right.len() && left.modified().ok() == right.modified().ok()
}

#[cfg(windows)]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}
#[cfg(not(windows))]
fn is_reparse_point(_: &std::fs::Metadata) -> bool {
    false
}

fn normalize_relative(value: &str) -> Result<String, BackendError> {
    let normalized = value.replace('\\', "/");
    let path = Path::new(&normalized);
    if normalized.trim().is_empty()
        || normalized.contains(':')
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir
                    | Component::CurDir
                    | Component::RootDir
                    | Component::Prefix(_)
            )
        })
    {
        return Err(quality_error());
    }
    Ok(normalized)
}

fn validate_markdown_content(markdown: &str, rendered: &str) -> Result<(), BackendError> {
    let lowercase = decode_html_entities(rendered).to_ascii_lowercase();
    let compact: String = lowercase
        .chars()
        .filter(|character| !character.is_ascii_whitespace() && !character.is_ascii_control())
        .collect();
    if markdown.trim().is_empty()
        || lowercase.contains("<script")
        || lowercase.contains("<iframe")
        || contains_unsafe_html_attribute(&lowercase)
        || compact.contains("javascript:")
        || compact.contains("vbscript:")
        || compact.contains("data:text/html")
    {
        return Err(quality_error());
    }
    Ok(())
}

fn image_destinations_from_rendered(rendered: &str) -> Vec<String> {
    let references = reference_definitions(&rendered);
    let mut destinations = inline_image_destinations(&rendered);
    destinations.extend(reference_image_destinations(&rendered, &references));
    destinations.extend(html_image_destinations(&rendered));
    destinations
}

fn strip_code_contexts(markdown: &str) -> String {
    let mut rendered = String::with_capacity(markdown.len());
    let mut fence: Option<(char, usize)> = None;
    for line in markdown.split_inclusive('\n') {
        let marker = fence_marker(line);
        if let Some((active_char, active_len)) = fence {
            if marker.is_some_and(|(character, length, whitespace_tail)| {
                character == active_char && length >= active_len && whitespace_tail
            }) {
                fence = None;
            }
            rendered.extend(std::iter::repeat_n(' ', line.len()));
            continue;
        }
        if let Some((character, length, _)) = marker {
            fence = Some((character, length));
            rendered.extend(std::iter::repeat_n(' ', line.len()));
            continue;
        }
        rendered.push_str(&strip_inline_code_and_escaped_images(line));
    }
    rendered
}

fn fence_marker(line: &str) -> Option<(char, usize, bool)> {
    let indentation = line.chars().take_while(|value| *value == ' ').count();
    if indentation > 3 {
        return None;
    }
    let candidate = &line[indentation..];
    let character = candidate
        .chars()
        .next()
        .filter(|value| matches!(value, '`' | '~'))?;
    let length = candidate
        .chars()
        .take_while(|value| *value == character)
        .count();
    let whitespace_tail = candidate[length..].chars().all(char::is_whitespace);
    (length >= 3).then_some((character, length, whitespace_tail))
}

fn strip_inline_code_and_escaped_images(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut output = bytes.to_vec();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' && bytes.get(index + 1) == Some(&b'!') {
            output[index + 1] = b' ';
            index += 2;
            continue;
        }
        if bytes[index] != b'`' {
            index += 1;
            continue;
        }
        let delimiter_start = index;
        while bytes.get(index) == Some(&b'`') {
            index += 1;
        }
        let delimiter_len = index - delimiter_start;
        let mut closing = index;
        let mut end = None;
        while closing < bytes.len() {
            if bytes[closing] != b'`' {
                closing += 1;
                continue;
            }
            let run_start = closing;
            while bytes.get(closing) == Some(&b'`') {
                closing += 1;
            }
            if closing - run_start == delimiter_len {
                end = Some(closing);
                break;
            }
        }
        if let Some(end) = end {
            output[delimiter_start..end].fill(b' ');
            index = end;
        } else {
            index = delimiter_start + delimiter_len;
        }
    }
    String::from_utf8(output).expect("input line was valid UTF-8")
}

fn inline_image_destinations(markdown: &str) -> Vec<String> {
    let mut destinations = Vec::new();
    let mut rest = markdown;
    while let Some(image_start) = rest.find("![") {
        rest = &rest[image_start + 2..];
        let Some(label_end) = rest.find("](") else {
            continue;
        };
        rest = &rest[label_end + 2..];
        let Some(destination_end) = rest.find(')') else {
            continue;
        };
        let raw = rest[..destination_end].trim();
        let destination = raw
            .strip_prefix('<')
            .and_then(|value| value.split_once('>').map(|pair| pair.0))
            .unwrap_or_else(|| raw.split_ascii_whitespace().next().unwrap_or(""));
        if !destination.is_empty() {
            destinations.push(destination.to_string());
        }
        rest = &rest[destination_end + 1..];
    }
    destinations
}

fn reference_definitions(markdown: &str) -> HashMap<String, String> {
    let mut definitions = HashMap::new();
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        let Some(label_end) = trimmed.strip_prefix('[').and_then(|value| value.find(']')) else {
            continue;
        };
        let after_label = &trimmed[label_end + 2..];
        let Some(raw) = after_label.strip_prefix(':') else {
            continue;
        };
        if let Some(destination) = destination_token(raw.trim_start()) {
            definitions.insert(
                trimmed[1..label_end + 1].trim().to_ascii_lowercase(),
                destination,
            );
        }
    }
    definitions
}

fn reference_image_destinations(
    markdown: &str,
    definitions: &HashMap<String, String>,
) -> Vec<String> {
    let mut destinations = Vec::new();
    let mut rest = markdown;
    while let Some(start) = rest.find("![") {
        rest = &rest[start + 2..];
        let Some(alt_end) = rest.find(']') else {
            break;
        };
        let alt = rest[..alt_end].trim();
        let after_alt = &rest[alt_end + 1..];
        if let Some(reference) = after_alt.strip_prefix('[') {
            if let Some(reference_end) = reference.find(']') {
                let label = reference[..reference_end].trim();
                let key = if label.is_empty() { alt } else { label }.to_ascii_lowercase();
                if let Some(destination) = definitions.get(&key) {
                    destinations.push(destination.clone());
                }
                rest = &reference[reference_end + 1..];
                continue;
            }
        }
        rest = after_alt;
    }
    destinations
}

fn html_image_destinations(markdown: &str) -> Vec<String> {
    let lowercase = markdown.to_ascii_lowercase();
    let mut destinations = Vec::new();
    let mut offset = 0;
    while let Some(relative_start) = lowercase[offset..].find("<img") {
        let start = offset + relative_start;
        let Some(relative_end) = lowercase[start..].find('>') else {
            break;
        };
        let end = start + relative_end + 1;
        if let Some(src) = html_attribute(&markdown[start..end], "src") {
            destinations.push(decode_html_entities(&src));
        }
        offset = end;
    }
    destinations
}

fn html_attribute(tag: &str, wanted: &str) -> Option<String> {
    let bytes = tag.as_bytes();
    let mut index = 0;
    if bytes.get(index) == Some(&b'<') {
        index += 1;
    }
    while bytes
        .get(index)
        .is_some_and(|byte| !byte.is_ascii_whitespace() && *byte != b'>')
    {
        index += 1;
    }
    while index < bytes.len() {
        while bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_whitespace() || matches!(byte, b'/' | b'>'))
        {
            index += 1;
        }
        let name_start = index;
        while bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            index += 1;
        }
        if name_start == index {
            index += 1;
            continue;
        }
        let name = &tag[name_start..index];
        while bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            index += 1;
        }
        if bytes.get(index) != Some(&b'=') {
            continue;
        }
        index += 1;
        while bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            index += 1;
        }
        let quote = bytes
            .get(index)
            .copied()
            .filter(|byte| matches!(byte, b'\'' | b'"'));
        if quote.is_some() {
            index += 1;
        }
        let value_start = index;
        while let Some(byte) = bytes.get(index) {
            if quote.map_or(byte.is_ascii_whitespace() || *byte == b'>', |quote| {
                *byte == quote
            }) {
                break;
            }
            index += 1;
        }
        if name.eq_ignore_ascii_case(wanted) {
            return Some(tag[value_start..index].to_string());
        }
        if quote.is_some() {
            index += 1;
        }
    }
    None
}

fn destination_token(raw: &str) -> Option<String> {
    if let Some(bracketed) = raw.strip_prefix('<') {
        return bracketed.find('>').map(|end| bracketed[..end].to_string());
    }
    raw.split_ascii_whitespace().next().map(str::to_string)
}

fn contains_unsafe_html_attribute(markdown: &str) -> bool {
    let mut rest = markdown;
    while let Some(start) = rest.find('<') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('>') else { break };
        let tag = &rest[..end];
        if html_attribute(tag, "srcdoc").is_some() || contains_event_handler(tag) {
            return true;
        }
        rest = &rest[end + 1..];
    }
    false
}

fn contains_event_handler(tag: &str) -> bool {
    let bytes = tag.as_bytes();
    let mut index = 0;
    while bytes
        .get(index)
        .is_some_and(|byte| !byte.is_ascii_whitespace())
    {
        index += 1;
    }
    while index < bytes.len() {
        while bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            index += 1;
        }
        let start = index;
        while bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            index += 1;
        }
        let name = &tag[start..index];
        while bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            index += 1;
        }
        if name.len() > 2
            && name.to_ascii_lowercase().starts_with("on")
            && bytes.get(index) == Some(&b'=')
        {
            return true;
        }
        if start == index {
            index += 1;
        }
        while bytes
            .get(index)
            .is_some_and(|byte| !byte.is_ascii_whitespace())
        {
            index += 1;
        }
    }
    false
}

fn decode_html_entities(value: &str) -> String {
    let mut decoded = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('&') {
        decoded.push_str(&rest[..start]);
        let entity = &rest[start + 1..];
        let Some(end) = entity.find(';') else {
            decoded.push_str(&rest[start..]);
            return decoded;
        };
        let name = &entity[..end];
        let replacement = name
            .strip_prefix("#x")
            .or_else(|| name.strip_prefix("#X"))
            .and_then(|digits| u32::from_str_radix(digits, 16).ok())
            .or_else(|| {
                name.strip_prefix('#')
                    .and_then(|digits| digits.parse().ok())
            })
            .and_then(char::from_u32)
            .or_else(|| match name.to_ascii_lowercase().as_str() {
                "colon" => Some(':'),
                "tab" => Some('\t'),
                "newline" => Some('\n'),
                _ => None,
            });
        if let Some(character) = replacement {
            decoded.push(character);
        } else {
            decoded.push('&');
            decoded.push_str(name);
            decoded.push(';');
        }
        rest = &entity[end + 1..];
    }
    decoded.push_str(rest);
    decoded
}

fn is_remote(destination: &str) -> bool {
    let lowercase = destination.to_ascii_lowercase();
    lowercase.starts_with("http://") || lowercase.starts_with("https://")
}

fn classify_asset(path: &str) -> ArtifactKind {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    let extension = PathBuf::from(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if normalized.starts_with("source-evidence/") {
        ArtifactKind::SourceEvidence
    } else if normalized.starts_with("ocr/") && extension == "md" {
        ArtifactKind::Transcript
    } else if normalized.starts_with("ocr/") && extension == "json" {
        ArtifactKind::Metadata
    } else if normalized.starts_with("transcripts/") && extension == "md" {
        ArtifactKind::Transcript
    } else if normalized.starts_with("transcripts/") && extension == "json" {
        ArtifactKind::Metadata
    } else if normalized.starts_with("subtitles/") && extension == "json" {
        ArtifactKind::Subtitle
    } else if matches!(
        extension.as_str(),
        "avif" | "gif" | "jpeg" | "jpg" | "png" | "svg" | "webp"
    ) {
        ArtifactKind::Image
    } else if matches!(extension.as_str(), "srt" | "vtt" | "lrc" | "ass" | "ssa") {
        ArtifactKind::Subtitle
    } else {
        ArtifactKind::Attachment
    }
}

fn push_metric(
    metrics: &mut Vec<QualityMetric>,
    warnings: &mut Vec<String>,
    code: &str,
    actual: Option<f64>,
    minimum: f64,
) {
    if let Some(actual) = actual.filter(|value| value.is_finite()) {
        let passed = actual >= minimum;
        metrics.push(QualityMetric {
            code: code.to_string(),
            actual,
            minimum,
            passed,
        });
        if !passed {
            let warning = match code {
                "TABLE_CELL_ACCURACY" => "LOW_TABLE_ACCURACY".to_string(),
                _ => format!("LOW_{code}"),
            };
            push_warning(warnings, &warning);
        }
    }
}

fn push_warning(warnings: &mut Vec<String>, warning: &str) {
    if !warnings.iter().any(|existing| existing == warning) {
        warnings.push(warning.to_string());
    }
}

fn quality_error() -> BackendError {
    BackendError::new(
        IMPORT_V2_QUALITY_FAILED,
        "Generated import artifacts failed deterministic quality validation.",
        true,
        true,
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::errors::IMPORT_V2_QUALITY_FAILED;
    use crate::models::import_v2::{ArtifactKind, QualityLevel};
    use crate::services::import_v2::engine::EngineResult;

    #[test]
    fn classifies_platform_json_subtitles_as_extracted_subtitles() {
        assert_eq!(
            classify_asset("subtitles/platform-subtitle-0.json"),
            ArtifactKind::Subtitle
        );
    }

    #[test]
    fn classifies_supplemental_api_payloads_as_source_evidence() {
        assert_eq!(
            classify_asset("source-evidence/bilibili-api.json"),
            ArtifactKind::SourceEvidence
        );
    }

    #[test]
    fn classifies_local_ocr_outputs_as_extracted_artifacts() {
        assert_eq!(classify_asset("ocr/image-0.md"), ArtifactKind::Transcript);
        assert_eq!(
            classify_asset("ocr/image-0.metadata.json"),
            ArtifactKind::Metadata
        );
    }

    struct QualityFixture {
        root: PathBuf,
        result: EngineResult,
    }

    impl Drop for QualityFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn quality_fixture(markdown: &str) -> QualityFixture {
        quality_fixture_with_metrics(markdown, 1.0, 1.0)
    }

    fn quality_fixture_with_metrics(markdown: &str, text: f64, table: f64) -> QualityFixture {
        let root = std::env::temp_dir().join(format!("quality-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("candidate.md"), markdown).unwrap();
        std::fs::write(root.join("source.bin"), b"source").unwrap();
        QualityFixture {
            root,
            result: EngineResult {
                source_snapshot_path: "source.bin".into(),
                markdown_path: "candidate.md".into(),
                asset_paths: Vec::new(),
                metadata_path: None,
                title: "Fixture".into(),
                text_coverage: Some(text),
                table_cell_accuracy: Some(table),
                sheet_count_exact: None,
                slide_count_exact: None,
                non_empty_cell_coverage: None,
                formula_value_pairs: None,
                meaningful_image_coverage: None,
                continuation: None,
                warnings: Vec::new(),
            },
        }
    }

    #[test]
    fn quality_gate_rejects_unsafe_or_empty_markdown() {
        for markdown in [
            "   ",
            "<script>alert(1)</script>",
            "[x](javascript:alert(1))",
            "[x](data:text/html,bad)",
        ] {
            let fixture = quality_fixture(markdown);
            let error = QualityGate::default()
                .evaluate(&fixture.root, &fixture.result)
                .unwrap_err();
            assert_eq!(error.code, IMPORT_V2_QUALITY_FAILED);
        }
    }

    #[test]
    fn quality_gate_warns_but_allows_low_coverage_preview() {
        let fixture = quality_fixture_with_metrics("# 标题\n\n正文", 0.91, 0.93);
        let preview = QualityGate::default()
            .evaluate(&fixture.root, &fixture.result)
            .unwrap();
        assert_eq!(preview.quality.level, QualityLevel::Warning);
        assert!(preview
            .quality
            .warnings
            .iter()
            .any(|warning| warning == "LOW_TEXT_COVERAGE"));
        assert!(preview
            .quality
            .warnings
            .iter()
            .any(|warning| warning == "LOW_TABLE_ACCURACY"));
    }

    #[test]
    fn quality_gate_requires_declared_existing_local_assets() {
        let mut missing = quality_fixture("![图](assets/图.png)");
        missing.result.asset_paths = vec!["assets/图.png".into()];
        assert_eq!(
            QualityGate::default()
                .evaluate(&missing.root, &missing.result)
                .unwrap_err()
                .code,
            IMPORT_V2_QUALITY_FAILED
        );

        let undeclared = quality_fixture("![图](assets/图.png)");
        std::fs::create_dir_all(undeclared.root.join("assets")).unwrap();
        std::fs::write(undeclared.root.join("assets/图.png"), b"png").unwrap();
        assert_eq!(
            QualityGate::default()
                .evaluate(&undeclared.root, &undeclared.result)
                .unwrap_err()
                .code,
            IMPORT_V2_QUALITY_FAILED
        );
    }

    #[test]
    fn quality_gate_rejects_path_escape_and_missing_source_snapshot() {
        let mut escaped = quality_fixture("# 标题");
        escaped.result.markdown_path = "../outside.md".into();
        assert_eq!(
            QualityGate::default()
                .evaluate(&escaped.root, &escaped.result)
                .unwrap_err()
                .code,
            IMPORT_V2_QUALITY_FAILED
        );

        let missing = quality_fixture("# 标题");
        std::fs::remove_file(missing.root.join("source.bin")).unwrap();
        assert_eq!(
            QualityGate::default()
                .evaluate(&missing.root, &missing.result)
                .unwrap_err()
                .code,
            IMPORT_V2_QUALITY_FAILED
        );
    }

    #[test]
    fn quality_gate_hashes_and_sizes_all_artifacts_and_preserves_cjk() {
        let mut fixture = quality_fixture("# 标题\n\n![图](assets/图.png)");
        std::fs::create_dir_all(fixture.root.join("assets")).unwrap();
        std::fs::write(fixture.root.join("assets/图.png"), b"image").unwrap();
        fixture.result.asset_paths = vec!["assets/图.png".into()];

        let preview = QualityGate::default()
            .evaluate(&fixture.root, &fixture.result)
            .unwrap();

        assert_eq!(preview.title, "Fixture");
        assert_eq!(preview.markdown.kind, ArtifactKind::Markdown);
        assert_eq!(preview.markdown.size_bytes, 32);
        assert_eq!(preview.assets[0].relative_path, "assets/图.png");
        assert_eq!(preview.assets[0].size_bytes, 5);
        assert_eq!(
            preview.assets[0].sha256,
            "6105d6cc76af400325e94d588ce511be5bfdbb73b437dc51eca43917d7a43e3d"
        );
        assert_eq!(preview.source_snapshot.size_bytes, 6);
    }

    #[test]
    fn quality_gate_warns_for_remote_images_and_engine_warnings() {
        let mut fixture = quality_fixture("# 标题\n\n![remote](https://example.com/image.png)");
        fixture.result.warnings.push("ENGINE_OCR_PARTIAL".into());
        let preview = QualityGate::default()
            .evaluate(&fixture.root, &fixture.result)
            .unwrap();
        assert_eq!(preview.quality.level, QualityLevel::Warning);
        assert!(preview
            .quality
            .warnings
            .iter()
            .any(|warning| warning == "REMOTE_IMAGE"));
        assert!(preview
            .quality
            .warnings
            .iter()
            .any(|warning| warning == "ENGINE_OCR_PARTIAL"));
    }

    #[test]
    fn quality_gate_validates_local_and_warns_remote_reference_images() {
        let mut local = quality_fixture("![图][asset]\n\n[asset]: assets/图.png");
        std::fs::create_dir_all(local.root.join("assets")).unwrap();
        std::fs::write(local.root.join("assets/图.png"), b"image").unwrap();
        local.result.asset_paths = vec!["assets/图.png".into()];
        assert_eq!(
            QualityGate::default()
                .evaluate(&local.root, &local.result)
                .unwrap()
                .quality
                .level,
            QualityLevel::Pass
        );

        let remote = quality_fixture("![remote][asset]\n\n[asset]: https://example.com/image.png");
        let preview = QualityGate::default()
            .evaluate(&remote.root, &remote.result)
            .unwrap();
        assert!(preview
            .quality
            .warnings
            .iter()
            .any(|warning| warning == "REMOTE_IMAGE"));
    }

    #[test]
    fn quality_gate_validates_local_and_warns_remote_html_images() {
        assert_eq!(
            html_image_destinations(r#"<IMG SRC='https://example.com/image.png'>"#),
            vec!["https://example.com/image.png"]
        );
        let mut local = quality_fixture(r#"<img alt="图" src="assets/图.png">"#);
        std::fs::create_dir_all(local.root.join("assets")).unwrap();
        std::fs::write(local.root.join("assets/图.png"), b"image").unwrap();
        local.result.asset_paths = vec!["assets/图.png".into()];
        assert_eq!(
            QualityGate::default()
                .evaluate(&local.root, &local.result)
                .unwrap()
                .quality
                .level,
            QualityLevel::Pass
        );

        let remote = quality_fixture(r#"<IMG SRC='https://example.com/image.png'>"#);
        let preview = QualityGate::default()
            .evaluate(&remote.root, &remote.result)
            .unwrap();
        assert!(preview
            .quality
            .warnings
            .iter()
            .any(|warning| warning == "REMOTE_IMAGE"));
    }

    #[test]
    fn quality_gate_rejects_active_html_and_obfuscated_unsafe_schemes() {
        assert!(contains_unsafe_html_attribute(
            r#"<div srcdoc="<p>unsafe</p>"></div>"#
        ));
        for markdown in [
            r#"<img src="safe.png" onerror="alert(1)">"#,
            r#"<div onclick = "alert(1)">unsafe</div>"#,
            r#"<iframe src="https://example.com"></iframe>"#,
            r#"<div srcdoc="<p>unsafe</p>"></div>"#,
            r#"[x](java&#x73;cript:alert(1))"#,
            r#"<img src="java&#115;cript:alert(1)">"#,
        ] {
            let fixture = quality_fixture(markdown);
            assert_eq!(
                QualityGate::default()
                    .evaluate(&fixture.root, &fixture.result)
                    .unwrap_err()
                    .code,
                IMPORT_V2_QUALITY_FAILED,
                "unsafe content should fail: {markdown}"
            );
        }
    }

    #[test]
    fn quality_gate_ignores_images_in_code_or_escaped_markdown() {
        for markdown in [
            "```markdown\n![not rendered](assets/missing.png)\n```",
            "Text `![not rendered](assets/missing.png)` text",
            r#"\![not rendered](assets/missing.png)"#,
            "```html\n<script>alert(1)</script>\n```",
        ] {
            let fixture = quality_fixture(markdown);
            let preview = QualityGate::default()
                .evaluate(&fixture.root, &fixture.result)
                .unwrap();
            assert_eq!(preview.quality.level, QualityLevel::Pass);
        }
    }

    #[test]
    fn unmatched_inline_backtick_keeps_rendered_resources_and_html_visible() {
        for markdown in [
            "` unmatched ![rendered](assets/missing.png)",
            "` unmatched <script>alert(1)</script>",
        ] {
            let fixture = quality_fixture(markdown);
            assert_eq!(
                QualityGate::default()
                    .evaluate(&fixture.root, &fixture.result)
                    .unwrap_err()
                    .code,
                IMPORT_V2_QUALITY_FAILED
            );
        }
    }

    #[test]
    fn escaped_bang_without_image_syntax_does_not_hide_unsafe_html() {
        for markdown in [
            r#"\! ordinary <script>alert(1)</script>)"#,
            r#"\![not an image](javascript:alert(1))"#,
        ] {
            let fixture = quality_fixture(markdown);
            assert_eq!(
                QualityGate::default()
                    .evaluate(&fixture.root, &fixture.result)
                    .unwrap_err()
                    .code,
                IMPORT_V2_QUALITY_FAILED
            );
        }
    }

    #[test]
    fn shorter_fence_does_not_close_a_longer_fence() {
        let fixture = quality_fixture(
            "````markdown\ncode\n```\n![not rendered](assets/missing.png)\n````\n# visible",
        );
        let preview = QualityGate::default()
            .evaluate(&fixture.root, &fixture.result)
            .unwrap();
        assert_eq!(preview.quality.level, QualityLevel::Pass);
    }

    #[test]
    fn four_space_indentation_does_not_open_a_fence_or_hide_unsafe_html() {
        let fixture = quality_fixture("    ```html\n<script>alert(1)</script>\n```");
        assert_eq!(
            QualityGate::default()
                .evaluate(&fixture.root, &fixture.result)
                .unwrap_err()
                .code,
            IMPORT_V2_QUALITY_FAILED
        );
    }

    #[test]
    fn trailing_non_whitespace_does_not_close_a_fence() {
        let fixture = quality_fixture(
            "```markdown\n``` trailing text\n![not rendered](assets/missing.png)\n```\n# visible",
        );
        let preview = QualityGate::default()
            .evaluate(&fixture.root, &fixture.result)
            .unwrap();
        assert_eq!(preview.quality.level, QualityLevel::Pass);
    }

    #[test]
    fn rejects_same_length_artifact_swap_between_validation_and_open() {
        let fixture = quality_fixture("# trusted\n");
        let replacement = fixture.root.join("replacement.md");
        std::fs::write(&replacement, b"# hostile\n").unwrap();
        assert_eq!(
            std::fs::metadata(fixture.root.join("candidate.md"))
                .unwrap()
                .len(),
            std::fs::metadata(&replacement).unwrap().len()
        );
        set_before_artifact_open(move |validated| {
            std::fs::remove_file(validated).unwrap();
            std::fs::rename(&replacement, validated).unwrap();
        });

        assert_eq!(
            QualityGate::default()
                .evaluate(&fixture.root, &fixture.result)
                .unwrap_err()
                .code,
            IMPORT_V2_QUALITY_FAILED
        );
    }

    #[test]
    fn agent_candidate_rejects_secret_corpus_and_unclosed_fence() {
        for markdown in [
            "# leaked\n\nAuthorization: Bearer secret-value",
            "# leaked\n\nsk-123456789012345678901234",
            "# malformed\n\n```text\nnever closed",
        ] {
            let fixture = quality_fixture(markdown);
            assert_eq!(
                QualityGate::default()
                    .evaluate_agent_candidate(&fixture.root, &fixture.result)
                    .unwrap_err()
                    .code,
                IMPORT_V2_QUALITY_FAILED
            );
        }
    }

    #[test]
    fn agent_candidate_assets_reject_secrets_active_svg_and_renamed_executables() {
        assert!(QualityGate::validate_agent_asset("notes.txt", b"api_key=do-not-persist").is_err());
        assert!(QualityGate::validate_agent_asset(
            "image.svg",
            b"<svg><script>alert(1)</script></svg>"
        )
        .is_err());
        assert!(QualityGate::validate_agent_asset("image.png", b"MZ executable").is_err());
        assert!(
            QualityGate::validate_agent_asset("image.png", b"\x89PNG\r\n\x1a\nminimal").is_ok()
        );
    }
}
