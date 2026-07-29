use std::path::{Path, PathBuf};

use serde::Serialize;
use unicode_normalization::UnicodeNormalization;

use crate::errors::{BackendError, IMPORT_V2_CANCELLED, IMPORT_V2_ENGINE_OUTPUT_INVALID};
use crate::models::import_v2::{ImportInput, ImportInputKind};
use crate::models::import_v2_file::FileFormat;
use crate::services::import_v2::engine::{
    EngineContinuation, EngineDescriptor, EngineRequest, EngineResult, ImportEngine,
};
use crate::services::import_v2::markdown_normalizer::{decode_text, normalize_markdown};
use crate::services::import_v2::media_router::{
    MediaInput, MediaKind, MediaRouter, SubtitleCandidate, SubtitleKind,
};
use crate::services::import_v2::native_file_engine::{
    resolve_inside, resolve_source, safe_read_source,
};
use crate::services::import_v2::subtitle::render_subtitle_markdown;
use crate::tasks::task_model::CancellationToken;

#[derive(Default)]
pub struct NativeSubtitleEngine;

impl ImportEngine for NativeSubtitleEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            engine_id: "builtin.local-subtitle".into(),
            engine_version: "1".into(),
            route: "media.subtitle".into(),
        }
    }

    fn supports(&self, input: &ImportInput) -> bool {
        input.kind == ImportInputKind::File
    }

    fn execute(
        &self,
        request: &EngineRequest,
        cancellation: &CancellationToken,
    ) -> Result<EngineResult, BackendError> {
        let prepared = prepare_input(request, cancellation)?;
        let format = detect_format(&prepared.source, &prepared.source_bytes)?;
        let extension = subtitle_extension(format).ok_or_else(|| {
            invalid("The selected file does not contain a supported subtitle format.")
        })?;
        let transcript = render_subtitle_markdown(&prepared.source_bytes, extension)
            .ok_or_else(subtitle_unavailable)?;
        let transcript_evidence = prepared.source_bytes.clone();
        stage_transcript_candidate(
            request,
            cancellation,
            prepared,
            &transcript,
            extension,
            "standalone",
            &transcript_evidence,
        )
    }
}

#[derive(Default)]
pub struct NativeMediaCompanionEngine;

impl ImportEngine for NativeMediaCompanionEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            engine_id: "builtin.local-media-companion".into(),
            engine_version: "1".into(),
            route: "media.companion".into(),
        }
    }

    fn supports(&self, input: &ImportInput) -> bool {
        input.kind == ImportInputKind::File
    }

    fn execute(
        &self,
        request: &EngineRequest,
        cancellation: &CancellationToken,
    ) -> Result<EngineResult, BackendError> {
        let prepared = prepare_input(request, cancellation)?;
        let media_format = detect_format(&prepared.source, &prepared.source_bytes)?;
        let kind = match media_format.content_kind() {
            crate::models::import_v2_file::FileContentKind::Audio => MediaKind::Audio,
            crate::models::import_v2_file::FileContentKind::Video => MediaKind::Video,
            _ => {
                return Err(invalid(
                    "The selected file does not contain supported audio or video.",
                ))
            }
        };
        let mut candidates = companion_candidates(&prepared.source)?;
        if let Some(selected_name) = request.selected_subtitle.as_deref() {
            candidates.retain(|candidate| {
                candidate.path.file_name().and_then(|name| name.to_str()) == Some(selected_name)
            });
        }
        if candidates.len() > 1 {
            return Err(subtitle_ambiguous(&candidates));
        }
        if request.selected_subtitle.is_some() && candidates.is_empty() {
            let current = companion_candidates(&prepared.source)?;
            if !current.is_empty() {
                return Err(subtitle_ambiguous(&current));
            }
        }
        let route_candidates = candidates
            .iter()
            .map(|candidate| {
                SubtitleCandidate::new(
                    SubtitleKind::HumanLocal,
                    candidate.path.to_string_lossy().into_owned(),
                )
            })
            .collect();
        let plan = MediaRouter.plan(
            &MediaInput {
                kind,
                subtitles: route_candidates,
                cover_path: None,
            },
            false,
        );
        let selected = plan.subtitle.and_then(|selected| {
            candidates
                .into_iter()
                .find(|candidate| candidate.path.to_string_lossy() == selected.path)
        });
        // Embedded subtitles outrank companion files. The signed media
        // capability performs an extraction-only embedded-track probe before
        // ASR authorization; the companion transcript is staged as a fallback
        // and is selected only when that probe reports no embedded track.
        stage_local_asr_candidate(request, cancellation, prepared, kind, selected.as_ref())
    }
}

