use sha2::{Digest, Sha256};

use crate::errors::{BackendError, IMPORT_V2_COMMIT_FAILED};
use crate::models::import_v2::{
    validate_source_frontmatter, ImportInputKind, QualityReport, SourceFrontmatter, SourcePageType,
};
use crate::services::import_v2::source_registry::{
    SourceCandidateRecord, SourceManifest, SourceVersion,
};
use crate::utils::markdown_utils::{parse_frontmatter, split_frontmatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateMetadata {
    pub source_kind: String,
    pub title: String,
    pub canonical_url: Option<String>,
    pub platform: Option<String>,
    pub platform_content_id: Option<String>,
    pub author: Option<String>,
    pub published_at: Option<String>,
    pub language: Option<String>,
}

pub struct CandidateInspection<'a> {
    pub input_kind: &'a ImportInputKind,
    pub display_name: &'a str,
    pub normalized_locator: &'a str,
    pub markdown: &'a [u8],
    pub metadata_documents: &'a [serde_json::Value],
}

pub struct FinalizationInput<'a> {
    pub candidate_markdown: &'a [u8],
    pub candidate: &'a CandidateMetadata,
    pub source_id: &'a str,
    pub version_id: &'a str,
    pub content_hash: &'a str,
    pub imported_at: &'a str,
    pub quality: &'a QualityReport,
    pub restricted: bool,
}

pub struct FinalizedSource {
    pub bytes: Vec<u8>,
    pub frontmatter: SourceFrontmatter,
    pub human_edit_hash: String,
}

