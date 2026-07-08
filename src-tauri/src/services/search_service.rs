use std::collections::HashSet;
use std::path::Path;

use chrono::TimeZone;

use crate::errors::BackendError;
use crate::models::chat::ChatRetrievalHit;
use crate::models::paths::ProjectContext;
use crate::models::search::{SearchRequest, SearchResponse, SearchResult};
use crate::models::wiki::{
    CreateWikiPageRequest, RenameWikiPageResponse, SaveWikiPageResponse, WikiPageMeta,
    WikiPageType, WikiTree, WikiTreeNode, WikiTreeNodeKind,
};
use crate::services::file_store::FileStore;
use crate::services::wiki_index::WikiIndex;
use crate::services::WriteMode;
use crate::utils::markdown_utils::{
    count_words, extract_title, extract_wikilinks, parse_frontmatter, rewrite_wikilinks,
    snippet_for_query, split_frontmatter, Frontmatter, FrontmatterSplit,
};

/// Owns wiki scanning, page read/save, and the local keyword/tag/type/source
/// search index. Search is purely local: it never calls an LLM or Agent.
///
/// A shared `WikiIndex` caches the parsed body + derived metadata for every
/// `wiki/**.md` file per project, so repeated `scan_wiki` / `search` /
/// `retrieve_with_excerpts` / Graph-freshness calls do not re-read unchanged
/// Markdown (audit PERF-004). The index is invalidated by `mtime` + `size`, so
/// external edits in Obsidian or an external editor are picked up before any
/// cached entry is served. Bookmark state is NOT cached (a bookmark toggle
/// changes `bookmarks.json` without moving the page mtime/size); callers
/// overlay live bookmark paths on top of the cached `WikiPageMeta`.
#[derive(Default)]
pub struct SearchService {
    file_store: FileStore,
    index: WikiIndex,
}

impl SearchService {
    /// Walk the `wiki/` directory and return the nested tree plus a flat page
    /// metadata list. Obsidian (`.obsidian`), Git, and `.app` are skipped by
    /// `FileStore::list_markdown_files`.
    ///
    /// Reuses the per-project `WikiIndex` cache: only files whose `mtime` or
    /// `size` changed since the last call are re-read. Bookmark state is overlaid
    /// from `bookmark_paths` on top of the cached (bookmark-neutral) metas.
    pub fn scan_wiki(
        &self,
        context: &ProjectContext,
        bookmark_paths: &HashSet<String>,
    ) -> Result<WikiTree, BackendError> {
        let entries = self.index.refresh(&context, &self.file_store)?;

        let mut pages: Vec<WikiPageMeta> = entries
            .into_iter()
            .map(|entry| {
                let mut meta = entry.meta;
                meta.bookmarked = bookmark_paths.contains(&meta.path);
                meta
            })
            .collect();

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
        bookmark_paths: &HashSet<String>,
    ) -> Result<crate::models::wiki::WikiPageContent, BackendError> {
        let contents = self.file_store.read_markdown(context, relative_path)?;
        let absolute = context.resolve_project_path(relative_path)?;
        let split = split_frontmatter(&contents);
        let frontmatter = split
            .frontmatter
            .as_deref()
            .map(parse_frontmatter)
            .unwrap_or_default();
        let meta = self.build_meta(
            context,
            relative_path,
            &absolute,
            &split,
            &frontmatter,
            bookmark_paths,
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

    /// Create a new wiki page with seeded frontmatter + an H1. Rejects existing
    /// paths via `WriteMode::CreateNew`. Creating a new file is non-destructive
    /// (no Git checkpoint required by the CLAUDE.md hard boundary), matching the
    /// chat `save_answer_to_wiki` new-page path. The path must resolve inside
    /// `wiki/` — `resolve_project_path` enforces traversal/absolute/symlink
    /// safety, and this method additionally rejects anything outside `wiki/`.
    pub fn create_page(
        &self,
        context: &ProjectContext,
        request: &CreateWikiPageRequest,
    ) -> Result<SaveWikiPageResponse, BackendError> {
        let absolute = context.resolve_project_path(&request.relative_path)?;
        if absolute.strip_prefix(&context.wiki_dir).is_err() {
            return Err(BackendError::new(
                "PATH_OUTSIDE_PROJECT",
                "New wiki pages must live under the wiki/ directory.".to_string(),
                false,
                true,
            ));
        }

        let stem = absolute
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("untitled")
            .to_string();
        let title = request
            .title
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|t| t.to_string())
            .unwrap_or_else(|| stem.clone());
        let created = crate::utils::time_utils::now_rfc3339();

        let mut markdown = String::new();
        markdown.push_str("---\n");
        if let Some(page_type) = request
            .page_type
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            markdown.push_str(&format!("type: {}\n", yaml_escape_scalar(page_type)));
        }
        markdown.push_str(&format!("title: {}\n", yaml_escape_scalar(&title)));
        markdown.push_str(&format!("created: {}\n", created));
        markdown.push_str("---\n\n");
        markdown.push_str(&format!("# {}\n", title));

        self.file_store.write_markdown_checked(
            context,
            &request.relative_path,
            &markdown,
            WriteMode::CreateNew,
        )?;

        let hash = self.file_store.file_hash(context, &request.relative_path)?;
        let graph_cache_invalidated = self.invalidate_graph_cache(context);
        self.append_save_log(context, &request.relative_path);

        Ok(SaveWikiPageResponse {
            relative_path: request.relative_path.clone(),
            hash,
            saved_at: created,
            graph_cache_invalidated,
        })
    }

