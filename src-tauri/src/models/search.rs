use serde::{Deserialize, Serialize};

use crate::models::wiki::WikiPageType;

/// Local keyword/tag/type/source search request. Never invokes an LLM or Agent.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub project_id: String,
    pub project_root_path: String,
    /// Optional keyword query. Matched case-insensitively against title, tags,
    /// sources, aliases, and body text. Empty query returns pages that satisfy
    /// the active filters, sorted by path.
    #[serde(default)]
    pub query: Option<String>,
    /// Restrict results to these page types. Empty = all types.
    #[serde(default)]
    pub page_types: Vec<WikiPageType>,
    /// Restrict results to pages tagged with any of these tags (OR semantics).
    #[serde(default)]
    pub tags: Vec<String>,
    /// Restrict results to pages referencing this source (substring match).
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub path: String,
    pub title: String,
    pub page_type: WikiPageType,
    pub starred: bool,
    /// Which fields matched the query: "title", "tags", "sources", "aliases", "content".
    #[serde(default)]
    pub matched_fields: Vec<String>,
    /// Text snippet around the first body match (empty when only metadata matched).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    /// Higher = better. Title > tags/sources/aliases > content.
    pub score: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub total: usize,
}

impl SearchResponse {
    pub fn empty() -> Self {
        Self {
            results: Vec::new(),
            total: 0,
        }
    }
}
