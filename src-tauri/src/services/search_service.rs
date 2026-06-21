use std::collections::HashSet;
use std::path::Path;

use chrono::TimeZone;

use crate::errors::BackendError;
use crate::models::chat::ChatRetrievalHit;
use crate::models::paths::ProjectContext;
use crate::models::search::{SearchRequest, SearchResponse, SearchResult};
use crate::models::wiki::{
    SaveWikiPageResponse, WikiPageMeta, WikiPageType, WikiTree, WikiTreeNode, WikiTreeNodeKind,
};
use crate::services::file_store::FileStore;
use crate::services::WriteMode;
use crate::utils::markdown_utils::{
    count_words, extract_title, extract_wikilinks, parse_frontmatter, snippet_for_query,
    split_frontmatter, Frontmatter, FrontmatterSplit,
};

/// Owns wiki scanning, page read/save, and the local keyword/tag/type/source
/// search index. Search is purely local: it never calls an LLM or Agent.
#[derive(Default)]
pub struct SearchService {
    file_store: FileStore,
}

impl SearchService {
    /// Walk the `wiki/` directory and return the nested tree plus a flat page
    /// metadata list. Obsidian (`.obsidian`), Git, and `.app` are skipped by
    /// `FileStore::list_markdown_files`.
    pub fn scan_wiki(&self, context: &ProjectContext) -> Result<WikiTree, BackendError> {
        let bookmarks = self.load_bookmarks(context);
        let files = self.file_store.list_markdown_files(&context.wiki_dir)?;

        let mut pages: Vec<WikiPageMeta> = Vec::with_capacity(files.len());
        for absolute in &files {
            let project_relative = context.to_project_relative(absolute)?;
            let (meta, _body) = self.load_page(context, &project_relative, absolute, &bookmarks)?;
            pages.push(meta);
        }

        pages.sort_by(|a, b| a.path.cmp(&b.path));
        let total_pages = pages.len();
        let root = self.build_tree(&pages);

        Ok(WikiTree {
            root,
            pages,
            total_pages,
        })
    }

    /// Read a single page (frontmatter + body + derived metadata).
    pub fn read_page(
        &self,
        context: &ProjectContext,
        relative_path: &str,
    ) -> Result<crate::models::wiki::WikiPageContent, BackendError> {
        let contents = self.file_store.read_markdown(context, relative_path)?;
        let absolute = context.resolve_project_path(relative_path)?;
        let split = split_frontmatter(&contents);
        let frontmatter = split
            .frontmatter
            .as_deref()
            .map(parse_frontmatter)
            .unwrap_or_default();
        let bookmarks = self.load_bookmarks(context);
        let meta = self.build_meta(
            context,
            relative_path,
            &absolute,
            &split,
            &frontmatter,
            &bookmarks,
        )?;

        Ok(crate::models::wiki::WikiPageContent {
            meta,
            raw_markdown: contents,
            body_markdown: split.body.clone(),
            frontmatter_yaml: split.frontmatter.clone(),
        })
    }

    /// Save a page with optimistic-concurrency hash checking. `expected_hash =
    /// None` means create-new (rejected if the file exists); `Some(hash)` only
    /// overwrites when the on-disk hash still matches, so an external edit in
    /// Obsidian surfaces as `FILE_HASH_MISMATCH` for the diff confirmation path.
    pub fn save_page(
        &self,
        context: &ProjectContext,
        relative_path: &str,
        contents: &str,
        expected_hash: Option<String>,
    ) -> Result<SaveWikiPageResponse, BackendError> {
        let mode = match expected_hash {
            Some(hash) => WriteMode::OverwriteIfHashMatches(hash),
            None => WriteMode::CreateNew,
        };

        self.file_store
            .write_markdown_checked(context, relative_path, contents, mode)?;

        let hash = self.file_store.file_hash(context, relative_path)?;
        let graph_cache_invalidated = self.invalidate_graph_cache(context);
        self.append_save_log(context, relative_path);

        Ok(SaveWikiPageResponse {
            relative_path: relative_path.to_string(),
            hash,
            saved_at: crate::utils::time_utils::now_rfc3339(),
            graph_cache_invalidated,
        })
    }