pub fn inspect_candidate(
    input: CandidateInspection<'_>,
) -> Result<CandidateMetadata, BackendError> {
    let markdown = std::str::from_utf8(input.markdown).map_err(|_| finalization_error())?;
    let split = split_frontmatter(markdown);
    let frontmatter = split
        .frontmatter
        .as_deref()
        .map(parse_frontmatter)
        .unwrap_or_default();
    for forbidden in [
        "sourceId",
        "versionId",
        "contentHash",
        "version_id",
        "content_hash",
    ] {
        if frontmatter.get(forbidden).is_some() {
            return Err(finalization_error());
        }
    }

    let metadata_string = |keys: &[&str]| {
        input
            .metadata_documents
            .iter()
            .find_map(|value| find_string(value, keys))
    };
    let platform = metadata_string(&["platform"])
        .or_else(|| frontmatter.get_scalar("source_platform"))
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    if frontmatter.get("source_id").is_some() && platform.is_none() {
        return Err(finalization_error());
    }
    let canonical_url = metadata_string(&["finalPublicUrl", "canonicalUrl"])
        .or_else(|| frontmatter.get_scalar("source_url"))
        .or_else(|| {
            (input.input_kind == &ImportInputKind::Url)
                .then(|| input.normalized_locator.to_string())
        })
        .and_then(normalize_public_url);
    let platform_content_id = metadata_string(&["platformId", "platformContentId"])
        .or_else(|| frontmatter.get_scalar("source_id"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let content_kind = metadata_string(&["contentKind", "contentType"])
        .or_else(|| frontmatter.get_scalar("content_type"));
    let source_kind = match input.input_kind {
        ImportInputKind::File => "local_document".to_string(),
        ImportInputKind::Folder => "local_collection".to_string(),
        ImportInputKind::ClipboardText => "local_text".to_string(),
        ImportInputKind::Url => match content_kind.as_deref().map(str::to_ascii_lowercase) {
            Some(kind) if kind.contains("video") || kind.contains("audio") => "web_media".into(),
            Some(kind) if kind.contains("image") || kind.contains("note") => {
                "web_image_text".into()
            }
            _ => "web_page".into(),
        },
    };
    let title = input.display_name.trim();
    let title = if title.is_empty() {
        frontmatter
            .get_scalar("title")
            .unwrap_or_else(|| "Source".into())
    } else {
        title.to_string()
    };
    Ok(CandidateMetadata {
        source_kind,
        title: title.chars().take(512).collect(),
        canonical_url,
        platform,
        platform_content_id,
        author: metadata_string(&["author", "creator", "uploader"])
            .or_else(|| frontmatter.get_scalar("author"))
            .map(|value| value.chars().take(512).collect()),
        published_at: metadata_string(&["publishedAt"])
            .or_else(|| frontmatter.get_scalar("published_at")),
        language: metadata_string(&["transcriptLanguage", "language"])
            .map(|value| value.chars().take(64).collect()),
    })
}

pub fn finalize_source(input: FinalizationInput<'_>) -> Result<FinalizedSource, BackendError> {
    let candidate_markdown =
        std::str::from_utf8(input.candidate_markdown).map_err(|_| finalization_error())?;
    let body = split_frontmatter(candidate_markdown)
        .body
        .trim_start_matches(['\r', '\n'])
        .to_string();
    if body.trim().is_empty() {
        return Err(finalization_error());
    }
    // Source Markdown deliberately keeps portable `assets/...` references.
    // The renderer resolves those references through the immutable manifest;
    // exposing `raw/assets/...` here would bypass that contract and break
    // Source portability.
    let mut body = body;
    if !body.ends_with('\n') {
        body.push('\n');
    }
    let frontmatter = SourceFrontmatter {
        page_type: SourcePageType::Source,
        source_id: input.source_id.into(),
        version_id: input.version_id.into(),
        source_kind: input.candidate.source_kind.clone(),
        title: input.candidate.title.clone(),
        imported_at: input.imported_at.into(),
        content_hash: input.content_hash.into(),
        platform: input.candidate.platform.clone(),
        canonical_url: input.candidate.canonical_url.clone(),
        platform_content_id: input.candidate.platform_content_id.clone(),
        author: input.candidate.author.clone(),
        published_at: input.candidate.published_at.clone(),
        language: input.candidate.language.clone(),
        quality: input.quality.clone(),
        restricted: input.restricted,
    };
    validate_source_frontmatter(&frontmatter, input.content_hash)
        .map_err(|_| finalization_error())?;
    let rendered = render_frontmatter(&frontmatter, &body)?;
    let (round_trip, round_trip_body) = parse_final_source(&rendered)?;
    if round_trip != frontmatter || round_trip_body != body {
        return Err(finalization_error());
    }
    Ok(FinalizedSource {
        human_edit_hash: format!("{:x}", Sha256::digest(rendered.as_bytes())),
        bytes: rendered.into_bytes(),
        frontmatter,
    })
}

pub fn validate_final_source_binding(
    markdown: &[u8],
    manifest: &SourceManifest,
    version: &SourceVersion,
) -> Result<(), BackendError> {
    let markdown = std::str::from_utf8(markdown).map_err(|_| finalization_error())?;
    let (frontmatter, body) = parse_final_source(markdown)?;
    validate_source_frontmatter(&frontmatter, &version.content_hash)
        .map_err(|_| finalization_error())?;
    if body.trim().is_empty()
        || frontmatter.source_id != manifest.source_id
        || frontmatter.version_id != version.version_id
        || frontmatter.source_kind != manifest.source_kind
        || frontmatter.title != manifest.title
        || frontmatter.imported_at != version.created_at
        || frontmatter.canonical_url != manifest.canonical_url
        || frontmatter.platform != manifest.platform
        || frontmatter.platform_content_id != manifest.platform_content_id
        || frontmatter.author != manifest.author
        || frontmatter.published_at != manifest.published_at
        || frontmatter.language != manifest.language
        || frontmatter.quality != version.quality
        || frontmatter.restricted != manifest.restricted_content
    {
        return Err(finalization_error());
    }
    Ok(())
}

/// Validate the immutable identity carried by one concrete Source version.
///
/// Unlike `validate_final_source_binding`, this deliberately compares
/// version-scoped metadata with `SourceVersion::candidate`. That keeps a
/// historical baseline valid after a later version changes the manifest's
/// current title or other presentation metadata.
pub fn validate_source_version_binding(
    markdown: &[u8],
    manifest: &SourceManifest,
    version: &SourceVersion,
) -> Result<(), BackendError> {
    let markdown = std::str::from_utf8(markdown).map_err(|_| finalization_error())?;
    let (frontmatter, body) = parse_final_source(markdown)?;
    validate_source_frontmatter(&frontmatter, &version.content_hash)
        .map_err(|_| finalization_error())?;
    if body.trim().is_empty()
        || frontmatter.source_id != manifest.source_id
        || frontmatter.version_id != version.version_id
        || frontmatter.source_kind != version.candidate.source_kind
        || frontmatter.title != version.candidate.title
        || frontmatter.imported_at != version.created_at
        || frontmatter.canonical_url != version.candidate.canonical_url
        || frontmatter.platform != version.candidate.platform
        || frontmatter.platform_content_id != version.candidate.platform_content_id
        || frontmatter.author != version.candidate.author
        || frontmatter.published_at != version.candidate.published_at
        || frontmatter.language != version.candidate.language
        || frontmatter.quality != version.quality
        || frontmatter.restricted != manifest.restricted_content
    {
        return Err(finalization_error());
    }
    Ok(())
}

pub fn candidate_record(
    candidate: &CandidateMetadata,
    candidate_markdown_hash: String,
) -> SourceCandidateRecord {
    SourceCandidateRecord {
        markdown_hash: candidate_markdown_hash,
        title: candidate.title.clone(),
        source_kind: candidate.source_kind.clone(),
        canonical_url: candidate.canonical_url.clone(),
        platform: candidate.platform.clone(),
        platform_content_id: candidate.platform_content_id.clone(),
        author: candidate.author.clone(),
        published_at: candidate.published_at.clone(),
        language: candidate.language.clone(),
    }
}

pub fn parse_final_source(markdown: &str) -> Result<(SourceFrontmatter, String), BackendError> {
    let split = split_frontmatter(markdown);
    let raw = split.frontmatter.ok_or_else(finalization_error)?;
    let mut object = serde_json::Map::new();
    for line in raw.lines().filter(|line| !line.trim().is_empty()) {
        let (key, raw_value) = line.split_once(':').ok_or_else(finalization_error)?;
        let key = key.trim();
        let raw_value = raw_value.trim();
        let value = match key {
            "type" => serde_json::Value::String(raw_value.to_string()),
            "restricted" => serde_json::from_str(raw_value).map_err(|_| finalization_error())?,
            "quality" => serde_json::from_str(raw_value).map_err(|_| finalization_error())?,
            _ => serde_json::from_str::<String>(raw_value)
                .map(serde_json::Value::String)
                .map_err(|_| finalization_error())?,
        };
        if object.insert(key.to_string(), value).is_some() {
            return Err(finalization_error());
        }
    }
    let frontmatter = serde_json::from_value(serde_json::Value::Object(object))
        .map_err(|_| finalization_error())?;
    Ok((frontmatter, split.body))
}

fn render_frontmatter(frontmatter: &SourceFrontmatter, body: &str) -> Result<String, BackendError> {
    let mut lines = vec![
        "---".to_string(),
        "type: source".to_string(),
        yaml_string("sourceId", &frontmatter.source_id)?,
        yaml_string("versionId", &frontmatter.version_id)?,
        yaml_string("sourceKind", &frontmatter.source_kind)?,
        yaml_string("title", &frontmatter.title)?,
    ];
    for (key, value) in [
        ("platform", frontmatter.platform.as_deref()),
        ("canonicalUrl", frontmatter.canonical_url.as_deref()),
        (
            "platformContentId",
            frontmatter.platform_content_id.as_deref(),
        ),
        ("author", frontmatter.author.as_deref()),
        ("publishedAt", frontmatter.published_at.as_deref()),
    ] {
        if let Some(value) = value {
            lines.push(yaml_string(key, value)?);
        }
    }
    lines.push(yaml_string("importedAt", &frontmatter.imported_at)?);
    if let Some(language) = frontmatter.language.as_deref() {
        lines.push(yaml_string("language", language)?);
    }
    lines.push(yaml_string("contentHash", &frontmatter.content_hash)?);
    lines.push(format!(
        "quality: {}",
        serde_json::to_string(&frontmatter.quality).map_err(|_| finalization_error())?
    ));
    lines.push(format!("restricted: {}", frontmatter.restricted));
    lines.push("---".into());
    lines.push(String::new());
    lines.push(body.trim_start_matches(['\r', '\n']).to_string());
    let mut rendered = lines.join("\n");
    if !rendered.ends_with('\n') {
        rendered.push('\n');
    }
    Ok(rendered)
}

/// Render a Source body with the frozen final-Source frontmatter contract.
///
/// Source lifecycle operations use this instead of editing YAML strings so a
/// restored or reprocessed version receives the same validated contract as an
/// Import V2 commit.
pub fn render_source_markdown(
    frontmatter: &SourceFrontmatter,
    body: &str,
) -> Result<String, BackendError> {
    validate_source_frontmatter(frontmatter, &frontmatter.content_hash)
        .map_err(|_| finalization_error())?;
    if body.trim().is_empty() {
        return Err(finalization_error());
    }
    render_frontmatter(frontmatter, body)
}

fn yaml_string(key: &str, value: &str) -> Result<String, BackendError> {
    Ok(format!(
        "{key}: {}",
        serde_json::to_string(value).map_err(|_| finalization_error())?
    ))
}

fn find_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    match value {
        serde_json::Value::Object(object) => {
            for key in keys {
                if let Some(value) = object.get(*key).and_then(serde_json::Value::as_str) {
                    let value = value.trim();
                    if !value.is_empty() {
                        return Some(value.to_string());
                    }
                }
            }
            object.values().find_map(|value| find_string(value, keys))
        }
        serde_json::Value::Array(values) => {
            values.iter().find_map(|value| find_string(value, keys))
        }
        _ => None,
    }
}

fn normalize_public_url(value: String) -> Option<String> {
    let mut url = url::Url::parse(value.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https") || !url.has_host() {
        return None;
    }
    url.set_fragment(None);
    Some(
        crate::services::import_v2::redaction::redact_sensitive_text(url.as_str())
            .trim()
            .to_string(),
    )
}

fn finalization_error() -> BackendError {
    BackendError::new(
        IMPORT_V2_COMMIT_FAILED,
        "The Source candidate could not be finalized safely.",
        true,
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::import_v2::{QualityLevel, QualityReport};

    #[test]
    fn final_source_round_trip_rejects_engine_owned_stable_ids() {
        let error = inspect_candidate(CandidateInspection {
            input_kind: &ImportInputKind::Url,
            display_name: "Example",
            normalized_locator: "https://example.com/post",
            markdown: b"---\nsourceId: \"forged\"\n---\n\n# Body\n",
            metadata_documents: &[],
        })
        .unwrap_err();
        assert_eq!(error.code, IMPORT_V2_COMMIT_FAILED);
    }

    #[test]
    fn final_source_round_trip_preserves_detected_language_and_original_text() {
        let quality = QualityReport {
            level: QualityLevel::Pass,
            metrics: Vec::new(),
            warnings: Vec::new(),
            sheet_count_exact: None,
            slide_count_exact: None,
            non_empty_cell_coverage: None,
            formula_value_pairs: None,
            meaningful_image_coverage: None,
        };
        let candidate = CandidateMetadata {
            source_kind: "web_page".into(),
            title: "研发访谈".into(),
            canonical_url: Some("https://example.com/post".into()),
            platform: Some("example".into()),
            platform_content_id: Some("post".into()),
            author: Some("Aletta".into()),
            published_at: None,
            language: Some("zh-CN".into()),
        };
        let final_source = finalize_source(FinalizationInput {
            candidate_markdown: "---\nengine_id: old\n---\n\n# 原始标题\n\n未经翻译的中文正文。\n"
                .as_bytes(),
            candidate: &candidate,
            source_id: "src_a",
            version_id: "ver_a",
            content_hash: &"a".repeat(64),
            imported_at: "2026-07-25T00:00:00Z",
            quality: &quality,
            restricted: false,
        })
        .unwrap();
        let rendered = String::from_utf8(final_source.bytes).unwrap();
        let (frontmatter, body) = parse_final_source(&rendered).unwrap();
        assert_eq!(frontmatter.source_id, "src_a");
        assert_eq!(frontmatter.platform_content_id.as_deref(), Some("post"));
        assert_eq!(frontmatter.language.as_deref(), Some("zh-CN"));
        assert_eq!(body, "# 原始标题\n\n未经翻译的中文正文。\n");
        for forbidden in ["engine_id", "cookie", "token", "staging", "sessionId"] {
            assert!(!rendered.contains(forbidden));
        }
    }

    #[test]
    fn final_source_normalizes_leading_blank_lines_from_html_extractors() {
        let quality = QualityReport {
            level: QualityLevel::Pass,
            metrics: Vec::new(),
            warnings: Vec::new(),
            sheet_count_exact: None,
            slide_count_exact: None,
            non_empty_cell_coverage: None,
            formula_value_pairs: None,
            meaningful_image_coverage: None,
        };
        let candidate = CandidateMetadata {
            source_kind: "web_page".into(),
            title: "Web fixture".into(),
            canonical_url: Some("https://example.com/post".into()),
            platform: None,
            platform_content_id: None,
            author: None,
            published_at: None,
            language: None,
        };

        let finalized = finalize_source(FinalizationInput {
            candidate_markdown: b"\n\n# Heading\n\nBody\n",
            candidate: &candidate,
            source_id: "src_web",
            version_id: "ver_web",
            content_hash: &"b".repeat(64),
            imported_at: "2026-07-26T00:00:00Z",
            quality: &quality,
            restricted: false,
        })
        .unwrap();
        let rendered = String::from_utf8(finalized.bytes).unwrap();
        let (_, body) = parse_final_source(&rendered).unwrap();

        assert_eq!(body, "# Heading\n\nBody\n");
    }
}