struct PreparedInput {
    source: PathBuf,
    staging: PathBuf,
    source_bytes: Vec<u8>,
}

struct CompanionCandidate {
    path: PathBuf,
    extension: String,
    markdown: String,
    bytes: Vec<u8>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalTranscriptMetadata<'a> {
    schema_version: u32,
    source_name: &'a str,
    relative_path: &'a str,
    transcript_origin: &'a str,
    transcript_format: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalMediaMetadata<'a> {
    schema_version: u32,
    source_name: &'a str,
    relative_path: &'a str,
}

fn prepare_input(
    request: &EngineRequest,
    cancellation: &CancellationToken,
) -> Result<PreparedInput, BackendError> {
    if cancellation.is_cancelled() {
        return Err(cancelled());
    }
    let root = PathBuf::from(&request.project_root);
    let source = resolve_source(&root, &request.input.locator)?;
    let staging = resolve_inside(&root, &request.staging_root)?;
    let identity = request
        .input
        .source_identity
        .as_ref()
        .ok_or_else(|| invalid("The selected source must be scanned again."))?;
    let source_bytes = safe_read_source(&source, identity)?;
    Ok(PreparedInput {
        source,
        staging,
        source_bytes,
    })
}

fn detect_format(path: &Path, bytes: &[u8]) -> Result<FileFormat, BackendError> {
    crate::services::import_v2::file_discovery::identify_file(path, &bytes[..bytes.len().min(8192)])
        .map(|(format, _)| format)
}

fn subtitle_extension(format: FileFormat) -> Option<&'static str> {
    match format {
        FileFormat::Srt => Some("srt"),
        FileFormat::Vtt => Some("vtt"),
        FileFormat::Ass => Some("ass"),
        FileFormat::Lrc => Some("lrc"),
        _ => None,
    }
}

fn companion_candidates(media: &Path) -> Result<Vec<CompanionCandidate>, BackendError> {
    let parent = media
        .parent()
        .ok_or_else(|| invalid("The media directory is invalid."))?;
    let target_stem = portable_stem(media);
    let mut paths = std::fs::read_dir(parent)
        .map_err(|_| invalid("The media directory could not be inspected."))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path != media && is_companion_stem(&portable_stem(path), &target_stem))
        .collect::<Vec<_>>();
    paths.sort_by_key(|path| path.to_string_lossy().to_lowercase());

    let mut candidates = Vec::new();
    for path in paths {
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata)
                if metadata.is_file()
                    && !metadata.file_type().is_symlink()
                    && metadata.len() <= 4 * 1024 * 1024 =>
            {
                metadata
            }
            _ => continue,
        };
        let _ = metadata;
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let markdown = match extension.as_str() {
            "srt" | "vtt" | "ass" | "ssa" | "lrc" => render_subtitle_markdown(&bytes, &extension),
            "md" | "markdown" => decode_text(&bytes)
                .ok()
                .map(|text| normalize_markdown(&text)),
            "txt" => decode_text(&bytes).ok().map(|text| text.trim().to_string()),
            _ => None,
        };
        if let Some(markdown) = markdown.filter(|value| !value.trim().is_empty()) {
            candidates.push(CompanionCandidate {
                path,
                extension,
                markdown,
                bytes,
            });
        }
    }
    Ok(candidates)
}

fn subtitle_ambiguous(candidates: &[CompanionCandidate]) -> BackendError {
    let choices = candidates
        .iter()
        .filter_map(|candidate| {
            candidate
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
        })
        .collect::<Vec<_>>();
    BackendError::new(
        "IMPORT_LOCAL_SUBTITLE_AMBIGUOUS",
        "Multiple reliable local subtitle or transcript files matched this media.",
        true,
        true,
    )
    .with_details(serde_json::json!({ "subtitleCandidates": choices }))
}