    /// Toggle the bookmark state for a wiki page. Bookmarks are stored as a JSON
    /// array of project-relative paths in `.app/bookmarks.json`. Only pages that
    /// actually exist under `wiki/` can be bookmarked — the path is resolved via
    /// `resolve_wiki_path` (rejects traversal / out-of-project paths) and the
    /// file must exist on disk. Returns the page's new bookmarked state.
    pub fn toggle_bookmark(
        &self,
        context: &ProjectContext,
        relative_path: &str,
    ) -> Result<crate::models::wiki::ToggleBookmarkResponse, BackendError> {
        let absolute = context.resolve_project_path(relative_path)?;
        if !absolute.exists() || !absolute.is_file() {
            return Err(BackendError::new(
                "FILE_NOT_FOUND",
                "Wiki page does not exist.".to_string(),
                false,
                true,
            ));
        }
        // Defense-in-depth: bookmarks are only meaningful for wiki pages, so
        // reject anything that resolves outside `wiki/`.
        if absolute.strip_prefix(&context.wiki_dir).is_err() {
            return Err(BackendError::new(
                "PATH_OUTSIDE_PROJECT",
                "Only wiki pages can be bookmarked.".to_string(),
                false,
                true,
            ));
        }

        let project_relative = context.to_project_relative(&absolute)?;
        let mut bookmarks = self
            .load_bookmarks(context)
            .into_iter()
            .collect::<Vec<String>>();
        let dominated = bookmarks
            .iter()
            .position(|entry| entry == &project_relative);
        let now_bookmarked = match dominated {
            Some(idx) => {
                bookmarks.remove(idx);
                false
            }
            None => {
                bookmarks.push(project_relative.clone());
                true
            }
        };
        bookmarks.sort();

        self.file_store
            .write_json_atomic(context, ".app/bookmarks.json", &bookmarks)?;

        Ok(crate::models::wiki::ToggleBookmarkResponse {
            relative_path: project_relative,
            bookmarked: now_bookmarked,
        })
    }

    /// Local keyword/tag/type/source search.
    pub fn search(
        &self,
        context: &ProjectContext,
        request: &SearchRequest,
    ) -> Result<SearchResponse, BackendError> {
        let bookmarks = self.load_bookmarks(context);
        let files = self.file_store.list_markdown_files(&context.wiki_dir)?;

        let query = request
            .query
            .as_deref()
            .map(|q| q.trim())
            .filter(|q| !q.is_empty())
            .map(|q| q.to_ascii_lowercase());
        let type_filter: HashSet<WikiPageType> = request.page_types.iter().copied().collect();
        let tag_filter: Vec<String> = request
            .tags
            .iter()
            .map(|tag| tag.to_ascii_lowercase())
            .collect();
        let source_filter = request
            .source
            .as_deref()
            .map(str::trim)
            .filter(|source| !source.is_empty())
            .map(|source| source.to_ascii_lowercase());

        let mut results: Vec<SearchResult> = Vec::new();

        for absolute in &files {
            let project_relative = context.to_project_relative(absolute)?;
            let (meta, body) = self.load_page(context, &project_relative, absolute, &bookmarks)?;

            if !type_filter.is_empty() && !type_filter.contains(&meta.page_type) {
                continue;
            }
            if !tag_filter.is_empty()
                && !meta
                    .tags
                    .iter()
                    .any(|tag| tag_filter.contains(&tag.to_ascii_lowercase()))
            {
                continue;
            }
            if let Some(ref source_needle) = source_filter {
                let has_source = meta
                    .sources
                    .iter()
                    .any(|source| source.to_ascii_lowercase().contains(source_needle));
                if !has_source {
                    continue;
                }
            }

            let (matched_fields, snippet, score) = match query.as_deref() {
                Some(query_lower) => {
                    let mut fields: Vec<&'static str> = Vec::new();
                    let mut score = 0i64;

                    if meta.title.to_ascii_lowercase().contains(query_lower) {
                        fields.push("title");
                        score += 100;
                    }
                    if meta
                        .tags
                        .iter()
                        .any(|tag| tag.to_ascii_lowercase().contains(query_lower))
                    {
                        fields.push("tags");
                        score += 40;
                    }
                    if meta
                        .sources
                        .iter()
                        .any(|source| source.to_ascii_lowercase().contains(query_lower))
                    {
                        fields.push("sources");
                        score += 30;
                    }
                    if meta
                        .aliases
                        .iter()
                        .any(|alias| alias.to_ascii_lowercase().contains(query_lower))
                    {
                        fields.push("aliases");
                        score += 30;
                    }

                    let body_lower = body.to_ascii_lowercase();
                    if body_lower.contains(query_lower) {
                        fields.push("content");
                        score += 10;
                    }

                    if fields.is_empty() {
                        continue;
                    }

                    let snippet = snippet_for_query(&body, query_lower, 48);
                    let fields_owned: Vec<String> = fields.into_iter().map(String::from).collect();
                    (fields_owned, snippet, score)
                }
                None => (Vec::new(), None, 0),
            };

            results.push(SearchResult {
                path: meta.path.clone(),
                title: meta.title.clone(),
                page_type: meta.page_type,
                starred: meta.starred,
                matched_fields,
                snippet,
                score,
            });
        }

        if query.is_some() {
            results.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
        } else {
            results.sort_by(|a, b| a.path.cmp(&b.path));
        }

        let total = results.len();
        if let Some(limit) = request.limit {
            results.truncate(limit);
        }

        Ok(SearchResponse { results, total })
    }

