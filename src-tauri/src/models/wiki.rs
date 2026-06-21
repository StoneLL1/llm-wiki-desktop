use serde::{Deserialize, Serialize};

/// Coarse page classification used for tree grouping, type filters, and search.
///
/// Inferred from the YAML `type` field when present and recognized, otherwise
/// from the top-level `wiki/` subdirectory (`entities`, `concepts`, ...).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum WikiPageType {
    Entity,
    Concept,
    Source,
    Synthesis,
    Comparison,
    Query,
    Index,
    Overview,
    Log,
    #[default]
    Other,
}

impl WikiPageType {
    /// Infer the page type from frontmatter `type` and the wiki-relative path.
    ///
    /// `wiki_relative_path` is the path relative to the `wiki/` directory, e.g.
    /// `concepts/agent-memory.md` or `index.md`.
    pub fn infer(type_field: Option<&str>, wiki_relative_path: &str) -> Self {
        if let Some(normalized) = type_field.map(|raw| raw.trim().to_ascii_lowercase()) {
            if !normalized.is_empty() {
                match normalized.as_str() {
                    "entity" | "entities" => return WikiPageType::Entity,
                    "concept" | "concepts" => return WikiPageType::Concept,
                    "source" | "sources" => return WikiPageType::Source,
                    "synthesis" | "syntheses" => return WikiPageType::Synthesis,
                    "comparison" | "comparisons" => return WikiPageType::Comparison,
                    "query" | "queries" => return WikiPageType::Query,
                    "index" => return WikiPageType::Index,
                    "overview" => return WikiPageType::Overview,
                    "log" | "changelog" => return WikiPageType::Log,
                    _ => {}
                }
            }
        }

        let normalized_path = wiki_relative_path.replace('\\', "/");
        let first_segment = normalized_path.split('/').next().unwrap_or("");
        let file_name = normalized_path
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();

        match first_segment {
            "entities" => WikiPageType::Entity,
            "concepts" => WikiPageType::Concept,
            "sources" => WikiPageType::Source,
            "synthesis" => WikiPageType::Synthesis,
            "comparisons" => WikiPageType::Comparison,
            "queries" => WikiPageType::Query,
            _ => match file_name.as_str() {
                "index.md" => WikiPageType::Index,
                "overview.md" => WikiPageType::Overview,
                "log.md" => WikiPageType::Log,
                _ => WikiPageType::Other,
            },
        }
    }
}

/// Metadata for a single wiki page, returned by the tree scan and search.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WikiPageMeta {
    /// Project-relative path with forward slashes, e.g. `wiki/concepts/x.md`.
    pub path: String,
    pub title: String,
    #[serde(default)]
    pub page_type: WikiPageType,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<String>,
    #[serde(default)]
    pub starred: bool,
    #[serde(default)]
    pub bookmarked: bool,
    pub word_count: usize,
    pub file_size: u64,
    /// RFC3339 filesystem modification time.
    pub modified_time: String,
    /// SHA-256 of the file bytes; used for external-modification conflict checks.
    pub hash: String,
    /// Outgoing `[[wikilink]]` targets (deduplicated, order preserved).
    #[serde(default)]
    pub wikilinks: Vec<String>,
}

