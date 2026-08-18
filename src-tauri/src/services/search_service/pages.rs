use crate::app_state::ProjectWritePermit;
use crate::errors::BackendError;
use crate::models::paths::ProjectContext;
use crate::models::wiki::{CreateWikiPageRequest, RenameWikiPageResponse, SaveWikiPageResponse};
use crate::services::import_v2::transaction::FileTransaction;
use crate::services::WriteMode;
use crate::utils::markdown_utils::{extract_wikilinks, rewrite_wikilinks, split_frontmatter};
use crate::utils::safe_project_dir::remove_project_file;

use super::catalog::file_read_error;
use super::SearchService;

impl SearchService {
    /// Capability-bearing production entry point for wiki-page saves.
    pub(crate) fn save_page_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        relative_path: &str,
        contents: &str,
        expected_hash: Option<String>,
    ) -> Result<SaveWikiPageResponse, BackendError> {
        self.save_page_unchecked(permit.context(), relative_path, contents, expected_hash)
    }

    /// Compatibility surface for integration and service tests. Production
    /// callers must enter through `save_page_authorized`.
    #[cfg(debug_assertions)]
    pub fn save_page(
        &self,
        context: &ProjectContext,
        relative_path: &str,
        contents: &str,
        expected_hash: Option<String>,
    ) -> Result<SaveWikiPageResponse, BackendError> {
        self.save_page_unchecked(context, relative_path, contents, expected_hash)
    }

    /// Save a page with optimistic-concurrency hash checking. `expected_hash =
    /// None` means create-new (rejected if the file exists); `Some(hash)` only
    /// overwrites when the on-disk hash still matches, so an external edit in
    /// Obsidian surfaces as `FILE_HASH_MISMATCH` for the diff confirmation path.
    fn save_page_unchecked(
        &self,
        context: &ProjectContext,
        relative_path: &str,
        contents: &str,
        expected_hash: Option<String>,
    ) -> Result<SaveWikiPageResponse, BackendError> {
        context.resolve_wiki_write_path(relative_path)?;
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
    fn create_page_unchecked(
        &self,
        context: &ProjectContext,
        request: &CreateWikiPageRequest,
    ) -> Result<SaveWikiPageResponse, BackendError> {
        let absolute = context.resolve_wiki_write_path(&request.relative_path)?;

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

    /// Capability-bearing production entry point for wiki-page creation.
    pub(crate) fn create_page_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        request: &CreateWikiPageRequest,
    ) -> Result<SaveWikiPageResponse, BackendError> {
        self.create_page_unchecked(permit.context(), request)
    }

    /// Compatibility surface for integration and service tests. Production
    /// callers must enter through `create_page_authorized`.
    #[cfg(debug_assertions)]
    pub fn create_page(
        &self,
        context: &ProjectContext,
        request: &CreateWikiPageRequest,
    ) -> Result<SaveWikiPageResponse, BackendError> {
        self.create_page_unchecked(context, request)
    }

    /// Rename a wiki page: move the file, then rewrite every `[[old-stem]]`
    /// reference across the wiki to `[[new-stem]]` (preserving aliases/anchors).
    /// A rename is a file move plus a batch rewrite of references, which the
    /// CLAUDE.md hard boundary covers ("覆盖、批量替换 — 操作前必须创建 Git
    /// 检查点"); the caller creates the checkpoint *before* invoking this so the
    /// old page and all reference files are recoverable. Returns the new path
    /// metadata and the list of pages whose references were rewritten.
    fn rename_page_unchecked(
        &self,
        context: &ProjectContext,
        relative_path: &str,
        new_relative_path: &str,
    ) -> Result<RenameWikiPageResponse, BackendError> {
        let old_absolute = context.resolve_wiki_write_path(relative_path)?;
        let new_absolute = context.resolve_wiki_write_path(new_relative_path)?;
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
        let source_hash = self.file_store.content_hash(contents.as_bytes());
        let split = split_frontmatter(&contents);

        // One retained-capability transaction owns every rewrite, the new page,
        // and source deletion. A failure rolls back only files whose installed
        // identity/hash still belongs to this operation.
        let mut transaction = FileTransaction::new_for_project(&context.root);
        let files = self.file_store.list_markdown_files(&context.wiki_dir)?;
        let mut updated_references: Vec<String> = Vec::new();
        for file_absolute in &files {
            // The renamed page itself is handled separately below; rewriting it
            // in-place before the move would race with the rename, so skip it
            // here and rewrite its body as part of the move.
            if file_absolute
                .canonicalize()
                .is_ok_and(|path| path == old_absolute)
            {
                continue;
            }
            let project_relative = context.to_project_relative(file_absolute)?;
            let writable_path = context.resolve_wiki_write_path(&project_relative)?;
            let body = std::fs::read_to_string(&writable_path)
                .map_err(|err| file_read_error(err, &writable_path))?;
            let (rewritten, n) = rewrite_wikilinks(&body, &old_stem, &new_stem);
            if n > 0 {
                transaction.write_if_hash_matches(
                    &writable_path,
                    rewritten.as_bytes(),
                    &self.file_store.content_hash(body.as_bytes()),
                )?;
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
        transaction.write_new(&new_absolute, final_contents.as_bytes())?;
        transaction.delete_if_hash_matches(&old_absolute, &source_hash)?;
        transaction.commit()?;

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

    /// Capability-bearing production entry point for wiki-page rename and
    /// reference rewrites. The caller must create the required Git checkpoint
    /// within the same write critical section before invoking this method.
    pub(crate) fn rename_page_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        relative_path: &str,
        new_relative_path: &str,
    ) -> Result<RenameWikiPageResponse, BackendError> {
        self.rename_page_unchecked(permit.context(), relative_path, new_relative_path)
    }

    /// Compatibility surface for integration and service tests. Production
    /// callers must enter through `rename_page_authorized`.
    #[cfg(debug_assertions)]
    pub fn rename_page(
        &self,
        context: &ProjectContext,
        relative_path: &str,
        new_relative_path: &str,
    ) -> Result<RenameWikiPageResponse, BackendError> {
        self.rename_page_unchecked(context, relative_path, new_relative_path)
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
    /// recoverable and visible in history. This is the generic Wiki page
    /// two-checkpoint contract; Source packages use their dedicated lifecycle.
    /// Returns whether the pre-delete checkpoint produced a commit hash.
    fn apply_page_delete_unchecked(
        &self,
        context: &ProjectContext,
        git_service: &crate::services::GitService,
        target_path: &str,
        target_hash: &str,
    ) -> Result<bool, BackendError> {
        use crate::models::git::CheckpointPurpose;

        let absolute = context.resolve_wiki_write_path(target_path)?;
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

        let mut transaction = FileTransaction::new_for_project(&context.root);
        let result = (|| {
            transaction.delete_if_hash_matches(&absolute, target_hash)?;
            // Drop the deleted page from the graph cache so a stale node
            // doesn't linger; scan will rebuild it. Best-effort.
            if let Ok(graph_cache) = context.resolve_project_write_path(".app/graph-cache.json") {
                let _ = remove_project_file(&context.root, &graph_cache);
            }
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
            let _ = git_service.unstage_paths(context, &[target_path.to_string()]);
            return Err(error);
        }
        transaction.commit()?;

        Ok(checkpoint.commit_hash.is_some())
    }

    pub(crate) fn apply_page_delete_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        git_service: &crate::services::GitService,
        target_path: &str,
        target_hash: &str,
    ) -> Result<bool, BackendError> {
        self.apply_page_delete_unchecked(permit.context(), git_service, target_path, target_hash)
    }

    #[cfg(debug_assertions)]
    pub fn apply_page_delete(
        &self,
        context: &ProjectContext,
        git_service: &crate::services::GitService,
        target_path: &str,
        target_hash: &str,
    ) -> Result<bool, BackendError> {
        self.apply_page_delete_unchecked(context, git_service, target_path, target_hash)
    }

    fn invalidate_graph_cache(&self, context: &ProjectContext) -> bool {
        let Ok(path) = context.resolve_project_write_path(".app/graph-cache.json") else {
            return false;
        };
        if path.exists() {
            remove_project_file(&context.root, &path).is_ok()
        } else {
            false
        }
    }

    fn append_save_log(&self, context: &ProjectContext, relative_path: &str) {
        let Ok(log_path) = context.resolve_wiki_write_path("wiki/log.md") else {
            return;
        };
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

#[cfg(test)]
mod tests {
    use super::SearchService;
    use crate::models::wiki::{CreateWikiPageRequest, SaveWikiPageResponse};
    use crate::services::search_service::test_support::{seed_sample_vault, tmp_context};
    use crate::services::{GitService, WriteMode};
    use crate::utils::time_utils::now_rfc3339;
    use sha2::{Digest, Sha256};

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
    fn save_page_rejects_a_page_discovered_through_an_internal_read_link() {
        let (context, root) = tmp_context("save-linked-page");
        std::fs::create_dir_all(context.wiki_dir.clone()).unwrap();
        let shared = root.join("shared");
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(shared.join("linked.md"), "# Linked").unwrap();
        let link = context.wiki_dir.join("linked");
        create_directory_link(&shared, &link).unwrap();

        // The layout walker reports the physical `shared/linked.md` path for
        // a contained read link. It is readable, but must not be a save target.
        let err = SearchService::default()
            .save_page(
                &context,
                "shared/linked.md",
                "# Modified",
                Some("hash".to_string()),
            )
            .expect_err("read-only linked page must not become writable by its physical path");
        assert_eq!(err.code, "PATH_OUTSIDE_PROJECT");
        assert_eq!(
            std::fs::read_to_string(shared.join("linked.md")).unwrap(),
            "# Linked"
        );

        remove_directory_link(&link);
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
    fn rename_page_keeps_referrers_unchanged_when_destination_is_unsafe() {
        // An invalid destination must not leave the wiki pointing at a
        // non-existent renamed page.
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
        // The strict write resolver rejects the file-as-directory component
        // before it can publish a move. Referrers are still unchanged.
        assert_eq!(err.code, "PATH_OUTSIDE_PROJECT");

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

    #[cfg(unix)]
    fn create_directory_link(
        target: &std::path::Path,
        link: &std::path::Path,
    ) -> std::io::Result<()> {
        std::os::unix::fs::symlink(target, link)
    }

    #[cfg(windows)]
    fn create_directory_link(
        target: &std::path::Path,
        link: &std::path::Path,
    ) -> std::io::Result<()> {
        let output = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ))
        }
    }

    fn remove_directory_link(link: &std::path::Path) {
        let _ = std::fs::remove_dir(link);
    }
}
