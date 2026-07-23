use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::errors::{BackendError, IMPORT_V2_CAPABILITY_INVALID};

pub const MEDIA_CHUNK_BYTES: usize = 1024 * 1024;
pub const MEDIA_MAX_IN_FLIGHT_CHUNKS: usize = 2;
const ITEM_STAGING_TEMP_PREFIXES: [&str; 6] = [
    ".asr-input-",
    ".ocr-input-",
    ".media-fetch-",
    ".web-fetch-",
    ".capability-runtime-",
    ".sensevoice-output-",
];

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
    WaitingAuthorization,
}

#[derive(Default)]
pub struct MediaRouter;

impl MediaRouter {
    pub fn plan(&self, input: &MediaInput, asr_available: bool) -> MediaRoutePlan {
        self.plan_authorized(input, asr_available, true)
    }

    pub fn plan_authorized(
        &self,
        input: &MediaInput,
        asr_available: bool,
        user_authorized: bool,
    ) -> MediaRoutePlan {
        let subtitle = input
            .subtitles
            .iter()
            .min_by_key(|candidate| subtitle_rank(candidate.kind))
            .cloned();
        let status = if subtitle.is_some() {
            MediaRouteStatus::Ready
        } else if !asr_available {
            MediaRouteStatus::WaitingCapability
        } else if !user_authorized {
            MediaRouteStatus::WaitingAuthorization
        } else {
            MediaRouteStatus::Ready
        };
        MediaRoutePlan {
            requires_asr: subtitle.is_none() && asr_available && user_authorized,
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
    cleanup_on_drop: bool,
}

impl TemporaryMediaWorkspace {
    pub fn create(path: &Path) -> Result<Self, BackendError> {
        fs::create_dir_all(path)
            .map_err(|_| media_error("Could not create the temporary media workspace."))?;
        Ok(Self {
            path: path.to_path_buf(),
            cleanup_on_drop: true,
        })
    }
    pub fn create_unique(parent: &Path, prefix: &str) -> Result<Self, BackendError> {
        for _ in 0..8 {
            let path = parent.join(format!("{prefix}-{}", uuid::Uuid::new_v4()));
            match fs::create_dir(&path) {
                Ok(()) => {
                    return Ok(Self {
                        path,
                        cleanup_on_drop: true,
                    })
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => break,
            }
        }
        Err(media_error(
            "Could not create a unique temporary media workspace.",
        ))
    }
    pub fn adopt_existing(path: &Path) -> Result<Self, BackendError> {
        let metadata = fs::symlink_metadata(path)
            .map_err(|_| media_error("The temporary media workspace is unavailable."))?;
        if metadata.file_type().is_symlink() || is_reparse(&metadata) || !metadata.is_dir() {
            return Err(media_error("The temporary media workspace is invalid."));
        }
        let canonical = path
            .canonicalize()
            .map_err(|_| media_error("The temporary media workspace is unavailable."))?;
        Ok(Self {
            path: canonical,
            cleanup_on_drop: true,
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn retain(mut self) -> PathBuf {
        self.cleanup_on_drop = false;
        self.path.clone()
    }
}

impl Drop for TemporaryMediaWorkspace {
    fn drop(&mut self) {
        if self.cleanup_on_drop {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

pub fn recover_media_temp_root(root: &Path) -> Result<(), BackendError> {
    if !root.exists() {
        return Ok(());
    }
    let root_metadata = fs::symlink_metadata(root)
        .map_err(|_| media_error("Could not inspect the temporary media root."))?;
    if root_metadata.file_type().is_symlink()
        || is_reparse(&root_metadata)
        || !root_metadata.is_dir()
    {
        return Err(media_error("The temporary media root is invalid."));
    }
    for entry in fs::read_dir(root)
        .map_err(|_| media_error("Could not inspect temporary media workspaces."))?
    {
        let path = entry
            .map_err(|_| media_error("Could not inspect a temporary media workspace."))?
            .path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| media_error("Could not inspect a temporary media workspace."))?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() && !is_reparse(&metadata) {
            fs::remove_dir_all(path).map_err(|_| {
                media_error("Could not remove an orphaned temporary media workspace.")
            })?;
        }
    }
    Ok(())
}

pub fn recover_item_staging_temporary_workspaces(staging: &Path) -> Result<(), BackendError> {
    if !staging.exists() {
        return Ok(());
    }
    let staging_metadata = fs::symlink_metadata(staging)
        .map_err(|_| media_error("Could not inspect the item staging directory."))?;
    if staging_metadata.file_type().is_symlink()
        || is_reparse(&staging_metadata)
        || !staging_metadata.is_dir()
    {
        return Err(media_error("The item staging directory is invalid."));
    }
    for entry in fs::read_dir(staging)
        .map_err(|_| media_error("Could not inspect item staging workspaces."))?
    {
        let entry =
            entry.map_err(|_| media_error("Could not inspect an item staging workspace."))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if name == "runtime-temp" {
            recover_media_temp_root(&path)?;
            continue;
        }
        if !ITEM_STAGING_TEMP_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| media_error("Could not inspect an item staging workspace."))?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() && !is_reparse(&metadata) {
            fs::remove_dir_all(&path)
                .map_err(|_| media_error("Could not remove an orphaned item workspace."))?;
        }
    }
    Ok(())
}

#[cfg(windows)]
fn is_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse(_: &fs::Metadata) -> bool {
    false
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

pub fn move_staged_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(_) => {
            fs::copy(source, destination)?;
            fs::remove_file(source)?;
            Ok(())
        }
    }
}

pub fn link_or_copy(source: &Path, destination: &Path) -> std::io::Result<()> {
    if fs::hard_link(source, destination).is_ok() {
        return Ok(());
    }
    fs::copy(source, destination).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_temporary_workspace_is_created_exclusively_and_removed_on_drop() {
        let parent = std::env::temp_dir().join(format!("media-temp-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&parent).unwrap();
        let created = {
            let workspace = TemporaryMediaWorkspace::create_unique(&parent, ".runtime").unwrap();
            assert!(workspace.path().starts_with(&parent));
            assert!(workspace.path().is_dir());
            workspace.path().to_path_buf()
        };
        assert!(!created.exists());
        fs::remove_dir(parent).unwrap();
    }

    #[test]
    fn existing_temporary_workspace_is_adopted_without_recreating_it() {
        let parent = std::env::temp_dir().join(format!("media-temp-{}", uuid::Uuid::new_v4()));
        let workspace = parent.join(".asr-input-existing");
        fs::create_dir_all(&workspace).unwrap();
        {
            let adopted = TemporaryMediaWorkspace::adopt_existing(&workspace).unwrap();
            assert_eq!(adopted.path(), workspace.canonicalize().unwrap());
        }
        assert!(!workspace.exists());
        fs::remove_dir(parent).unwrap();
    }

    #[test]
    fn recovery_removes_only_known_item_temporary_workspaces() {
        let staging = std::env::temp_dir().join(format!("media-recovery-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(staging.join(".asr-input-orphan")).unwrap();
        fs::create_dir_all(staging.join(".ocr-input-orphan")).unwrap();
        fs::create_dir_all(staging.join(".media-fetch-orphan")).unwrap();
        fs::create_dir_all(staging.join(".web-fetch-orphan")).unwrap();
        fs::create_dir_all(staging.join(".capability-runtime-orphan")).unwrap();
        fs::create_dir_all(staging.join("runtime-temp/legacy-orphan")).unwrap();
        fs::create_dir_all(staging.join("assets")).unwrap();
        fs::write(staging.join("assets/cover.jpg"), b"durable").unwrap();
        fs::write(staging.join("document.md"), b"durable").unwrap();

        recover_item_staging_temporary_workspaces(&staging).unwrap();

        for temporary in [
            ".asr-input-orphan",
            ".ocr-input-orphan",
            ".media-fetch-orphan",
            ".web-fetch-orphan",
            ".capability-runtime-orphan",
            "runtime-temp/legacy-orphan",
        ] {
            assert!(
                !staging.join(temporary).exists(),
                "{temporary} was retained"
            );
        }
        assert_eq!(
            fs::read(staging.join("assets/cover.jpg")).unwrap(),
            b"durable"
        );
        assert_eq!(fs::read(staging.join("document.md")).unwrap(), b"durable");
        fs::remove_dir_all(staging).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn recovery_does_not_follow_matching_symlink_workspace() {
        use std::os::unix::fs::symlink;

        let staging = std::env::temp_dir().join(format!("media-recovery-{}", uuid::Uuid::new_v4()));
        let outside =
            std::env::temp_dir().join(format!("media-recovery-outside-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&staging).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("keep.txt"), b"keep").unwrap();
        symlink(&outside, staging.join(".asr-input-linked")).unwrap();

        recover_item_staging_temporary_workspaces(&staging).unwrap();

        assert_eq!(fs::read(outside.join("keep.txt")).unwrap(), b"keep");
        assert!(fs::symlink_metadata(staging.join(".asr-input-linked")).is_ok());
        fs::remove_file(staging.join(".asr-input-linked")).unwrap();
        fs::remove_dir_all(staging).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
}
