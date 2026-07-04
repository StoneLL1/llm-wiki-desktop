use serde::{Deserialize, Serialize};

pub const BOOKMARK_FILE_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum BookmarkResourceKind {
    WikiPage,
    ExportHtml,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkEntry {
    pub id: String,
    pub kind: BookmarkResourceKind,
    pub path: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export_record_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkFile {
    pub version: u32,
    #[serde(default)]
    pub entries: Vec<BookmarkEntry>,
}

impl Default for BookmarkFile {
    fn default() -> Self {
        Self {
            version: BOOKMARK_FILE_VERSION,
            entries: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExportBookmarkResponse {
    pub export_record_id: String,
    pub bookmarked: bool,
}
