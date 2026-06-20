use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::models::wiki::WikiPageType;

/// One node per Wiki page. `id` is the project-relative path (forward slashes)
/// so the frontend can navigate to the page directly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: String,
    /// Project-relative path with forward slashes, e.g. `wiki/concepts/x.md`.
    pub path: String,
    /// Display label (page title). May contain CJK characters.
    pub label: String,
    #[serde(rename = "type")]
    pub page_type: WikiPageType,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub starred: bool,
    /// Number of incident edges, filled in when the graph is built.
    #[serde(default)]
    pub degree: usize,
}

/// MVP edges all use relation `related`. `source`/`target` are node ids (paths).
/// `weight` is the number of association signals (wikilink + shared tags) that
/// produced the edge; the relation kind is intentionally single-valued for v1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub relation: String,
    #[serde(default = "default_edge_weight")]
    pub weight: usize,
}

fn default_edge_weight() -> usize {
    1
}

/// Layout + community data computed on the frontend (ForceAtlas2 + Louvain via
/// graphology) and persisted back so repeated opens skip recomputation when the
/// underlying wiki content has not changed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct GraphLayout {
    /// Node id -> [x, y].
    #[serde(default)]
    pub positions: HashMap<String, [f64; 2]>,
    /// Node id -> community id.
    #[serde(default)]
    pub communities: HashMap<String, usize>,
}

/// The full graph payload cached under `.app/graph-cache.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    /// Stable hash over the wiki page set (path + file hash). When it changes
    /// the cache is stale and the layout must be rebuilt.
    pub content_hash: String,
    /// RFC3339 build timestamp.
    pub built_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<GraphLayout>,
}

impl GraphData {
    pub fn empty(content_hash: String) -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            content_hash,
            built_at: crate::utils::time_utils::now_rfc3339(),
            layout: None,
        }
    }
}

/// Returned by `get_graph`: tells the UI whether the data came from cache and
/// whether the cached layout is still valid for the current wiki content.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphBuildResult {
    pub data: GraphData,
    /// True when the topology was served from `.app/graph-cache.json` without
    /// rescanning the wiki.
    pub cached: bool,
    /// True when the cached topology matches the live wiki but no usable
    /// persisted layout is available (missing, or doesn't cover every node).
    pub layout_stale: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphRequest {
    pub project_id: String,
    pub project_root_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveGraphLayoutRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub content_hash: String,
    #[serde(default)]
    pub positions: HashMap<String, [f64; 2]>,
    #[serde(default)]
    pub communities: HashMap<String, usize>,
}