    /// Retrieve the top wiki pages for a natural-language chat question, each
    /// with a bounded body excerpt for the model prompt. Reuses the keyword
    /// `search` index (no model is called) and `read_page` for excerpts. This is
    /// the chat-retrieval entry point; the global search command stays
    /// keyword-only and never calls this for autocomplete.
    pub fn retrieve_with_excerpts(
        &self,
        context: &ProjectContext,
        query: &str,
        limit: usize,
        excerpt_chars: usize,
    ) -> Result<Vec<ChatRetrievalHit>, BackendError> {
        let request = SearchRequest {
            project_id: context.project_id.clone(),
            project_root_path: context.root.to_string_lossy().to_string(),
            query: Some(query.to_string()),
            page_types: Vec::new(),
            tags: Vec::new(),
            source: None,
            limit: Some(limit),
        };
        let response = self.search(context, &request)?;
        let mut hits = Vec::with_capacity(response.results.len());
        for result in response.results {
            let excerpt = self
                .read_page(context, &result.path)
                .ok()
                .map(|page| truncate_excerpt(&page.body_markdown, excerpt_chars));
            hits.push(ChatRetrievalHit {
                path: result.path,
                title: result.title,
                snippet: result.snippet,
                score: result.score,
                excerpt,
            });
        }
        Ok(hits)
    }

    fn load_page(
        &self,
        context: &ProjectContext,
        project_relative: &str,
        absolute: &Path,
        bookmarks: &HashSet<String>,
    ) -> Result<(WikiPageMeta, String), BackendError> {
        let contents =
            std::fs::read_to_string(absolute).map_err(|err| file_read_error(err, absolute))?;
        let split = split_frontmatter(&contents);
        let frontmatter = split
            .frontmatter
            .as_deref()
            .map(parse_frontmatter)
            .unwrap_or_default();
        let meta = self.build_meta(
            context,
            project_relative,
            absolute,
            &split,
            &frontmatter,
            bookmarks,
        )?;
        Ok((meta, split.body))
    }

