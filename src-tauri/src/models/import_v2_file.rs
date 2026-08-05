use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FileFormat {
    Markdown,
    Text,
    Html,
    Csv,
    Doc,
    Docx,
    Xls,
    Xlsx,
    Ppt,
    Pptx,
    Pdf,
    Png,
    Jpeg,
    Webp,
    Bmp,
    Tiff,
    Heic,
    Heif,
    AnimatedGif,
    Mp3,
    Wav,
    M4a,
    Aac,
    Flac,
    Ogg,
    Opus,
    Wma,
    Mp4,
    Mov,
    Mkv,
    Webm,
    Avi,
    M4v,
    Wmv,
    Srt,
    Vtt,
    Ass,
    Lrc,
}

impl FileFormat {
    pub fn canonical_extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Text => "txt",
            Self::Html => "html",
            Self::Csv => "csv",
            Self::Doc => "doc",
            Self::Docx => "docx",
            Self::Xls => "xls",
            Self::Xlsx => "xlsx",
            Self::Ppt => "ppt",
            Self::Pptx => "pptx",
            Self::Pdf => "pdf",
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
            Self::Bmp => "bmp",
            Self::Tiff => "tiff",
            Self::Heic => "heic",
            Self::Heif => "heif",
            Self::AnimatedGif => "gif",
            Self::Mp3 => "mp3",
            Self::Wav => "wav",
            Self::M4a => "m4a",
            Self::Aac => "aac",
            Self::Flac => "flac",
            Self::Ogg => "ogg",
            Self::Opus => "opus",
            Self::Wma => "wma",
            Self::Mp4 => "mp4",
            Self::Mov => "mov",
            Self::Mkv => "mkv",
            Self::Webm => "webm",
            Self::Avi => "avi",
            Self::M4v => "m4v",
            Self::Wmv => "wmv",
            Self::Srt => "srt",
            Self::Vtt => "vtt",
            Self::Ass => "ass",
            Self::Lrc => "lrc",
        }
    }

    pub fn content_kind(self) -> FileContentKind {
        match self {
            Self::Png
            | Self::Jpeg
            | Self::Webp
            | Self::Bmp
            | Self::Tiff
            | Self::Heic
            | Self::Heif => FileContentKind::Image,
            Self::Mp3
            | Self::Wav
            | Self::M4a
            | Self::Aac
            | Self::Flac
            | Self::Ogg
            | Self::Opus
            | Self::Wma => FileContentKind::Audio,
            Self::AnimatedGif
            | Self::Mp4
            | Self::Mov
            | Self::Mkv
            | Self::Webm
            | Self::Avi
            | Self::M4v
            | Self::Wmv => FileContentKind::Video,
            Self::Srt | Self::Vtt | Self::Ass | Self::Lrc => FileContentKind::Subtitle,
            _ => FileContentKind::Document,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FileContentKind {
    Document,
    Image,
    Audio,
    Video,
    Subtitle,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FileDetectionMethod {
    Magic,
    Container,
    StructuredText,
    ExtensionFallback,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileIdentity {
    /// The user-visible extension, retained only as a hint.
    pub extension: String,
    /// Stable human-readable signature/container label, never raw prefix bytes.
    pub magic: String,
    pub mime: String,
    pub detection_method: FileDetectionMethod,
    pub extension_mismatch: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileSkipReason {
    SymlinkOrReparsePoint,
    HiddenOrSystem,
    IgnoredDirectory,
    ProjectInternal,
    UnsupportedFormat,
    CycleDetected,
    DepthLimitExceeded,
    FileLimitExceeded,
    FileTooLarge,
    LargeDataConfirmationRequired,
    Duplicate,
    InvalidPath,
    Unreadable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredFile {
    pub source_path: String,
    pub relative_path: String,
    pub display_name: String,
    pub format: FileFormat,
    pub content_kind: FileContentKind,
    pub size_bytes: u64,
    pub identity: FileIdentity,
    pub source_identity: crate::models::import_v2::SourceIdentity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub large_data: Option<LargeDataEstimate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LargeDataEstimate {
    pub row_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet_count: Option<u32>,
    pub estimated_output_files: u32,
    pub total_bytes: u64,
    pub requires_confirmation: bool,
    #[serde(default = "serde_default_true")]
    pub estimate_complete: bool,
}

fn serde_default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkippedFile {
    pub source_path: String,
    pub relative_path: Option<String>,
    pub reason: FileSkipReason,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileScanPolicy {
    pub max_depth: u32,
    pub max_files: u32,
    pub max_file_bytes: u64,
    pub include_hidden: bool,
}

impl Default for FileScanPolicy {
    fn default() -> Self {
        Self {
            max_depth: 32,
            max_files: 10_000,
            max_file_bytes: 64 * 1024 * 1024,
            include_hidden: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileScanResult {
    pub files: Vec<DiscoveredFile>,
    pub skipped: Vec<SkippedFile>,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan_identity: Option<ImportScanIdentity>,
    #[serde(default)]
    pub totals: ImportScanTotals,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmation_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aggregate_confirmed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discarded_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportScanIdentity {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub task_id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportScanConfirmationReason {
    FileCount,
    TotalBytes,
    EstimatedOutputFiles,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportScanTotals {
    pub file_count: u32,
    pub total_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_output_files: Option<u64>,
    pub requires_confirmation: bool,
    pub reasons: Vec<ImportScanConfirmationReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityRequirement {
    pub capability_id: String,
    pub minimum_version: Option<String>,
    pub protocol_version: String,
    pub target_triple: String,
    pub accepted_license_expressions: Vec<String>,
}