fn is_companion_stem(candidate_stem: &str, media_stem: &str) -> bool {
    if candidate_stem == media_stem {
        return true;
    }
    let Some(suffix) = candidate_stem.strip_prefix(media_stem).and_then(|suffix| {
        suffix
            .strip_prefix('.')
            .or_else(|| suffix.strip_prefix('_'))
            .or_else(|| suffix.strip_prefix('-'))
    }) else {
        return false;
    };
    let language_parts = suffix.split(['-', '_']).collect::<Vec<_>>();
    matches!(language_parts.as_slice(), [language] if is_language_part(language))
        || matches!(
            language_parts.as_slice(),
            [language, region]
                if is_language_part(language)
                    && (region.len() == 2 || region.len() == 3)
                    && region.chars().all(|value| value.is_ascii_alphanumeric())
        )
}

fn is_language_part(value: &str) -> bool {
    (2..=3).contains(&value.len())
        && value
            .chars()
            .all(|character| character.is_ascii_alphabetic())
}

fn stage_transcript_candidate(
    request: &EngineRequest,
    cancellation: &CancellationToken,
    prepared: PreparedInput,
    transcript: &str,
    extension: &str,
    origin: &str,
    transcript_evidence: &[u8],
) -> Result<EngineResult, BackendError> {
    if cancellation.is_cancelled() {
        return Err(cancelled());
    }
    let evidence_path = if matches!(extension, "srt" | "vtt" | "ass" | "ssa" | "lrc") {
        format!("subtitles/original.{extension}")
    } else {
        format!("source-evidence/companion.{extension}")
    };
    let evidence_parent = prepared
        .staging
        .join(&evidence_path)
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| invalid("The transcript evidence path is invalid."))?;
    std::fs::create_dir_all(evidence_parent)
        .map_err(|_| invalid("The transcript staging directory could not be created."))?;
    let title = Path::new(&request.input.display_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Local media")
        .to_string();
    let markdown = format!("# {title}\n\n## Transcript\n\n{}\n", transcript.trim());
    let metadata = LocalTranscriptMetadata {
        schema_version: 1,
        source_name: &request.input.display_name,
        relative_path: &request.input.display_name,
        transcript_origin: origin,
        transcript_format: extension,
    };
    std::fs::write(prepared.staging.join("source.bin"), &prepared.source_bytes)
        .and_then(|_| std::fs::write(prepared.staging.join("document.md"), markdown.as_bytes()))
        .and_then(|_| std::fs::write(prepared.staging.join(&evidence_path), transcript_evidence))
        .and_then(|_| {
            serde_json::to_vec_pretty(&metadata)
                .map_err(std::io::Error::other)
                .and_then(|bytes| std::fs::write(prepared.staging.join("metadata.json"), bytes))
        })
        .map_err(|_| invalid("The local transcript candidate could not be staged."))?;
    if cancellation.is_cancelled() {
        let _ = std::fs::remove_dir_all(&prepared.staging);
        return Err(cancelled());
    }
    Ok(EngineResult {
        source_snapshot_path: "source.bin".into(),
        markdown_path: "document.md".into(),
        asset_paths: vec![evidence_path],
        metadata_path: Some("metadata.json".into()),
        title,
        text_coverage: Some(1.0),
        table_cell_accuracy: None,
        sheet_count_exact: None,
        slide_count_exact: None,
        non_empty_cell_coverage: None,
        formula_value_pairs: None,
        meaningful_image_coverage: None,
        continuation: None,
        warnings: Vec::new(),
    })
}