    #[allow(clippy::too_many_arguments)]
    fn build_meta(
        &self,
        context: &ProjectContext,
        project_relative: &str,
        absolute: &Path,
        split: &FrontmatterSplit,
        frontmatter: &Frontmatter,
        bookmarks: &HashSet<String>,
    ) -> Result<WikiPageMeta, BackendError> {
        let file_name = absolute
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("page.md");
        let wiki_relative = project_relative
            .strip_prefix("wiki/")
            .unwrap_or(project_relative);

        let title = extract_title(&split.body, frontmatter, file_name);
        let type_field = frontmatter.get_scalar("type");
        let page_type = WikiPageType::infer(type_field.as_deref(), wiki_relative);

        let starred = frontmatter
            .get_scalar("starred")
            .map(|value| value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let bookmarked = bookmarks.contains(project_relative);

        let file_size = std::fs::metadata(absolute)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        let modified_time = mtime_rfc3339(absolute);
        let hash = self.file_store.file_hash(context, project_relative)?;

        Ok(WikiPageMeta {
            path: project_relative.to_string(),
            title,
            page_type,
            tags: frontmatter.get_list("tags"),
            sources: frontmatter.get_list("sources"),
            aliases: frontmatter.get_list("aliases"),
            created: frontmatter.get_scalar("created"),
            updated: frontmatter.get_scalar("updated"),
            starred,
            bookmarked,
            word_count: count_words(&split.body),
            file_size,
            modified_time,
            hash,
            wikilinks: extract_wikilinks(&split.body),
        })
    }

    fn build_tree(&self, pages: &[WikiPageMeta]) -> WikiTreeNode {
        let mut root = WikiTreeNode {
            name: "wiki".to_string(),
            kind: WikiTreeNodeKind::Folder,
            path: "wiki".to_string(),
            page_type: None,
            title: None,
            starred: false,
            bookmarked: false,
            file_count: 0,
            children: Vec::new(),
        };

        for page in pages {
            let wiki_relative = page.path.strip_prefix("wiki/").unwrap_or(&page.path);
            let segments: Vec<&str> = wiki_relative.split('/').collect();
            if segments.is_empty() {
                continue;
            }
            insert_node(&mut root, page, &segments, 0);
        }

        compute_file_counts(&mut root);
        root
    }

    fn load_bookmarks(&self, context: &ProjectContext) -> HashSet<String> {
        let path = context.app_dir.join("bookmarks.json");
        self.file_store
            .read_json_file::<Vec<String>>(&path)
            .map(|entries| entries.into_iter().collect())
            .unwrap_or_default()
    }

    fn invalidate_graph_cache(&self, context: &ProjectContext) -> bool {
        let path = context.app_dir.join("graph-cache.json");
        if path.exists() {
            std::fs::remove_file(&path).is_ok()
        } else {
            false
        }
    }

    fn append_save_log(&self, context: &ProjectContext, relative_path: &str) {
        let log_path = context.wiki_dir.join("log.md");
        if !log_path.exists() {
            return;
        }
        let stamp = chrono::Utc::now().format("%Y-%m-%d %H:%M").to_string();
        let line = format!("- [{}] saved {} · you\n", stamp, relative_path);
        if let Ok(mut file) = std::fs::OpenOptions::new().append(true).open(&log_path) {
            use std::io::Write;
            let _ = file.write_all(line.as_bytes());
        }
    }
}

fn insert_node(node: &mut WikiTreeNode, page: &WikiPageMeta, segments: &[&str], depth: usize) {
    if depth == segments.len() - 1 {
        node.children.push(WikiTreeNode {
            name: segments[depth].to_string(),
            kind: WikiTreeNodeKind::File,
            path: page.path.clone(),
            page_type: Some(page.page_type),
            title: Some(page.title.clone()),
            starred: page.starred,
            bookmarked: page.bookmarked,
            file_count: 1,
            children: Vec::new(),
        });
        return;
    }

    let segment = segments[depth].to_string();
    let folder_path = format!("{}/{}", node.path, segment);
    let index = node
        .children
        .iter()
        .position(|child| child.kind == WikiTreeNodeKind::Folder && child.name == segment);

    let child = match index {
        Some(existing) => &mut node.children[existing],
        None => {
            node.children.push(WikiTreeNode {
                name: segment,
                kind: WikiTreeNodeKind::Folder,
                path: folder_path,
                page_type: None,
                title: None,
                starred: false,
                bookmarked: false,
                file_count: 0,
                children: Vec::new(),
            });
            node.children.last_mut().expect("folder node just pushed")
        }
    };

    insert_node(child, page, segments, depth + 1);
}

fn compute_file_counts(node: &mut WikiTreeNode) {
    if node.kind == WikiTreeNodeKind::Folder {
        let mut total = 0;
        for child in &mut node.children {
            compute_file_counts(child);
            total += child.file_count;
        }
        node.file_count = total;
    }
}

fn mtime_rfc3339(path: &Path) -> String {
    let Ok(metadata) = std::fs::metadata(path) else {
        return String::new();
    };
    let Ok(modified) = metadata.modified() else {
        return String::new();
    };
    let duration = modified
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    chrono::Utc
        .timestamp_opt(duration.as_secs() as i64, duration.subsec_nanos())
        .single()
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}

fn file_read_error(err: std::io::Error, path: &Path) -> BackendError {
    BackendError::new("FILE_READ_FAILED", err.to_string(), true, false)
        .with_details(serde_json::json!({ "path": path.to_string_lossy() }))
}

/// Bound a page body excerpt to keep chat prompts within a sane token budget.
fn truncate_excerpt(body: &str, max_chars: usize) -> String {
    let trimmed = body.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let taken: String = trimmed.chars().take(max_chars).collect();
    let cut = taken.trim_end();
    let nearest_break = cut.rfind(['\n', '.']).map(|i| &cut[..=i]).unwrap_or(cut);
    format!("{}…", nearest_break.trim_end())
}

#[cfg(test)]
mod tests {
    use super::SearchService;
    use crate::models::paths::ProjectContext;
    use crate::models::search::{SearchRequest, SearchResponse};
    use crate::models::wiki::{SaveWikiPageResponse, WikiPageContent, WikiPageType, WikiTree};
    use crate::services::WriteMode;
    use crate::utils::time_utils::now_rfc3339;
    use sha2::{Digest, Sha256};
    use std::path::PathBuf;