/// A node in the wiki file tree. Folders contain children; files carry metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WikiTreeNode {
    pub name: String,
    pub kind: WikiTreeNodeKind,
    /// Project-relative path with forward slashes (folder root is `wiki`).
    pub path: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub page_type: Option<WikiPageType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub starred: bool,
    #[serde(default)]
    pub bookmarked: bool,
    /// Descendant markdown file count for folders; 1 for files.
    pub file_count: usize,
    #[serde(default)]
    pub children: Vec<WikiTreeNode>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WikiTreeNodeKind {
    Folder,
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WikiTree {
    pub root: WikiTreeNode,
    /// Flat list of every page's metadata, sorted by path.
    #[serde(default)]
    pub pages: Vec<WikiPageMeta>,
    pub total_pages: usize,
}

/// Full page content returned by `read_wiki_page`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WikiPageContent {
    pub meta: WikiPageMeta,
    /// Full file content including the frontmatter block.
    pub raw_markdown: String,
    /// Body content with the frontmatter block stripped.
    pub body_markdown: String,
    /// Raw YAML frontmatter text (without the `---` fences), if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frontmatter_yaml: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadWikiPageRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub relative_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveWikiPageRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub relative_path: String,
    pub contents: String,
    /// Hash of the file the editor last read. When `None` the write is treated
    /// as create-new (rejected if the file already exists). When `Some`, the
    /// write only proceeds if the on-disk hash still matches — an external
    /// edit surfaces as `FILE_HASH_MISMATCH` for the diff confirmation path.
    #[serde(default)]
    pub expected_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveWikiPageResponse {
    pub relative_path: String,
    pub hash: String,
    pub saved_at: String,
    /// True when a pre-existing graph cache was invalidated by this save.
    pub graph_cache_invalidated: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggleBookmarkRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub relative_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggleBookmarkResponse {
    pub relative_path: String,
    pub bookmarked: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWikiPageRequest {
    pub project_id: String,
    pub project_root_path: String,
    /// Project-relative path under `wiki/` for the new page, e.g.
    /// `wiki/concepts/agent-memory.md`. Must not already exist.
    pub relative_path: String,
    /// Optional title; defaults to the filename stem when omitted. Used both in
    /// the seeded frontmatter `title` field and the `# {title}` H1.
    #[serde(default)]
    pub title: Option<String>,
    /// Optional page type written into frontmatter `type`. When omitted the
    /// frontmatter omits `type` and `WikiPageType::infer` falls back to the
    /// `wiki/` subdirectory.
    #[serde(default)]
    pub page_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameWikiPageRequest {
    pub project_id: String,
    pub project_root_path: String,
    /// Project-relative path of the existing page to rename.
    pub relative_path: String,
    /// New project-relative path under `wiki/`, e.g.
    /// `wiki/concepts/reasoning-loop.md`. Must not already exist.
    pub new_relative_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameWikiPageResponse {
    /// The new project-relative path of the renamed page.
    pub relative_path: String,
    pub hash: String,
    pub saved_at: String,
    /// Other wiki pages whose `[[old]]` wikilinks were rewritten to `[[new]]`.
    pub updated_references: Vec<String>,
    /// True when a pre-existing graph cache was invalidated by this rename.
    pub graph_cache_invalidated: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteWikiPageRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub relative_path: String,
}

#[cfg(test)]
mod tests {
    use super::WikiPageType;

    #[test]
    fn infers_type_from_frontmatter_type_field() {
        assert_eq!(
            WikiPageType::infer(Some("concept"), "concepts/x.md"),
            WikiPageType::Concept
        );
        assert_eq!(
            WikiPageType::infer(Some("Entity"), "anything/x.md"),
            WikiPageType::Entity
        );
        assert_eq!(
            WikiPageType::infer(Some("syntheses"), "s/x.md"),
            WikiPageType::Synthesis
        );
    }

    #[test]
    fn falls_back_to_directory_when_type_absent_or_unknown() {
        assert_eq!(
            WikiPageType::infer(None, "entities/claude.md"),
            WikiPageType::Entity
        );
        assert_eq!(
            WikiPageType::infer(None, "sources/paper.md"),
            WikiPageType::Source
        );
        assert_eq!(WikiPageType::infer(None, "index.md"), WikiPageType::Index);
        assert_eq!(WikiPageType::infer(None, "log.md"), WikiPageType::Log);
        assert_eq!(
            WikiPageType::infer(Some("custom"), "notes/free.md"),
            WikiPageType::Other
        );
    }

    #[test]
    fn handles_windows_separators_in_path() {
        assert_eq!(
            WikiPageType::infer(None, "concepts\\agent.md"),
            WikiPageType::Concept
        );
    }
}
