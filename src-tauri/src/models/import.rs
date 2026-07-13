use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportRequest {
    pub source_paths: Vec<String>,
    pub allow_duplicates: bool,
    pub link_duplicates: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub files: Vec<ImportFileEntry>,
    pub conflicts: Vec<ImportConflict>,
    pub summary: ImportSummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub v2_session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportFileEntry {
    pub original_name: String,
    pub source_path: String,
    pub archived_path: String,
    pub file_type: SourceFileType,
    pub size_bytes: u64,
    pub hash: String,
    pub extraction_status: ExtractionStatus,
    pub extraction_error: Option<String>,
    pub text_preview: Option<String>,
    pub page_count: Option<u32>,
    pub word_count: Option<u32>,
    pub metadata: Option<SourceMetadata>,
    #[serde(default)]
    pub extracted_text_path: Option<String>,
    #[serde(default)]
    pub extracted_assets: Vec<String>,
    pub conflict: Option<ImportConflict>,
    pub renamed_from: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SourceFileType {
    Pdf,
    Document,
    Presentation,
    Spreadsheet,
    Markdown,
    Text,
    Image,
    Html,
    Csv,
    Url,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ExtractionStatus {
    Pending,
    Extracted,
    Unsupported,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub created: Option<String>,
    pub modified: Option<String>,
    pub page_count: Option<u32>,
    pub word_count: Option<u32>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportConflict {
    pub original_name: String,
    pub conflict_type: ConflictType,
    pub existing_path: Option<String>,
    pub resolved_path: String,
    pub existing_hash: Option<String>,
    pub new_hash: String,
    pub resolution: Option<ConflictResolution>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictType {
    ExactDuplicate,
    NameCollision,
    PathConflict,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    Skip,
    LinkToExisting,
    Rename,
    Overwrite,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub total_files: u32,
    pub archived_files: u32,
    pub duplicate_files: u32,
    pub renamed_files: u32,
    pub failed_files: u32,
    pub conflicts_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExtractResult {
    pub original_name: String,
    pub file_type: SourceFileType,
    pub status: ExtractionStatus,
    pub error: Option<String>,
    pub text_preview: Option<String>,
    pub metadata: Option<SourceMetadata>,
    pub extracted_text_path: Option<String>,
    pub extracted_assets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmedImport {
    pub preview: ImportPreview,
    pub confirmed_at: String,
    /// Commit hash of the scoped Git checkpoint created when the caller
    /// requested `create_checkpoint: true` on confirm. Serialized as JSON
    /// `null` (not omitted) when no checkpoint was created, matching the
    /// `PendingAction.checkpoint_hash` convention so the frontend can use a
    /// uniform `!== null` check.
    #[serde(default)]
    pub checkpoint_hash: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceArtifactIndex {
    #[serde(default)]
    pub sources: std::collections::HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportedSource {
    pub path: String,
    pub size_bytes: u64,
    pub file_type: SourceFileType,
}
