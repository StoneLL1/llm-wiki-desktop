use std::collections::HashSet;
use std::path::Path;

use chrono::TimeZone;

use crate::errors::BackendError;
use crate::models::paths::ProjectContext;
use crate::models::wiki::{WikiPageMeta, WikiPageType, WikiTree, WikiTreeNode, WikiTreeNodeKind};
use crate::utils::markdown_utils::{
    count_words, extract_title, extract_wikilinks, parse_frontmatter, split_frontmatter,
    Frontmatter, FrontmatterSplit,
};

use super::SearchService;

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
        let file_size = contents.len() as u64;
        let hash = self.file_store.content_hash(contents.as_bytes());
        let split = split_frontmatter(&contents);
        let frontmatter = split
            .frontmatter
            .as_deref()
            .map(parse_frontmatter)
            .unwrap_or_default();
        let meta = self.build_meta(
            relative_path,
            &absolute,
            &split,
            &frontmatter,
            bookmark_paths,
            file_size,
            hash,
        )?;

        Ok(crate::models::wiki::WikiPageContent {
            meta,
            raw_markdown: contents,
            body_markdown: split.body.clone(),
            frontmatter_yaml: split.frontmatter.clone(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn build_meta(
        &self,
        project_relative: &str,
        absolute: &Path,
        split: &FrontmatterSplit,
        frontmatter: &Frontmatter,
        bookmarks: &HashSet<String>,
        file_size: u64,
        hash: String,
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

        let modified_time = mtime_rfc3339(absolute);

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
            source_binding: None,
            source_id: None,
            version_id: None,
            source_status: None,
            quality: None,
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

pub(super) fn file_read_error(err: std::io::Error, path: &Path) -> BackendError {
    BackendError::new("FILE_READ_FAILED", err.to_string(), true, false)
        .with_details(serde_json::json!({ "path": path.to_string_lossy() }))
}

#[cfg(test)]
mod tests {
    use super::SearchService;
    use crate::models::wiki::{WikiPageContent, WikiPageType, WikiTree};
    use crate::services::search_service::test_support::{
        find_tree_node, seed_sample_vault, tmp_context,
    };
    use crate::services::BookmarkService;
    use std::collections::HashSet;

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
        assert_eq!(
            page.meta.hash,
            service
                .file_store
                .content_hash(page.raw_markdown.as_bytes())
        );
        assert_eq!(page.meta.file_size, page.raw_markdown.len() as u64);

        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod index_integration_tests {
    use super::SearchService;
    use crate::services::search_service::test_support::{
        cross_mtime_boundary, seed_index as seed, tmp_index_context as tmp_context, write_file,
    };
    use crate::services::BookmarkService;
    use std::collections::HashSet;

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
}