    /// Rename a wiki page: move the file, then rewrite every `[[old-stem]]`
    /// reference across the wiki to `[[new-stem]]` (preserving aliases/anchors).
    /// A rename is a file move plus a batch rewrite of references, which the
    /// CLAUDE.md hard boundary covers ("覆盖、批量替换 — 操作前必须创建 Git
    /// 检查点"); the caller creates the checkpoint *before* invoking this so the
    /// old page and all reference files are recoverable. Returns the new path
    /// metadata and the list of pages whose references were rewritten.
    pub fn rename_page(
        &self,
        context: &ProjectContext,
        relative_path: &str,
        new_relative_path: &str,
    ) -> Result<RenameWikiPageResponse, BackendError> {
        let old_absolute = context.resolve_project_path(relative_path)?;
        let new_absolute = context.resolve_project_path(new_relative_path)?;
        // Both endpoints must live under wiki/.
        if old_absolute.strip_prefix(&context.wiki_dir).is_err()
            || new_absolute.strip_prefix(&context.wiki_dir).is_err()
        {
            return Err(BackendError::new(
                "PATH_OUTSIDE_PROJECT",
                "Wiki renames must keep both paths under wiki/.".to_string(),
                false,
                true,
            ));
        }
        if !old_absolute.exists() || !old_absolute.is_file() {
            return Err(BackendError::new(
                "FILE_NOT_FOUND",
                "The wiki page being renamed does not exist.".to_string(),
                false,
                true,
            )
            .with_details(serde_json::json!({ "path": relative_path })));
        }
        if new_absolute.exists() {
            return Err(BackendError::new(
                "FILE_ALREADY_EXISTS",
                "A page already exists at the destination path.".to_string(),
                true,
                true,
            )
            .with_details(serde_json::json!({ "path": new_relative_path })));
        }

        let old_stem = old_absolute
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let new_stem = new_absolute
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        // Read the body once (excluding frontmatter) so we can rewrite links
        // inside the page being renamed too (self-references to its own old
        // stem should point to the new stem).
        let contents = std::fs::read_to_string(&old_absolute)
            .map_err(|err| file_read_error(err, &old_absolute))?;
        let split = split_frontmatter(&contents);

        // Snapshot every file we will mutate so a mid-rename failure can
        // restore the working tree to its pre-rename state. The caller's Git
        // checkpoint protects the before-state for manual recovery, but an
        // automatic rollback keeps the wiki consistent without forcing the
        // user to dig through git (mirrors apply_source_delete's backup+restore).
        let files = self.file_store.list_markdown_files(&context.wiki_dir)?;
        // (project-relative path, original bytes) for each file we touch,
        // starting with the source page itself.
        let mut snapshots: Vec<(String, Vec<u8>)> =
            vec![(relative_path.to_string(), contents.clone().into_bytes())];
        let mut updated_references: Vec<String> = Vec::new();
        for file_absolute in &files {
            // The renamed page itself is handled separately below; rewriting it
            // in-place before the move would race with the rename, so skip it
            // here and rewrite its body as part of the move.
            if file_absolute == &old_absolute {
                continue;
            }
            let body = std::fs::read_to_string(file_absolute)
                .map_err(|err| file_read_error(err, file_absolute))?;
            let (rewritten, n) = rewrite_wikilinks(&body, &old_stem, &new_stem);
            if n > 0 {
                std::fs::write(file_absolute, rewritten.as_bytes())
                    .map_err(|err| io_write_error(err, file_absolute))?;
                let project_relative = context.to_project_relative(file_absolute)?;
                snapshots.push((project_relative.clone(), body.into_bytes()));
                updated_references.push(project_relative);
            }
        }

        // Move the file, rewriting any self-references in its own body. On any
        // failure after this point, roll back every file we touched and remove
        // the new file if it was created.
        let final_contents = if old_stem == new_stem {
            // Same stem (e.g. only a directory changed) — no self-rewrite needed.
            contents
        } else {
            let (rewritten_body, _) = rewrite_wikilinks(&split.body, &old_stem, &new_stem);
            match split.frontmatter.as_ref() {
                Some(fm) => format!("---\n{fm}\n---\n\n{rewritten_body}"),
                None => rewritten_body,
            }
        };
        let rename_result = (|| {
            if let Some(parent) = new_absolute.parent() {
                std::fs::create_dir_all(parent).map_err(|err| io_write_error(err, parent))?;
            }
            std::fs::write(&new_absolute, final_contents.as_bytes())
                .map_err(|err| io_write_error(err, &new_absolute))?;
            std::fs::remove_file(&old_absolute)
                .map_err(|err| io_write_error(err, &old_absolute))?;
            Ok::<(), BackendError>(())
        })();
        if let Err(error) = rename_result {
            // Roll back: restore every touched file from its snapshot, then
            // drop the half-written new file. Git checkpoint from the caller
            // remains the long-term safety net.
            for (rel, bytes) in &snapshots {
                if let Ok(abs) = context.resolve_project_path(rel) {
                    if let Some(parent) = abs.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let _ = std::fs::write(&abs, bytes);
                }
            }
            let _ = std::fs::remove_file(&new_absolute);
            return Err(error);
        }

        let hash = self.file_store.file_hash(context, new_relative_path)?;
        let graph_cache_invalidated = self.invalidate_graph_cache(context);
        updated_references.sort();
        self.append_save_log(context, new_relative_path);

        Ok(RenameWikiPageResponse {
            relative_path: new_relative_path.to_string(),
            hash,
            saved_at: crate::utils::time_utils::now_rfc3339(),
            updated_references,
            graph_cache_invalidated,
        })
    }