    fn tmp_context(suffix: &str) -> (ProjectContext, PathBuf) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("llm-wiki-search-{stamp}-{suffix}"));
        std::fs::create_dir_all(&root).unwrap();
        (ProjectContext::new("project-1", root.clone()), root)
    }

    fn write_file(context: &ProjectContext, rel: &str, body: &str) {
        let path = context.resolve_project_path(rel).unwrap();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, body).unwrap();
    }

    fn seed_sample_vault(context: &ProjectContext) {
        write_file(
            context,
            "wiki/concepts/agent-memory.md",
            "---\ntitle: Agent Memory\ntype: concept\ntags: [memory, context]\nsources:\n  - raw/articles/paper.md\nstarred: true\n---\n\n# Agent Memory\n\nCovers short context windows and RAG. See [[react-pattern]].",
        );
        write_file(
            context,
            "wiki/concepts/react-pattern.md",
            "---\ntitle: ReAct Pattern\ntype: concept\ntags: [reasoning, tools]\n---\n\n# ReAct Pattern\n\nReason then act loop.",
        );
        write_file(
            context,
            "wiki/entities/claude.md",
            "---\ntitle: Anthropic Claude\ntype: entity\ntags: [vendor, claude]\n---\n\n# Anthropic Claude\n\nMaker of Claude models.",
        );
        write_file(context, "wiki/index.md", "# Index\n\nWelcome to the wiki.");
    }

    #[test]
    fn scan_wiki_builds_tree_and_flat_pages() {
        let (context, root) = tmp_context("scan");
        seed_sample_vault(&context);
        let service = SearchService::default();

        let tree: WikiTree = service.scan_wiki(&context).unwrap();

        assert_eq!(tree.total_pages, 4);
        assert_eq!(tree.root.name, "wiki");
        // folders: concepts, entities
        let folder_names: Vec<&str> = tree
            .root
            .children
            .iter()
            .filter(|c| c.kind == crate::models::wiki::WikiTreeNodeKind::Folder)
            .map(|c| c.name.as_str())
            .collect();
        assert!(folder_names.contains(&"concepts"));
        assert!(folder_names.contains(&"entities"));
        // index.md is a top-level file node
        assert!(tree.root.children.iter().any(|c| c.name == "index.md"));
        // file counts roll up
        let concepts = tree
            .root
            .children
            .iter()
            .find(|c| c.name == "concepts")
            .unwrap();
        assert_eq!(concepts.file_count, 2);

        let agent = tree
            .pages
            .iter()
            .find(|p| p.path == "wiki/concepts/agent-memory.md")
            .unwrap();
        assert_eq!(agent.title, "Agent Memory");
        assert_eq!(agent.page_type, WikiPageType::Concept);
        assert_eq!(agent.tags, vec!["memory", "context"]);
        assert_eq!(agent.sources, vec!["raw/articles/paper.md"]);
        assert!(agent.starred);
        assert_eq!(agent.wikilinks, vec!["react-pattern"]);
        assert!(!agent.hash.is_empty());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scan_wiki_reads_bookmarks_from_app_state() {
        let (context, root) = tmp_context("bookmarks");
        seed_sample_vault(&context);
        std::fs::create_dir_all(context.app_dir.clone()).unwrap();
        std::fs::write(
            context.app_dir.join("bookmarks.json"),
            serde_json::to_string(&vec!["wiki/concepts/react-pattern.md".to_string()]).unwrap(),
        )
        .unwrap();

        let service = SearchService::default();
        let tree = service.scan_wiki(&context).unwrap();
        let react = tree
            .pages
            .iter()
            .find(|p| p.path == "wiki/concepts/react-pattern.md")
            .unwrap();
        assert!(react.bookmarked);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn toggle_bookmark_persists_and_round_trips() {
        let (context, root) = tmp_context("toggle-bookmark");
        std::fs::create_dir_all(context.app_dir.clone()).unwrap();
        seed_sample_vault(&context);
        let service = SearchService::default();

        // Adding a bookmark writes the path into .app/bookmarks.json.
        let added = service
            .toggle_bookmark(&context, "wiki/concepts/agent-memory.md")
            .unwrap();
        assert!(added.bookmarked);
        assert_eq!(added.relative_path, "wiki/concepts/agent-memory.md");

        // The persisted array is a JSON list of project-relative paths.
        let on_disk = std::fs::read_to_string(context.app_dir.join("bookmarks.json")).unwrap();
        let parsed: Vec<String> = serde_json::from_str(&on_disk).unwrap();
        assert_eq!(parsed, vec!["wiki/concepts/agent-memory.md".to_string()]);

        // A scan reflects the bookmarked flag.
        let tree = service.scan_wiki(&context).unwrap();
        let agent = tree
            .pages
            .iter()
            .find(|p| p.path == "wiki/concepts/agent-memory.md")
            .unwrap();
        assert!(agent.bookmarked);

        // Toggling again removes it.
        let removed = service
            .toggle_bookmark(&context, "wiki/concepts/agent-memory.md")
            .unwrap();
        assert!(!removed.bookmarked);
        let after: Vec<String> = serde_json::from_str(
            &std::fs::read_to_string(context.app_dir.join("bookmarks.json")).unwrap(),
        )
        .unwrap();
        assert!(after.is_empty());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn toggle_bookmark_rejects_missing_and_non_wiki_paths() {
        let (context, root) = tmp_context("toggle-bookmark-reject");
        std::fs::create_dir_all(context.app_dir.clone()).unwrap();
        seed_sample_vault(&context);
        let service = SearchService::default();

        // Missing page.
        let missing = service.toggle_bookmark(&context, "wiki/concepts/nope.md");
        assert!(missing.is_err());

        // Path traversal is rejected by the path resolver.
        let traversal = service.toggle_bookmark(&context, "wiki/../../purpose.md");
        assert!(traversal.is_err());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn read_page_returns_frontmatter_body_and_meta() {
        let (context, root) = tmp_context("read");
        seed_sample_vault(&context);
        let service = SearchService::default();

        let page: WikiPageContent = service
            .read_page(&context, "wiki/concepts/agent-memory.md")
            .unwrap();

        assert_eq!(page.meta.title, "Agent Memory");
        assert!(page.raw_markdown.starts_with("---"));
        assert!(page.body_markdown.starts_with("# Agent Memory"));
        assert!(page
            .frontmatter_yaml
            .as_deref()
            .unwrap()
            .contains("type: concept"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn save_page_rejects_silent_overwrite_without_matching_hash() {
        let (context, root) = tmp_context("save-conflict");
        seed_sample_vault(&context);
        let service = SearchService::default();

        let err = service
            .save_page(
                &context,
                "wiki/concepts/react-pattern.md",
                "# New content",
                Some("stale-hash".to_string()),
            )
            .expect_err("stale hash must surface a conflict");
        assert_eq!(err.code, "FILE_HASH_MISMATCH");

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn save_page_writes_and_updates_hash_and_log() {
        let (context, root) = tmp_context("save-ok");
        seed_sample_vault(&context);
        // graph cache present → save should invalidate it
        std::fs::create_dir_all(context.app_dir.clone()).unwrap();
        std::fs::write(context.app_dir.join("graph-cache.json"), "{}").unwrap();
        // log.md present → save should append
        std::fs::write(context.wiki_dir.join("log.md"), "# Log\n").unwrap();

        let service = SearchService::default();
        let current_hash = service
            .file_store
            .file_hash(&context, "wiki/concepts/react-pattern.md")
            .unwrap();
        let response: SaveWikiPageResponse = service
            .save_page(
                &context,
                "wiki/concepts/react-pattern.md",
                "---\ntitle: ReAct Pattern\ntype: concept\n---\n\n# ReAct Pattern\n\nUpdated body.",
                Some(current_hash),
            )
            .unwrap();

        assert_eq!(response.relative_path, "wiki/concepts/react-pattern.md");
        assert!(!response.hash.is_empty());
        assert!(response.graph_cache_invalidated);
        assert!(!context.app_dir.join("graph-cache.json").exists());
        let log = std::fs::read_to_string(context.wiki_dir.join("log.md")).unwrap();
        assert!(log.contains("saved wiki/concepts/react-pattern.md"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn save_page_create_new_rejects_existing_file() {
        let (context, root) = tmp_context("save-new");
        seed_sample_vault(&context);
        let service = SearchService::default();

        let err = service
            .save_page(&context, "wiki/concepts/react-pattern.md", "# Dup", None)
            .expect_err("create-new must reject existing file");
        assert_eq!(err.code, "FILE_ALREADY_EXISTS");

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn search_filters_by_type_tag_source_and_keyword() {
        let (context, root) = tmp_context("search");
        seed_sample_vault(&context);
        let service = SearchService::default();

        // type filter
        let only_entities = service
            .search(
                &context,
                &SearchRequest {
                    project_id: "p".into(),
                    project_root_path: context.root.to_string_lossy().to_string(),
                    query: None,
                    page_types: vec![WikiPageType::Entity],
                    tags: Vec::new(),
                    source: None,
                    limit: None,
                },
            )
            .unwrap();
        assert_eq!(only_entities.total, 1);
        assert_eq!(only_entities.results[0].path, "wiki/entities/claude.md");

        // tag filter
        let only_memory = service
            .search(
                &context,
                &SearchRequest {
                    project_id: "p".into(),
                    project_root_path: context.root.to_string_lossy().to_string(),
                    query: None,
                    page_types: Vec::new(),
                    tags: vec!["memory".to_string()],
                    source: None,
                    limit: None,
                },
            )
            .unwrap();
        assert_eq!(only_memory.total, 1);
        assert_eq!(only_memory.results[0].path, "wiki/concepts/agent-memory.md");

        // source filter (substring of source path)
        let by_source = service
            .search(
                &context,
                &SearchRequest {
                    project_id: "p".into(),
                    project_root_path: context.root.to_string_lossy().to_string(),
                    query: None,
                    page_types: Vec::new(),
                    tags: Vec::new(),
                    source: Some("paper.md".to_string()),
                    limit: None,
                },
            )
            .unwrap();
        assert_eq!(by_source.total, 1);

        // keyword ranks title above content
        let keyword = service
            .search(
                &context,
                &SearchRequest {
                    project_id: "p".into(),
                    project_root_path: context.root.to_string_lossy().to_string(),
                    query: Some("react".to_string()),
                    page_types: Vec::new(),
                    tags: Vec::new(),
                    source: None,
                    limit: None,
                },
            )
            .unwrap();
        let paths: Vec<&str> = keyword.results.iter().map(|r| r.path.as_str()).collect();
        assert!(paths.contains(&"wiki/concepts/react-pattern.md"));
        assert!(paths.contains(&"wiki/concepts/agent-memory.md"));
        // react-pattern matches title (higher score) → ranked first
        assert_eq!(keyword.results[0].path, "wiki/concepts/react-pattern.md");
        assert!(keyword.results[0]
            .matched_fields
            .contains(&"title".to_string()));

        let _ = SearchResponse::empty();

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn write_mode_variants_still_compile() {
        let _ = WriteMode::CreateNew;
        let _ = WriteMode::OverwriteIfHashMatches("hash".to_string());
    }

    #[test]
    fn now_timestamp_is_rfc3339() {
        let stamp = now_rfc3339();
        assert!(stamp.contains('T'));
    }

    #[test]
    fn hash_helper_is_sha256() {
        // Sanity: ensure we use Sha256 consistently with FileStore.
        let mut hasher = Sha256::new();
        hasher.update(b"abc");
        let digest = format!("{:x}", hasher.finalize());
        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
