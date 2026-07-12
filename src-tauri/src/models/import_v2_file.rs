use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FileFormat {
    Markdown,
    Doc,
    Docx,
    Xls,
    Xlsx,
    Ppt,
    Pptx,
    Pdf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileIdentity {
    pub extension: String,
    pub magic: String,
    pub mime: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileSkipReason {
    SymlinkOrReparsePoint,
    HiddenOrSystem,
    ProjectInternal,
    UnsupportedFormat,
    CycleDetected,
    DepthLimitExceeded,
    FileLimitExceeded,
    FileTooLarge,
    Duplicate,
    CaseCollision,
    UnicodeNormalizationCollision,
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
    pub size_bytes: u64,
    pub identity: FileIdentity,
    pub source_identity: crate::models::import_v2::SourceIdentity,
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