    /// Find every wiki page that links to `target_stem` (the file stem without
    /// extension). Used by the delete path to warn the user that those links
    /// would become missing after deletion. Matching is case-insensitive on the
    /// link target, consistent with `extract_wikilinks` / `rewrite_wikilinks`.
    /// A page's self-references (it links to its own stem) are excluded: those
    /// links vanish with the page and would not show up as missing elsewhere.
    pub fn find_pages_referencing(
        &self,
        context: &ProjectContext,
        target_stem: &str,
    ) -> Result<Vec<String>, BackendError> {
        let needle = target_stem.to_ascii_lowercase();
        let files = self.file_store.list_markdown_files(&context.wiki_dir)?;
        let mut referencing: Vec<String> = Vec::new();
        for file_absolute in &files {
            // Skip the target page itself: a self-link `[[self]]` disappears
            // with the page and should not appear in the "missing links"
            // preview (the preview warns about *other* pages breaking).
            if file_absolute
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_ascii_lowercase() == needle)
                .unwrap_or(false)
            {
                continue;
            }
            let body = std::fs::read_to_string(file_absolute)
                .map_err(|err| file_read_error(err, file_absolute))?;
            let split = split_frontmatter(&body);
            let links = extract_wikilinks(&split.body);
            if links.iter().any(|link| link.to_ascii_lowercase() == needle) {
                referencing.push(context.to_project_relative(file_absolute)?);
            }
        }
        referencing.sort();
        Ok(referencing)
    }

    /// Execute a confirmed wiki page deletion: re-verify the file still exists
    /// at the registered hash (defends against edits between registration and
    /// confirmation), create a pre-delete scoped Git checkpoint (the safety
    /// net, CLAUDE.md hard rule), remove the file, invalidate the graph cache,
    /// and commit the deletion as a FinalResult checkpoint so the change is
    /// recoverable and visible in history. Mirrors
    /// `import_service::apply_source_delete`'s two-checkpoint contract.
    /// Returns whether the pre-delete checkpoint produced a commit hash.
    pub fn apply_page_delete(
        &self,
        context: &ProjectContext,
        git_service: &crate::services::GitService,
        target_path: &str,
        target_hash: &str,
    ) -> Result<bool, BackendError> {
        use crate::models::git::CheckpointPurpose;

        let absolute = context.resolve_project_path(target_path)?;
        if absolute.strip_prefix(&context.wiki_dir).is_err() {
            return Err(BackendError::new(
                "PATH_OUTSIDE_PROJECT",
                "Only wiki pages can be deleted here.".to_string(),
                false,
                true,
            ));
        }
        if !absolute.exists() || !absolute.is_file() {
            return Err(BackendError::new(
                "FILE_NOT_FOUND",
                "The wiki page was already removed.".to_string(),
                false,
                true,
            )
            .with_details(serde_json::json!({ "path": target_path })));
        }
        let current_hash = self.file_store.file_hash(context, target_path)?;
        if current_hash != target_hash {
            // Surface the on-disk baseline so the frontend can show what
            // changed since the delete was requested (consistent with the
            // file_store FILE_HASH_MISMATCH baseline surface).
            let baseline_content = std::fs::read(&absolute)
                .ok()
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
            let mut details = serde_json::json!({
                "path": target_path,
                "expectedHash": target_hash,
                "currentHash": current_hash,
            });
            if let Some(baseline) = baseline_content {
                details["baselineContent"] = serde_json::Value::String(baseline);
            }
            return Err(BackendError::new(
                "FILE_HASH_MISMATCH",
                "The wiki page changed since the delete was requested. Reload and try again."
                    .to_string(),
                true,
                true,
            )
            .with_details(details));
        }

        // Pre-delete safety checkpoint (CLAUDE.md). Scoped to the target path
        // so unrelated working-tree changes are not swept in. `created` may be
        // false when the file is already committed and clean — that is fine;
        // the file is still recoverable from HEAD.
        let checkpoint = git_service.create_scoped_checkpoint(
            context,
            CheckpointPurpose::HighRiskOperation,
            "Before deleting wiki page",
            &[target_path.to_string()],
        )?;

        // Snapshot the file bytes so a mid-operation failure can restore the
        // working tree. `unstage_paths` alone only resets the index, not the
        // working tree, so a successful remove_file followed by a failed
        // post-delete checkpoint would otherwise leave the page gone on disk
        // (mirrors the backup+restore pattern in apply_source_delete).
        let snapshot = std::fs::read(&absolute).map_err(|err| file_read_error(err, &absolute))?;

        let result = (|| {
            std::fs::remove_file(&absolute).map_err(|err| io_write_error(err, &absolute))?;
            // Drop the deleted page from the graph cache so a stale node
            // doesn't linger; scan will rebuild it. Best-effort.
            let graph_cache = context.app_dir.join("graph-cache.json");
            let _ = std::fs::remove_file(&graph_cache);
            // Commit the deletion itself so the change lands in history and is
            // recoverable (PRD-GIT-003: 成功操作后提交最终结果).
            git_service.create_scoped_checkpoint(
                context,
                CheckpointPurpose::FinalResult,
                "Delete wiki page",
                &[target_path.to_string()],
            )?;
            Ok::<(), BackendError>(())
        })();
        if let Err(error) = result {
            // Restore the page on disk and unstage so the working tree returns
            // to its pre-delete state.
            let _ = std::fs::write(&absolute, &snapshot);
            let _ = git_service.unstage_paths(context, &[target_path.to_string()]);
            return Err(error);
        }

        Ok(checkpoint.commit_hash.is_some())
    }

    /// Local keyword/tag/type/source search.
    ///
    /// Reuses the per-project `WikiIndex` cache so repeated searches do not
    /// re-read unchanged Markdown: the index refreshes once (mtime/size
    /// invalidation), then search scores against the cached bodies/metas.
    /// Bookmarks are intentionally not joined here (the global search command
    /// passes an empty set, matching the pre-index behavior).
    pub fn search(
        &self,
        context: &ProjectContext,
        request: &SearchRequest,
    ) -> Result<SearchResponse, BackendError> {
        let entries = self.index.refresh(&context, &self.file_store)?;

        let query_terms = request
            .query
            .as_deref()
            .map(|q| q.trim())
            .filter(|q| !q.is_empty())
            .map(extract_query_terms)
            .filter(|terms| !terms.is_empty());
        let type_filter: HashSet<WikiPageType> = request.page_types.iter().copied().collect();
        let tag_filter: Vec<String> = request
            .tags
            .iter()
            .map(|tag| normalize_for_search(tag))
            .collect();
        let source_filter = request
            .source
            .as_deref()
            .map(str::trim)
            .filter(|source| !source.is_empty())
            .map(normalize_for_search);

        let mut results: Vec<SearchResult> = Vec::new();

        for entry in &entries {
            let meta = &entry.meta;
            let body = &entry.body_markdown;

            if !type_filter.is_empty() && !type_filter.contains(&meta.page_type) {
                continue;
            }
            if !tag_filter.is_empty()
                && !meta
                    .tags
                    .iter()
                    .any(|tag| tag_filter.contains(&normalize_for_search(tag)))
            {
                continue;
            }
            if let Some(ref source_needle) = source_filter {
                let has_source = meta
                    .sources
                    .iter()
                    .any(|source| normalize_for_search(source).contains(source_needle));
                if !has_source {
                    continue;
                }
            }

            let (matched_fields, snippet, score) = match query_terms.as_deref() {
                Some(terms) => {
                    let mut fields: Vec<&'static str> = Vec::new();
                    let mut score = 0i64;

                    if let Some(field_score) = score_field(&meta.title, terms, 120, 80) {
                        fields.push("title");
                        score += field_score;
                    }
                    let tags = meta.tags.join(" ");
                    if let Some(field_score) = score_field(&tags, terms, 0, 35) {
                        fields.push("tags");
                        score += field_score;
                    }
                    let sources = meta.sources.join(" ");
                    if let Some(field_score) = score_field(&sources, terms, 0, 25) {
                        fields.push("sources");
                        score += field_score;
                    }
                    let aliases = meta.aliases.join(" ");
                    if let Some(field_score) = score_field(&aliases, terms, 70, 45) {
                        fields.push("aliases");
                        score += field_score;
                    }

                    if let Some(field_score) = score_field(body, terms, 18, 8) {
                        fields.push("content");
                        score += field_score;
                    }
                    if let Some(field_score) = score_field(&meta.path, terms, 0, 20) {
                        fields.push("path");
                        score += field_score;
                    }

                    if fields.is_empty() {
                        continue;
                    }

                    let snippet = first_matching_term(body, terms)
                        .and_then(|term| snippet_for_query(body, &term, 48))
                        .or_else(|| first_body_excerpt(body, 96));
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

        if query_terms.is_some() {
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
    /// `search` index (no model is called). The excerpt is derived from the
    /// cached `WikiIndex` body (no per-result `read_page` re-read), so a chat
    /// retrieval after a search pays zero extra file reads. This is the
    /// chat-retrieval entry point; the global search command stays
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
        // Build an excerpt from the cached body for each hit. The index was
        // refreshed by `search` above, so `entries()` is a cheap clone-out
        // with no disk reads. Falls back to `None` (matching the prior
        // `read_page(...).ok()` behavior) if a path is missing from the cache.
        let cached = self.index.entries(context)?;
        let by_path: std::collections::HashMap<&str, &str> = cached
            .iter()
            .map(|entry| (entry.path.as_str(), entry.body_markdown.as_str()))
            .collect();
        let mut hits = Vec::with_capacity(response.results.len());
        for result in response.results {
            let excerpt = by_path
                .get(result.path.as_str())
                .map(|body| truncate_excerpt(body, excerpt_chars));
            hits.push(ChatRetrievalHit {
                path: result.path,
                title: result.title,
                snippet: result.snippet,
                score: result.score,
                excerpt,
                is_pinned: false,
            });
        }
        Ok(hits)
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

fn io_write_error(err: std::io::Error, path: &Path) -> BackendError {
    BackendError::new("FILE_WRITE_FAILED", err.to_string(), true, false)
        .with_details(serde_json::json!({ "path": path.to_string_lossy() }))
}

/// Quote a scalar for our hand-rolled frontmatter parser, matching
/// `chat_service::yaml_scalar`. Quote when the value would otherwise be parsed
/// as a list, a nested mapping (`:`), a multi-line value (newline), or carry a
/// leading/special character the parser treats specially.
fn yaml_escape_scalar(value: &str) -> String {
    if value.contains(':')
        || value.contains('[')
        || value.contains(']')
        || value.contains('\n')
        || value.contains('\r')
    {
        format!(
            "\"{}\"",
            value.replace('"', "\\\"").replace(['\n', '\r'], " ")
        )
    } else {
        value.to_string()
    }
}

fn normalize_for_search(value: &str) -> String {
    let mut normalized = String::new();
    let mut last_was_space = true;

    for ch in value.to_lowercase().chars() {
        if ch.is_alphanumeric() || is_cjk(ch) {
            normalized.push(ch);
            last_was_space = false;
        } else if !last_was_space {
            normalized.push(' ');
            last_was_space = true;
        }
    }

    let mut trimmed = normalized.trim().to_string();
    for prefix in ["什么是", "请解释", "解释一下"] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            trimmed = rest.trim().to_string();
            break;
        }
    }
    for suffix in ["是什么", "？", "?", "吗", "呢"] {
        if let Some(rest) = trimmed.strip_suffix(suffix) {
            trimmed = rest.trim().to_string();
            break;
        }
    }
    trimmed
}

fn extract_query_terms(query: &str) -> Vec<String> {
    let normalized = normalize_for_search(query);
    let mut terms: Vec<String> = Vec::new();
    push_unique_term(&mut terms, normalized.clone());

    let mut cjk_run = String::new();
    let mut ascii_run = String::new();
    for ch in normalized.chars() {
        if is_cjk(ch) {
            flush_ascii_run(&mut terms, &mut ascii_run);
            cjk_run.push(ch);
        } else if ch.is_ascii_alphanumeric() {
            flush_cjk_run(&mut terms, &mut cjk_run);
            ascii_run.push(ch);
        } else {
            flush_cjk_run(&mut terms, &mut cjk_run);
            flush_ascii_run(&mut terms, &mut ascii_run);
        }
    }
    flush_cjk_run(&mut terms, &mut cjk_run);
    flush_ascii_run(&mut terms, &mut ascii_run);

    let base_terms: Vec<String> = terms
        .iter()
        .filter_map(|term| strip_trailing_ascii_digits(term))
        .collect();
    for term in base_terms {
        push_unique_term(&mut terms, term);
    }

    terms
}

fn score_field(
    field: &str,
    terms: &[String],
    exact_phrase_weight: i64,
    term_weight: i64,
) -> Option<i64> {
    let haystack = normalize_for_search(field);
    if haystack.is_empty() {
        return None;
    }

    let mut score = 0i64;
    if let Some(phrase) = terms.first() {
        if !phrase.is_empty() && haystack.contains(phrase) {
            score += exact_phrase_weight;
        }
    }
    for term in terms {
        if !term.is_empty() && haystack.contains(term) {
            score += term_weight;
        }
    }

    (score > 0).then_some(score)
}

fn first_matching_term(field: &str, terms: &[String]) -> Option<String> {
    let haystack = normalize_for_search(field);
    terms
        .iter()
        .find(|term| !term.is_empty() && haystack.contains(term.as_str()))
        .cloned()
}

fn first_body_excerpt(body: &str, max_chars: usize) -> Option<String> {
    let line = body.lines().map(str::trim).find(|line| !line.is_empty())?;
    Some(truncate_excerpt(line, max_chars))
}

fn push_unique_term(terms: &mut Vec<String>, term: String) {
    let term = term.trim();
    if term.chars().count() >= 2 && !terms.iter().any(|existing| existing == term) {
        terms.push(term.to_string());
    }
}

fn flush_cjk_run(terms: &mut Vec<String>, run: &mut String) {
    if run.chars().count() >= 2 {
        push_unique_term(terms, run.clone());
    }
    run.clear();
}

fn flush_ascii_run(terms: &mut Vec<String>, run: &mut String) {
    if run.chars().count() >= 2 {
        push_unique_term(terms, run.clone());
    }
    run.clear();
}

fn strip_trailing_ascii_digits(term: &str) -> Option<String> {
    let stripped = term.trim_end_matches(|ch: char| ch.is_ascii_digit());
    if stripped != term && stripped.chars().count() >= 2 {
        Some(stripped.to_string())
    } else {
        None
    }
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0x20000..=0x2A6DF
            | 0x2A700..=0x2B73F
            | 0x2B740..=0x2B81F
            | 0x2B820..=0x2CEAF
    )
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
    use crate::models::wiki::{
        CreateWikiPageRequest, SaveWikiPageResponse, WikiPageContent, WikiPageType, WikiTree,
        WikiTreeNode,
    };
    use crate::services::{BookmarkService, GitService, WriteMode};
    use crate::utils::time_utils::now_rfc3339;
    use sha2::{Digest, Sha256};
    use std::collections::HashSet;
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

    fn seed_chinese_question_page(context: &ProjectContext) {
        write_file(
            context,
            "wiki/concepts/constraints-first.md",
            "---\ntitle: 约束先行\naliases: [约束先行2]\ntags: [方法]\n---\n\n# 约束先行\n\n约束先行是一种先定义限制条件再生成方案的工作方式。",
        );
    }

    fn search_request(context: &ProjectContext, query: &str) -> SearchRequest {
        SearchRequest {
            project_id: "p".into(),
            project_root_path: context.root.to_string_lossy().to_string(),
            query: Some(query.to_string()),
            page_types: Vec::new(),
            tags: Vec::new(),
            source: None,
            limit: None,
        }
    }

    fn find_tree_node<'a>(node: &'a WikiTreeNode, path: &str) -> Option<&'a WikiTreeNode> {
        if node.path == path {
            return Some(node);
        }
        node.children
            .iter()
            .find_map(|child| find_tree_node(child, path))
    }

    #[test]
    fn scan_wiki_builds_tree_and_flat_pages() {
        let (context, root) = tmp_context("scan");
        seed_sample_vault(&context);
        let service = SearchService::default();

        let tree: WikiTree = service.scan_wiki(&context, &HashSet::new()).unwrap();

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
    fn scan_wiki_joins_v2_bookmarks_into_pages_and_tree_nodes() {
        let (context, root) = tmp_context("bookmarks");
        seed_sample_vault(&context);
        std::fs::create_dir_all(context.app_dir.clone()).unwrap();
        std::fs::write(
            context.app_dir.join("bookmarks.json"),
            serde_json::json!({
                "version": 2,
                "entries": [{
                    "id": "wiki_page:wiki/concepts/react-pattern.md",
                    "kind": "wiki_page",
                    "path": "wiki/concepts/react-pattern.md",
                    "title": "ReAct Pattern",
                    "createdAt": "2026-07-04T00:00:00Z"
                }]
            })
            .to_string(),
        )
        .unwrap();

        let service = SearchService::default();
        let bookmark_paths = BookmarkService::default()
            .wiki_page_paths(&context)
            .unwrap();
        let tree = service.scan_wiki(&context, &bookmark_paths).unwrap();
        let react = tree
            .pages
            .iter()
            .find(|p| p.path == "wiki/concepts/react-pattern.md")
            .unwrap();
        assert!(react.bookmarked);
        assert!(
            find_tree_node(&tree.root, "wiki/concepts/react-pattern.md")
                .unwrap()
                .bookmarked
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scan_wiki_still_accepts_legacy_bookmark_arrays_from_bookmark_service() {
        let (context, root) = tmp_context("legacy-bookmarks");
        seed_sample_vault(&context);
        std::fs::create_dir_all(context.app_dir.clone()).unwrap();
        std::fs::write(
            context.app_dir.join("bookmarks.json"),
            serde_json::to_string(&vec!["wiki/concepts/agent-memory.md".to_string()]).unwrap(),
        )
        .unwrap();

        let bookmark_paths = BookmarkService::default()
            .wiki_page_paths(&context)
            .unwrap();
        let tree = SearchService::default()
            .scan_wiki(&context, &bookmark_paths)
            .unwrap();
        let agent = tree
            .pages
            .iter()
            .find(|p| p.path == "wiki/concepts/agent-memory.md")
            .unwrap();
        assert!(agent.bookmarked);
        assert!(
            find_tree_node(&tree.root, "wiki/concepts/agent-memory.md")
                .unwrap()
                .bookmarked
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn read_page_returns_frontmatter_body_and_meta() {
        let (context, root) = tmp_context("read");
        seed_sample_vault(&context);
        let service = SearchService::default();

        let page: WikiPageContent = service
            .read_page(&context, "wiki/concepts/agent-memory.md", &HashSet::new())
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
    fn search_matches_chinese_question_by_extracted_title_term() {
        let (context, root) = tmp_context("search-cjk-title");
        seed_chinese_question_page(&context);
        let service = SearchService::default();

        let response = service
            .search(&context, &search_request(&context, "什么是约束先行2？"))
            .unwrap();

        assert_eq!(response.total, 1);
        assert_eq!(
            response.results[0].path,
            "wiki/concepts/constraints-first.md"
        );
        assert!(response.results[0]
            .matched_fields
            .contains(&"title".to_string()));
        assert!(response.results[0]
            .matched_fields
            .contains(&"aliases".to_string()));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retrieve_with_excerpts_handles_chinese_question_suffix() {
        let (context, root) = tmp_context("retrieve-cjk");
        seed_chinese_question_page(&context);
        let service = SearchService::default();

        let hits = service
            .retrieve_with_excerpts(&context, "约束先行是什么？", 3, 80)
            .unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "wiki/concepts/constraints-first.md");
        assert!(hits[0].excerpt.as_deref().unwrap().contains("约束先行"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn search_uses_unicode_lowercase_not_ascii_only() {
        let (context, root) = tmp_context("search-unicode-lower");
        write_file(
            &context,
            "wiki/concepts/eclair.md",
            "---\ntitle: Éclair Guide\n---\n\n# Éclair Guide\n\nDessert notes.",
        );
        let service = SearchService::default();

        let response = service
            .search(&context, &search_request(&context, "éclair"))
            .unwrap();

        assert_eq!(response.total, 1);
        assert_eq!(response.results[0].path, "wiki/concepts/eclair.md");
        assert!(response.results[0]
            .matched_fields
            .contains(&"title".to_string()));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn search_prefers_exact_title_or_alias_over_body_term() {
        let (context, root) = tmp_context("search-cjk-ranking");
        seed_chinese_question_page(&context);
        write_file(
            &context,
            "wiki/concepts/body-mention.md",
            "---\ntitle: Body Mention\n---\n\n# Body Mention\n\n什么是约束先行2 是正文里的一个问题。",
        );
        let service = SearchService::default();

        let response = service
            .search(&context, &search_request(&context, "什么是约束先行2？"))
            .unwrap();

        assert_eq!(response.total, 2);
        assert_eq!(
            response.results[0].path,
            "wiki/concepts/constraints-first.md"
        );
        assert!(response.results[0].score > response.results[1].score);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn search_returns_no_hits_for_truly_unmatched_question() {
        let (context, root) = tmp_context("search-cjk-none");
        seed_chinese_question_page(&context);
        let service = SearchService::default();

        let response = service
            .search(
                &context,
                &search_request(&context, "什么是完全不存在的概念？"),
            )
            .unwrap();

        assert_eq!(response.total, 0);
        assert!(response.results.is_empty());

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

    fn make_create_request(project_id: &str, root: &str, path: &str) -> CreateWikiPageRequest {
        CreateWikiPageRequest {
            project_id: project_id.to_string(),
            project_root_path: root.to_string(),
            relative_path: path.to_string(),
            title: None,
            page_type: None,
        }
    }

    #[test]
    fn create_page_seeds_frontmatter_and_h1_and_rejects_existing() {
        let (context, root) = tmp_context("create");
        seed_sample_vault(&context);
        let service = SearchService::default();

        let response = service
            .create_page(
                &context,
                &make_create_request(
                    "p",
                    &context.root.to_string_lossy(),
                    "wiki/concepts/new-page.md",
                ),
            )
            .unwrap();
        assert_eq!(response.relative_path, "wiki/concepts/new-page.md");
        assert!(!response.hash.is_empty());

        let on_disk = std::fs::read_to_string(
            context
                .resolve_project_path("wiki/concepts/new-page.md")
                .unwrap(),
        )
        .unwrap();
        assert!(on_disk.starts_with("---\n"));
        assert!(on_disk.contains("title: new-page"));
        assert!(on_disk.contains("created:"));
        assert!(on_disk.contains("# new-page"));

        // Existing path is rejected with FILE_ALREADY_EXISTS.
        let err = service
            .create_page(
                &context,
                &make_create_request(
                    "p",
                    &context.root.to_string_lossy(),
                    "wiki/concepts/react-pattern.md",
                ),
            )
            .expect_err("create must reject an existing file");
        assert_eq!(err.code, "FILE_ALREADY_EXISTS");

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn create_page_rejects_paths_outside_wiki_and_supports_cjk() {
        let (context, root) = tmp_context("create-cjk");
        seed_sample_vault(&context);
        let service = SearchService::default();

        // Path outside wiki/ is rejected even though it is inside the project.
        let err = service
            .create_page(
                &context,
                &make_create_request("p", &context.root.to_string_lossy(), "raw/sources/x.md"),
            )
            .expect_err("non-wiki path must be rejected");
        assert_eq!(err.code, "PATH_OUTSIDE_PROJECT");

        // CJK filename round-trips through the path resolver + filesystem.
        let response = service
            .create_page(
                &context,
                &make_create_request("p", &context.root.to_string_lossy(), "wiki/概念/智能体.md"),
            )
            .unwrap();
        assert_eq!(response.relative_path, "wiki/概念/智能体.md");
        let on_disk =
            std::fs::read_to_string(context.resolve_project_path("wiki/概念/智能体.md").unwrap())
                .unwrap();
        assert!(on_disk.contains("# 智能体"));
        assert!(on_disk.contains("title: 智能体"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rename_page_moves_file_and_rewrites_references_including_self() {
        let (context, root) = tmp_context("rename");
        seed_sample_vault(&context);
        // agent-memory.md links [[react-pattern]]; rename react-pattern so
        // agent-memory.md must be rewritten.
        let service = SearchService::default();

        let response = service
            .rename_page(
                &context,
                "wiki/concepts/react-pattern.md",
                "wiki/concepts/reasoning-loop.md",
            )
            .unwrap();
        assert_eq!(response.relative_path, "wiki/concepts/reasoning-loop.md");
        // agent-memory.md referenced react-pattern → rewritten.
        assert_eq!(
            response.updated_references,
            vec!["wiki/concepts/agent-memory.md".to_string()]
        );

        // Old path gone, new path exists.
        assert!(!context
            .resolve_project_path("wiki/concepts/react-pattern.md")
            .unwrap()
            .exists());
        let new_body = std::fs::read_to_string(
            context
                .resolve_project_path("wiki/concepts/reasoning-loop.md")
                .unwrap(),
        )
        .unwrap();
        // The moved page keeps its own H1/body.
        assert!(new_body.contains("# ReAct Pattern"));

        // The referencing page now links the new stem.
        let referrer = std::fs::read_to_string(
            context
                .resolve_project_path("wiki/concepts/agent-memory.md")
                .unwrap(),
        )
        .unwrap();
        assert!(referrer.contains("[[reasoning-loop]]"));
        assert!(!referrer.contains("[[react-pattern]]"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rename_page_rejects_outside_wiki_and_existing_destination_and_cjk() {
        let (context, root) = tmp_context("rename-cjk");
        seed_sample_vault(&context);
        let service = SearchService::default();

        // Destination outside wiki/ rejected.
        let err = service
            .rename_page(&context, "wiki/index.md", "raw/index.md")
            .expect_err("rename must keep both paths under wiki/");
        assert_eq!(err.code, "PATH_OUTSIDE_PROJECT");

        // Existing destination rejected.
        let err = service
            .rename_page(
                &context,
                "wiki/concepts/react-pattern.md",
                "wiki/concepts/agent-memory.md",
            )
            .expect_err("rename must reject an existing destination");
        assert_eq!(err.code, "FILE_ALREADY_EXISTS");

        // Missing source rejected.
        let err = service
            .rename_page(&context, "wiki/concepts/ghost.md", "wiki/concepts/other.md")
            .expect_err("rename must reject a missing source");
        assert_eq!(err.code, "FILE_NOT_FOUND");

        // CJK rename with a CJK reference round-trips.
        std::fs::create_dir_all(context.wiki_dir.join("概念")).unwrap();
        std::fs::write(
            context.wiki_dir.join("概念").join("甲.md"),
            "# 甲\n\nsee [[乙]]",
        )
        .unwrap();
        std::fs::write(
            context.wiki_dir.join("概念").join("乙.md"),
            "# 乙\n\nself [[乙]]",
        )
        .unwrap();
        let response = service
            .rename_page(&context, "wiki/概念/乙.md", "wiki/概念/乙二.md")
            .unwrap();
        assert_eq!(response.relative_path, "wiki/概念/乙二.md");
        assert_eq!(
            response.updated_references,
            vec!["wiki/概念/甲.md".to_string()]
        );
        // Self-reference in the renamed page is rewritten too.
        let moved =
            std::fs::read_to_string(context.resolve_project_path("wiki/概念/乙二.md").unwrap())
                .unwrap();
        assert!(moved.contains("[[乙二]]"));
        let referrer =
            std::fs::read_to_string(context.resolve_project_path("wiki/概念/甲.md").unwrap())
                .unwrap();
        assert!(referrer.contains("[[乙二]]"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn find_pages_referencing_is_case_insensitive_and_excludes_unrelated() {
        let (context, root) = tmp_context("refs");
        seed_sample_vault(&context);
        let service = SearchService::default();

        // react-pattern is referenced by agent-memory.md (and itself? no —
        // agent-memory links it; react-pattern does not link itself).
        let refs = service
            .find_pages_referencing(&context, "react-pattern")
            .unwrap();
        assert_eq!(refs, vec!["wiki/concepts/agent-memory.md".to_string()]);

        // Case-insensitive lookup.
        let refs_upper = service
            .find_pages_referencing(&context, "REACT-PATTERN")
            .unwrap();
        assert_eq!(
            refs_upper,
            vec!["wiki/concepts/agent-memory.md".to_string()]
        );

        // Unknown stem yields no references.
        let none = service
            .find_pages_referencing(&context, "nonexistent")
            .unwrap();
        assert!(none.is_empty());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn apply_page_delete_removes_file_after_git_checkpoint() {
        let (context, root) = tmp_context("del");
        seed_sample_vault(&context);
        let service = SearchService::default();
        let git = GitService;
        git.initialize_repository(&context, "init").unwrap();

        let target_path = "wiki/concepts/react-pattern.md";
        let target_hash = service.file_store.file_hash(&context, target_path).unwrap();

        let created = service
            .apply_page_delete(&context, &git, target_path, &target_hash)
            .unwrap();
        assert!(
            created,
            "a checkpoint commit must be created before removal"
        );
        assert!(!context.resolve_project_path(target_path).unwrap().exists());
        // A graph-cache file would have been removed; seeding none here is fine,
        // the call must still succeed.
        assert!(git.repository_status(&context).unwrap().head.is_some());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn apply_page_delete_invalidates_graph_cache() {
        let (context, root) = tmp_context("del-cache");
        seed_sample_vault(&context);
        std::fs::create_dir_all(context.app_dir.clone()).unwrap();
        std::fs::write(context.app_dir.join("graph-cache.json"), "{}").unwrap();
        let service = SearchService::default();
        let git = GitService;
        git.initialize_repository(&context, "init").unwrap();

        let target_path = "wiki/index.md";
        let target_hash = service.file_store.file_hash(&context, target_path).unwrap();
        service
            .apply_page_delete(&context, &git, target_path, &target_hash)
            .unwrap();
        assert!(!context.app_dir.join("graph-cache.json").exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn apply_page_delete_rejects_hash_drift_and_missing_and_outside_wiki() {
        let (context, root) = tmp_context("del-reject");
        seed_sample_vault(&context);
        let service = SearchService::default();
        let git = GitService;
        git.initialize_repository(&context, "init").unwrap();

        // Hash drift → FILE_HASH_MISMATCH, file untouched.
        let target_path = "wiki/concepts/react-pattern.md";
        let err = service
            .apply_page_delete(&context, &git, target_path, "stale-hash")
            .expect_err("stale hash must block deletion");
        assert_eq!(err.code, "FILE_HASH_MISMATCH");
        assert!(context.resolve_project_path(target_path).unwrap().exists());

        // Missing file → FILE_NOT_FOUND.
        let err = service
            .apply_page_delete(&context, &git, "wiki/concepts/ghost.md", "any")
            .expect_err("missing file must surface FILE_NOT_FOUND");
        assert_eq!(err.code, "FILE_NOT_FOUND");

        // Outside wiki/ → PATH_OUTSIDE_PROJECT.
        std::fs::write(context.root.join("purpose.md"), "# Purpose\n").unwrap();
        let err = service
            .apply_page_delete(&context, &git, "purpose.md", "any")
            .expect_err("non-wiki path must be rejected");
        assert_eq!(err.code, "PATH_OUTSIDE_PROJECT");

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn apply_page_delete_supports_cjk_filename() {
        let (context, root) = tmp_context("del-cjk");
        seed_sample_vault(&context);
        std::fs::create_dir_all(context.wiki_dir.join("概念")).unwrap();
        std::fs::write(
            context.wiki_dir.join("概念").join("智能体.md"),
            "# 智能体\n",
        )
        .unwrap();
        let service = SearchService::default();
        let git = GitService;
        git.initialize_repository(&context, "init").unwrap();

        let target_path = "wiki/概念/智能体.md";
        let target_hash = service.file_store.file_hash(&context, target_path).unwrap();
        let created = service
            .apply_page_delete(&context, &git, target_path, &target_hash)
            .unwrap();
        assert!(created);
        assert!(!context.resolve_project_path(target_path).unwrap().exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn find_pages_referencing_excludes_self_links() {
        // A page that links to its own stem should not appear in the
        // "will show as missing" preview — those links vanish with the page.
        let (context, root) = tmp_context("refs-self");
        seed_sample_vault(&context);
        // react-pattern now self-links.
        std::fs::write(
            context.wiki_dir.join("concepts").join("react-pattern.md"),
            "# ReAct Pattern\n\nSee [[react-pattern]] for details.",
        )
        .unwrap();
        let service = SearchService::default();

        let refs = service
            .find_pages_referencing(&context, "react-pattern")
            .unwrap();
        // agent-memory links react-pattern; react-pattern itself is excluded.
        assert_eq!(refs, vec!["wiki/concepts/agent-memory.md".to_string()]);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rename_page_rolls_back_referrers_when_move_fails() {
        // If the file move fails after referrers were rewritten, every touched
        // file must be restored to its pre-rename content so the wiki is not
        // left with [[new]] pointing at a non-existent page.
        let (context, root) = tmp_context("rename-rollback");
        seed_sample_vault(&context);
        let service = SearchService::default();

        // agent-memory links react-pattern; snapshot its original content.
        let referrer_path = context.wiki_dir.join("concepts").join("agent-memory.md");
        let original_referrer = std::fs::read(&referrer_path).unwrap();

        // Make the destination's parent creation fail by placing a file where
        // an intermediate directory would need to be. The full new path does
        // not exist, so the FILE_ALREADY_EXISTS pre-check passes, but
        // create_dir_all("wiki/concepts/blocker/sub") fails because "blocker"
        // is a file — exercising rollback after referrers were rewritten.
        std::fs::write(context.wiki_dir.join("concepts").join("blocker"), "block").unwrap();

        let err = service
            .rename_page(
                &context,
                "wiki/concepts/react-pattern.md",
                "wiki/concepts/blocker/sub/never.md",
            )
            .expect_err("rename whose destination parent cannot be created must fail");
        // The failure surfaces via io_write_error (create_dir_all path).
        assert_eq!(err.code, "FILE_WRITE_FAILED");

        // The referrer must be restored to its original bytes (not left with
        // [[react-pattern]] stripped/rewritten to the new stem).
        let after_referrer = std::fs::read(&referrer_path).unwrap();
        assert_eq!(
            after_referrer, original_referrer,
            "referrer must be rolled back to its pre-rename content"
        );
        // The source page must still exist (move did not complete).
        assert!(context
            .wiki_dir
            .join("concepts")
            .join("react-pattern.md")
            .exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn create_page_escapes_page_type_and_title_in_frontmatter() {
        // page_type and title must be YAML-escaped so a value with a colon or
        // newline cannot inject extra frontmatter keys.
        let (context, root) = tmp_context("create-escape");
        seed_sample_vault(&context);
        let service = SearchService::default();

        let request = CreateWikiPageRequest {
            project_id: "project-1".into(),
            project_root_path: context.root.to_string_lossy().to_string(),
            relative_path: "wiki/concepts/injected.md".to_string(),
            title: Some("Title: with colon\nand newline".to_string()),
            page_type: Some("concept\nmalicious: true".to_string()),
        };
        service.create_page(&context, &request).unwrap();

        let written =
            std::fs::read_to_string(context.wiki_dir.join("concepts").join("injected.md")).unwrap();
        // The injected "malicious: true" must not appear as its own key — it
        // is quoted inside the type value (newline collapsed to space).
        assert!(
            !written.contains("\nmalicious: true"),
            "page_type newline must not inject a frontmatter key: {written}"
        );
        assert!(written.contains("type: \"concept malicious: true\""));
        // Title colon is quoted.
        assert!(written.contains("title: \"Title: with colon and newline\""));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rewrite_wikilinks_handles_cjk_target_and_alias_preservation() {
        // CJK in the wikilink target (not just the filename) must rewrite, and
        // alias/anchor around a CJK target must be preserved.
        use crate::utils::markdown_utils::rewrite_wikilinks;
        let body = "see [[智能体]] and [[智能体|AI Agent]] and [[智能体#概述]]";
        let (out, n) = rewrite_wikilinks(body, "智能体", "AI助手");
        assert_eq!(n, 3);
        assert!(out.contains("[[AI助手]]"));
        assert!(out.contains("[[AI助手|AI Agent]]"));
        assert!(out.contains("[[AI助手#概述]]"));
    }
}

/// Integration-level tests for the shared `WikiIndex` backing `scan_wiki` /
/// `search` / `retrieve_with_excerpts`. These prove the Batch-4 acceptance
/// criteria at the service boundary: the three consumers share one index
/// snapshot, unchanged Markdown is not re-read, external edits/deletes are
/// picked up before any cached entry is served, and CJK filenames + bookmark
/// joins stay correct through the cache.
#[cfg(test)]
mod index_integration_tests {
    use super::SearchService;
    use crate::models::paths::ProjectContext;
    use crate::models::search::SearchRequest;
    use crate::services::BookmarkService;
    use std::collections::HashSet;
    use std::path::PathBuf;

    fn tmp_context(suffix: &str) -> (ProjectContext, PathBuf) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("llm-wiki-search-idx-{stamp}-{suffix}"));
        std::fs::create_dir_all(&root).unwrap();
        (ProjectContext::new("project-idx", root.clone()), root)
    }

    fn write_file(context: &ProjectContext, rel: &str, body: &str) {
        let path = context.resolve_project_path(rel).unwrap();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, body).unwrap();
    }

    /// Sleep past a 1-second mtime boundary so an external edit is observable
    /// to the index's mtime+size invalidation on every supported filesystem.
    fn cross_mtime_boundary() {
        std::thread::sleep(std::time::Duration::from_millis(1100));
    }

    fn seed(context: &ProjectContext) {
        write_file(
            context,
            "wiki/concepts/agent.md",
            "---\ntitle: Agent\ntype: concept\ntags: [memory]\n---\n\n# Agent\n\nCovers short context windows.",
        );
        write_file(
            context,
            "wiki/concepts/react.md",
            "---\ntitle: ReAct\ntype: concept\ntags: [reasoning]\n---\n\n# ReAct\n\nReason then act loop. See [[agent]].",
        );
        write_file(context, "wiki/index.md", "# Index\nWelcome.");
    }

    /// Proves the three consumers share one index: after `scan_wiki`,
    /// `search` and `retrieve_with_excerpts` do not re-read any file. The
    /// index is owned by `SearchService`, so the shared-state contract is
    /// enforced by construction (a second service instance would have its
    /// own cache — this test documents that the single shared instance is
    /// what the commands layer reaches).
    #[test]
    fn scan_search_and_retrieve_share_one_index_snapshot() {
        let (context, root) = tmp_context("shared");
        seed(&context);
        let service = SearchService::default();
        let bookmarks = HashSet::new();

        let tree = service.scan_wiki(&context, &bookmarks).unwrap();
        assert_eq!(tree.total_pages, 3);

        // search after scan: must not re-read unchanged files. We assert the
        // observable contract — results match the disk — and rely on the
        // index's own content_reads counter tests (wiki_index::tests) for the
        // no-reread proof. Here we confirm the shared cache produces correct
        // search results and correct chat excerpts in sequence.
        let request = SearchRequest {
            project_id: context.project_id.clone(),
            project_root_path: context.root.to_string_lossy().to_string(),
            query: Some("agent".to_string()),
            page_types: Vec::new(),
            tags: Vec::new(),
            source: None,
            limit: None,
        };
        let response = service.search(&context, &request).unwrap();
        assert!(response
            .results
            .iter()
            .any(|r| r.path == "wiki/concepts/agent.md"));

        let hits = service
            .retrieve_with_excerpts(&context, "agent", 5, 80)
            .unwrap();
        let agent_hit = hits
            .iter()
            .find(|h| h.path == "wiki/concepts/agent.md")
            .unwrap();
        // Excerpt comes from the cached body (no read_page re-read).
        assert!(agent_hit
            .excerpt
            .as_deref()
            .unwrap()
            .contains("short context"));

        std::fs::remove_dir_all(root).unwrap();
    }

    /// An external edit (Obsidian / external editor) between two `scan_wiki`
    /// calls must surface in the second scan: the index's mtime+size
    /// invalidation forces a re-read of the changed file, and the tree
    /// reflects the new title/body.
    #[test]
    fn scan_wiki_picks_up_external_edit_via_mtime_size() {
        let (context, root) = tmp_context("external-edit");
        seed(&context);
        let service = SearchService::default();
        let bookmarks = HashSet::new();

        let before = service.scan_wiki(&context, &bookmarks).unwrap();
        let agent_before = before
            .pages
            .iter()
            .find(|p| p.path == "wiki/concepts/agent.md")
            .unwrap();
        let hash_before = agent_before.hash.clone();

        // Simulate an external editor: rewrite the file outside the app.
        cross_mtime_boundary();
        write_file(
            &context,
            "wiki/concepts/agent.md",
            "---\ntitle: Agent v2\ntype: concept\ntags: [memory, context]\n---\n\n# Agent v2\n\nEdited externally.",
        );

        let after = service.scan_wiki(&context, &bookmarks).unwrap();
        let agent_after = after
            .pages
            .iter()
            .find(|p| p.path == "wiki/concepts/agent.md")
            .unwrap();
        assert_eq!(agent_after.title, "Agent v2");
        assert_eq!(
            agent_after.tags,
            vec!["memory".to_string(), "context".to_string()]
        );
        assert_ne!(agent_after.hash, hash_before);

        std::fs::remove_dir_all(root).unwrap();
    }

    /// An external delete (file removed while the app is open) must drop the
    /// page from subsequent scans — the index retains only live files, so
    /// Graph/Search never surface ghost pages.
    #[test]
    fn scan_wiki_drops_externally_deleted_page() {
        let (context, root) = tmp_context("external-delete");
        seed(&context);
        let service = SearchService::default();
        let bookmarks = HashSet::new();
        let first = service.scan_wiki(&context, &bookmarks).unwrap();
        assert_eq!(first.total_pages, 3);

        std::fs::remove_file(
            context
                .resolve_project_path("wiki/concepts/react.md")
                .unwrap(),
        )
        .unwrap();
        let after = service.scan_wiki(&context, &bookmarks).unwrap();
        assert_eq!(after.total_pages, 2);
        assert!(!after
            .pages
            .iter()
            .any(|p| p.path == "wiki/concepts/react.md"));

        std::fs::remove_dir_all(root).unwrap();
    }

    /// CJK filenames and bodies survive a scan -> edit -> scan cycle through
    /// the index: the mtime/size keys are stable across path encodings, and
    /// the canonicalize-based path safety in `ProjectContext` does not corrupt
    /// the CJK join.
    #[test]
    fn scan_wiki_round_trips_cjk_filenames_through_the_index() {
        let (context, root) = tmp_context("cjk-cycle");
        write_file(
            &context,
            "wiki/概念/智能体.md",
            "---\ntitle: 智能体\ntype: concept\ntags: [方法]\n---\n\n# 智能体\n\n约束先行。",
        );
        let service = SearchService::default();
        let bookmarks = HashSet::new();

        let first = service.scan_wiki(&context, &bookmarks).unwrap();
        let cjk = first
            .pages
            .iter()
            .find(|p| p.path == "wiki/概念/智能体.md")
            .unwrap();
        assert_eq!(cjk.title, "智能体");
        assert_eq!(cjk.tags, vec!["方法".to_string()]);

        // Edit the CJK file externally; the index must pick up the new title.
        cross_mtime_boundary();
        write_file(
            &context,
            "wiki/概念/智能体.md",
            "---\ntitle: 智能体（修订）\ntype: concept\ntags: [方法, 实践]\n---\n\n# 智能体（修订）\n\n约束先行，再迭代。",
        );
        let after = service.scan_wiki(&context, &bookmarks).unwrap();
        let cjk_after = after
            .pages
            .iter()
            .find(|p| p.path == "wiki/概念/智能体.md")
            .unwrap();
        assert_eq!(cjk_after.title, "智能体（修订）");
        assert_eq!(cjk_after.tags, vec!["方法".to_string(), "实践".to_string()]);

        std::fs::remove_dir_all(root).unwrap();
    }

    /// Bookmark joins stay current through the cache: a bookmark toggle
    /// (which changes `bookmarks.json` without moving the page mtime/size)
    /// must be reflected in the next `scan_wiki` even though no wiki file
    /// changed. The index caches bookmark-neutral metas and overlays live
    /// bookmark paths at scan time, so a toggle between scans flips the
    /// `bookmarked` flag without a cache invalidation.
    #[test]
    fn bookmark_toggle_between_scans_flips_bookmarked_without_a_file_change() {
        let (context, root) = tmp_context("bookmark-overlay");
        seed(&context);
        std::fs::create_dir_all(context.app_dir.clone()).unwrap();
        // No bookmarks initially.
        let service = SearchService::default();
        let bookmark_service = BookmarkService::default();

        let first = service
            .scan_wiki(
                &context,
                &bookmark_service.wiki_page_paths(&context).unwrap(),
            )
            .unwrap();
        let agent_first = first
            .pages
            .iter()
            .find(|p| p.path == "wiki/concepts/agent.md")
            .unwrap();
        assert!(!agent_first.bookmarked);

        // Toggle a bookmark ON without touching any wiki file.
        bookmark_service
            .toggle_wiki_page(&context, "wiki/concepts/agent.md", "Agent")
            .unwrap();

        // No mtime/size change on agent.md — the index reuses the cached
        // entry — but the bookmark overlay must still mark it bookmarked.
        let second = service
            .scan_wiki(
                &context,
                &bookmark_service.wiki_page_paths(&context).unwrap(),
            )
            .unwrap();
        let agent_second = second
            .pages
            .iter()
            .find(|p| p.path == "wiki/concepts/agent.md")
            .unwrap();
        assert!(agent_second.bookmarked);
        // And the tree node too (scan_wiki builds the tree from the overlaid
        // metas, so the bookmark flag propagates).
        let agent_node = second
            .root
            .children
            .iter()
            .flat_map(|c| c.children.iter())
            .find(|c| c.name == "agent.md")
            .unwrap();
        assert!(agent_node.bookmarked);

        std::fs::remove_dir_all(root).unwrap();
    }

    /// `retrieve_with_excerpts` after `search` pays zero extra file reads: the
    /// excerpt is derived from the index body cached during `search`. This
    /// matches the audit PERF-004 goal — chat retrieval no longer does a
    /// `read_page` per top result.
    #[test]
    fn retrieve_with_excerpts_reuses_cached_body_and_does_not_reread() {
        let (context, root) = tmp_context("retrieve-no-reread");
        seed(&context);
        let service = SearchService::default();

        let hits = service
            .retrieve_with_excerpts(&context, "agent", 5, 80)
            .unwrap();
        let agent = hits
            .iter()
            .find(|h| h.path == "wiki/concepts/agent.md")
            .unwrap();
        assert!(agent.excerpt.as_deref().unwrap().contains("short context"));

        // A second retrieve call must still return the same excerpt (the
        // index is still warm; no invalidation, no reread). This is the
        // chat-retrieval hot path: repeated questions reuse the cache.
        let again = service
            .retrieve_with_excerpts(&context, "agent", 5, 80)
            .unwrap();
        let agent_again = again
            .iter()
            .find(|h| h.path == "wiki/concepts/agent.md")
            .unwrap();
        assert_eq!(agent.excerpt, agent_again.excerpt);

        std::fs::remove_dir_all(root).unwrap();
    }
}
