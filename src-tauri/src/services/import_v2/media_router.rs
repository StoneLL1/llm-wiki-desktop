use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::errors::{BackendError, IMPORT_V2_CAPABILITY_INVALID};

pub const MEDIA_CHUNK_BYTES: usize = 1024 * 1024;
pub const MEDIA_MAX_IN_FLIGHT_CHUNKS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Audio,
    Video,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SubtitleKind {
    HumanPlatform,
    HumanLocal,
    Automatic,
    Embedded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtitleCandidate {
    pub kind: SubtitleKind,
    pub path: String,
}

impl SubtitleCandidate {
    pub fn new(kind: SubtitleKind, path: impl Into<String>) -> Self {
        Self {
            kind,
            path: path.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaInput {
    pub kind: MediaKind,
    pub subtitles: Vec<SubtitleCandidate>,
    pub cover_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaArtifactPlan;

impl MediaArtifactPlan {
    pub fn subtitle_markdown_cover_metadata() -> Vec<String> {
        vec![
            "subtitle.vtt".into(),
            "document.md".into(),
            "cover".into(),
            "metadata.json".into(),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaRoutePlan {
    pub subtitle: Option<SubtitleCandidate>,
    pub requires_asr: bool,
    pub artifacts: Vec<String>,
    pub chunk_bytes: usize,
    pub max_in_flight_chunks: usize,
    pub status: MediaRouteStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaRouteStatus {
    Ready,
    WaitingCapability,
}

#[derive(Default)]
pub struct MediaRouter;

impl MediaRouter {
    pub fn plan(&self, input: &MediaInput, whisper_available: bool) -> MediaRoutePlan {
        let subtitle = input
            .subtitles
            .iter()
            .min_by_key(|candidate| subtitle_rank(candidate.kind))
            .cloned();
        let status = if subtitle.is_none() && !whisper_available {
            MediaRouteStatus::WaitingCapability
        } else {
            MediaRouteStatus::Ready
        };
        MediaRoutePlan {
            requires_asr: subtitle.is_none() && whisper_available,
            subtitle,
            artifacts: MediaArtifactPlan::subtitle_markdown_cover_metadata(),
            chunk_bytes: MEDIA_CHUNK_BYTES,
            max_in_flight_chunks: MEDIA_MAX_IN_FLIGHT_CHUNKS,
            status,
        }
    }
}

fn subtitle_rank(kind: SubtitleKind) -> u8 {
    match kind {
        SubtitleKind::HumanPlatform | SubtitleKind::HumanLocal => 0,
        SubtitleKind::Automatic => 1,
        SubtitleKind::Embedded => 2,
    }
}

pub struct TemporaryMediaWorkspace {
    path: PathBuf,
}

impl TemporaryMediaWorkspace {
    pub fn create(path: &Path) -> Result<Self, BackendError> {
        fs::create_dir_all(path)
            .map_err(|_| media_error("Could not create the temporary media workspace."))?;
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryMediaWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub fn recover_media_temp_root(root: &Path) -> Result<(), BackendError> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root)
        .map_err(|_| media_error("Could not inspect temporary media workspaces."))?
    {
        let path = entry
            .map_err(|_| media_error("Could not inspect a temporary media workspace."))?
            .path();
        if path.is_dir() {
            fs::remove_dir_all(path).map_err(|_| {
                media_error("Could not remove an orphaned temporary media workspace.")
            })?;
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AsrModelCatalog {
    pub models: Vec<AsrModel>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AsrModel {
    pub id: String,
    pub sha256: String,
    pub download: ModelDownloadContract,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDownloadContract {
    pub resumable: bool,
    pub signature_required: bool,
    pub runtime_install_allowed: bool,
}

impl AsrModelCatalog {
    pub fn from_manifest_str(value: &str) -> Result<Self, BackendError> {
        let catalog: Self = serde_json::from_str(value)
            .map_err(|_| media_error("The ASR model catalog is invalid."))?;
        if catalog.models.iter().any(|model| {
            model.sha256.len() != 64 || !model.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        }) {
            return Err(media_error(
                "Every ASR model must have a pinned SHA-256 digest.",
            ));
        }
        Ok(catalog)
    }
    pub fn select(&self, id: &str) -> Result<&AsrModel, BackendError> {
        self.models
            .iter()
            .find(|model| model.id == id)
            .ok_or_else(|| media_error("The requested ASR model is not declared."))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

pub fn render_timestamped_markdown(
    segments: &[TranscriptSegment],
    engine: &str,
    model: &str,
    language: &str,
    confidence: f64,
) -> String {
    let mut output = format!("---\nengine: {engine}\nmodel: {model}\nlanguage: {language}\nlanguageConfidence: {confidence:.3}\n---\n\n");
    for segment in segments {
        output.push_str(&format!(
            "- [{} --> {}] {}\n",
            timestamp(segment.start_ms),
            timestamp(segment.end_ms),
            segment.text.trim()
        ));
    }
    output
}

fn timestamp(milliseconds: u64) -> String {
    let hours = milliseconds / 3_600_000;
    let minutes = milliseconds / 60_000 % 60;
    let seconds = milliseconds / 1_000 % 60;
    let millis = milliseconds % 1_000;
    format!("{hours:02}:{minutes:02}:{seconds:02}.{millis:03}")
}

fn media_error(message: &str) -> BackendError {
    BackendError::new(IMPORT_V2_CAPABILITY_INVALID, message, false, false)
}