fn stage_local_asr_candidate(
    request: &EngineRequest,
    cancellation: &CancellationToken,
    prepared: PreparedInput,
    kind: MediaKind,
    companion_fallback: Option<&CompanionCandidate>,
) -> Result<EngineResult, BackendError> {
    if cancellation.is_cancelled() {
        return Err(cancelled());
    }
    std::fs::create_dir_all(&prepared.staging)
        .map_err(|_| invalid("The local media staging directory could not be created."))?;
    let workspace =
        crate::services::import_v2::media_router::TemporaryMediaWorkspace::create_unique(
            &prepared.staging,
            ".asr-input",
        )?;
    let extension = prepared
        .source
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("bin");
    let temporary_file = workspace.path().join(format!("original.{extension}"));
    let mut asset_paths = Vec::new();
    let metadata = serde_json::to_vec_pretty(&LocalMediaMetadata {
        schema_version: 1,
        source_name: &request.input.display_name,
        relative_path: &request.input.display_name,
    })
    .map_err(|_| invalid("The local media metadata could not be staged."))?;
    std::fs::write(prepared.staging.join("source.bin"), &prepared.source_bytes)
        .and_then(|_| std::fs::write(&temporary_file, &prepared.source_bytes))
        .and_then(|_| {
            std::fs::write(
                prepared.staging.join("document.md"),
                format!(
                    "# {}\n",
                    Path::new(&request.input.display_name)
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or("Local media")
                ),
            )
        })
        .and_then(|_| std::fs::write(prepared.staging.join("metadata.json"), metadata))
        .map_err(|_| invalid("The local media ASR input could not be staged."))?;
    if let Some(companion) = companion_fallback {
        let evidence_path = if matches!(
            companion.extension.as_str(),
            "srt" | "vtt" | "ass" | "ssa" | "lrc"
        ) {
            format!("subtitles/companion.{}", companion.extension)
        } else {
            format!("source-evidence/companion.{}", companion.extension)
        };
        let evidence_parent = prepared
            .staging
            .join(&evidence_path)
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| invalid("The companion transcript evidence path is invalid."))?;
        std::fs::create_dir_all(evidence_parent)
            .and_then(|_| std::fs::create_dir_all(prepared.staging.join("transcripts")))
            .and_then(|_| std::fs::write(prepared.staging.join(&evidence_path), &companion.bytes))
            .and_then(|_| {
                std::fs::write(
                    prepared.staging.join("transcripts/companion-fallback.md"),
                    companion.markdown.as_bytes(),
                )
            })
            .map_err(|_| invalid("The companion transcript fallback could not be staged."))?;
        asset_paths.push(evidence_path);
    }
    if cancellation.is_cancelled() {
        let _ = std::fs::remove_dir_all(&prepared.staging);
        return Err(cancelled());
    }
    let workspace = workspace.retain();
    let temporary_input_path = temporary_file
        .strip_prefix(&prepared.staging)
        .map_err(|_| invalid("The local media ASR input path is invalid."))?
        .to_string_lossy()
        .replace('\\', "/");
    let title = Path::new(&request.input.display_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Local media")
        .to_string();
    debug_assert!(workspace.starts_with(&prepared.staging));
    Ok(EngineResult {
        source_snapshot_path: "source.bin".into(),
        markdown_path: "document.md".into(),
        asset_paths,
        metadata_path: Some("metadata.json".into()),
        title,
        text_coverage: None,
        table_cell_accuracy: None,
        sheet_count_exact: None,
        slide_count_exact: None,
        non_empty_cell_coverage: None,
        formula_value_pairs: None,
        meaningful_image_coverage: None,
        continuation: Some(EngineContinuation::LocalAsr {
            temporary_input_path,
            media_kind: match kind {
                MediaKind::Audio => "audio",
                MediaKind::Video => "video",
            }
            .into(),
        }),
        warnings: Vec::new(),
    })
}

fn portable_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .nfc()
        .collect::<String>()
        .to_lowercase()
}

fn subtitle_unavailable() -> BackendError {
    BackendError::new(
        "IMPORT_WEB_SUBTITLE_UNAVAILABLE",
        "No reliable local subtitle or transcript was found.",
        true,
        true,
    )
}

fn invalid(message: &str) -> BackendError {
    BackendError::new(IMPORT_V2_ENGINE_OUTPUT_INVALID, message, true, true)
}

fn cancelled() -> BackendError {
    BackendError::new(
        IMPORT_V2_CANCELLED,
        "Local media import was cancelled.",
        true,
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::is_companion_stem;

    #[test]
    fn companion_stem_accepts_exact_and_language_suffixes_only() {
        for candidate in ["video", "video.zh", "video.zh-cn", "video_en", "video-ja"] {
            assert!(is_companion_stem(candidate, "video"), "{candidate}");
        }
        for candidate in [
            "video-notes",
            "video.backup",
            "video.zh-cn-extra",
            "other.zh-cn",
        ] {
            assert!(!is_companion_stem(candidate, "video"), "{candidate}");
        }
    }
}
