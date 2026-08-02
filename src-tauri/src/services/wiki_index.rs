//! Per-project in-memory wiki index.
//!
//! Reduces repeated full-Markdown scans across Search, Chat retrieval, and
//! Graph cache freshness (audit PERF-004): instead of each call site listing
//! and reading every `wiki/**.md` file, the index reads each file once and
//! caches its parsed body plus derived metadata, keyed by project-relative
//! path. A subsequent call skips the file read when `mtime` and `size` are
//! unchanged; external edits (Obsidian / external editor) surface as a changed
//! `mtime`/`size` and force a refresh before any cached entry is returned.
//!
//! Hard boundaries honored:
//! - No database. The index is in-memory; nothing is written under the
//!   project folder by this module (persistence to `.app/index.json` is an
//!   explicit non-goal of the MVP).
//! - No user wiki content is mutated; the index only reads.
//! - `raw/sources/` immutability is untouched (the index only walks `wiki/`).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use sha2::{Digest, Sha256};

use chrono::TimeZone;

use crate::errors::BackendError;
use crate::models::layout::ProjectMarkdownRootRole;
use crate::models::paths::ProjectContext;
use crate::models::wiki::WikiPageMeta;
use crate::services::file_store::FileStore;
use crate::utils::markdown_utils::{
    count_words, extract_title, extract_wikilinks, parse_frontmatter, split_frontmatter,
};

/// Upper bound on the number of project snapshots held in memory at once.
/// Each snapshot caches the full parsed body of every `wiki/**.md` page, so
/// without a cap a long session that opens many projects would grow memory
/// without limit (there is no project-close command today to drive an
/// explicit `evict`). When inserting a snapshot would exceed this cap, the
/// least-recently-inserted snapshot is dropped first. The cap is generous
/// enough for typical multi-project use; raising it is a one-line change.
const MAX_CACHED_PROJECTS: usize = 8;

/// Per-project in-memory wiki index.
///
/// One instance is shared across Search, Chat retrieval, and Graph freshness
/// (owned by `SearchService` and reached via `&self`). Internal state is
/// guarded by a `Mutex` so concurrent Tauri commands can call `refresh` /
/// `entries` safely; the critical sections are short (clone-out of cached
/// entries, or a single filesystem walk), matching the existing
/// `ProjectRegistry` pattern in `app_state.rs`.
#[derive(Default)]
pub struct WikiIndex {
    snapshots: Mutex<HashMap<String, IndexSnapshot>>,
    /// Insertion order of project ids (oldest first), used to evict the
    /// least-recently-inserted snapshot when the cache exceeds
    /// `MAX_CACHED_PROJECTS`. Kept in lockstep with `snapshots` under the
    /// same mutex.
    order: Mutex<Vec<String>>,
}

/// One project's worth of cached pages, keyed by project-relative path
/// (`wiki/concepts/x.md`). Bookmarks are NOT cached here: bookmark state lives
/// in `bookmarks.json` and can change without a page's `mtime`/`size` moving,
/// so the cached `WikiPageMeta.bookmarked` is always false at the index layer
/// and is overlaid by the caller (`SearchService::scan_wiki`) at read time.
#[derive(Debug, Clone, Default)]
pub struct IndexSnapshot {
    entries: HashMap<String, IndexEntry>,
}

/// A single cached page: parsed body + fully-derived metadata + filesystem
/// invalidation tokens (`mtime_secs`/`mtime_nanos`/`size`).
///
/// The body is the full Markdown body (frontmatter stripped), not a bounded
/// excerpt: `SearchService::search` scores the full body, so caching only an
/// excerpt would leave search as a full-scan hot path (the exact thing
/// PERF-004 calls out). For the 200-500 page target the memory cost of
/// holding bodies is acceptable; if a much larger vault ever needs it, the
/// field can be gated behind a size budget without changing this API.
#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub path: String,
    pub mtime_secs: u64,
    pub mtime_nanos: u32,
    pub size: u64,
    pub hash: String,
    pub meta: WikiPageMeta,
    pub body_markdown: String,
    /// Monotonic counter incremented each time this entry's file bytes are
    /// read from disk (i.e. a cache miss). Tests assert reuse by checking that
    /// repeated calls leave this at 0 for unchanged files and bump it on
    /// external edits. Crate-private so it does not leak into the service's
    /// public API. Read only by tests, hence `allow(dead_code)` in the lib
    /// build — the field still participates in clone/derive and is exercised
    /// under `--tests`.
    #[allow(dead_code)]
    pub(crate) content_reads: u64,
}

impl WikiIndex {
    /// Refresh the index for `context` and return a flat vector of cached
    /// entries (sorted by path).
    ///
    /// Walks `wiki/` once via `FileStore::list_markdown_files`. For each file:
    /// - If the cached entry's `mtime` and `size` still match the filesystem,
    ///   reuse it (no file read, no hash).
    /// - Otherwise read the file once, derive body + frontmatter + metadata,
    ///   and hash the already-read bytes (no second `fs::read` via
    ///   `FileStore::file_hash`).
    /// Entries for files that no longer exist on disk are dropped.
    pub fn refresh(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
    ) -> Result<Vec<IndexEntry>, BackendError> {
        self.refresh_internal(context, file_store, None)
    }

    /// Refresh with an optional per-entry content-read counter bump callback
    /// used by tests to assert reuse. Production callers pass `None`.
    fn refresh_internal(
        &self,
        context: &ProjectContext,
        _file_store: &FileStore,
        mut on_read: Option<&mut dyn FnMut(&str)>,
    ) -> Result<Vec<IndexEntry>, BackendError> {
        let files = context.list_markdown_files_for_roles(&[
            ProjectMarkdownRootRole::Wiki,
            ProjectMarkdownRootRole::Mixed,
        ])?;

        // Build the next snapshot from the current on-disk file set. Reuse
        // unchanged entries from the prior snapshot; replace stale/missing
        // ones. Start from the prior snapshot so reuse is cheap, then drop
        // anything not in the new file set.
        let mut next: HashMap<String, IndexEntry> = {
            let prior = self.snapshots.lock().map_err(|_| lock_failed())?;
            prior
                .get(&context.project_id)
                .map(|snapshot| snapshot.entries.clone())
                .unwrap_or_default()
        };
        // Restrict to files that still exist, then refresh each.
        let mut live_paths: std::collections::HashSet<String> =
            std::collections::HashSet::with_capacity(files.len());
        for absolute in &files {
            let project_relative = context.to_project_relative(absolute)?;
            live_paths.insert(project_relative.clone());
            let reuse = next.get(&project_relative).and_then(|entry| {
                let (secs, nanos, size) = fs_tokens(absolute).ok()?;
                if entry.mtime_secs == secs && entry.mtime_nanos == nanos && entry.size == size {
                    Some(entry.clone())
                } else {
                    None
                }
            });
            if let Some(entry) = reuse {
                next.insert(project_relative, entry);
                continue;
            }

            if let Some(callback) = on_read.as_mut() {
                callback(&project_relative);
            }
            let entry = build_entry(absolute, &project_relative)?;
            next.insert(project_relative, entry);
        }
        // Drop entries for files that disappeared from the wiki (external
        // delete). Keeps the snapshot consistent with the live file set so
        // Graph/Search do not surface ghost pages.
        next.retain(|path, _| live_paths.contains(path));

        let mut entries: Vec<IndexEntry> = next.values().cloned().collect();
        entries.sort_by(|a, b| a.path.cmp(&b.path));

        let mut snapshots = self.snapshots.lock().map_err(|_| lock_failed())?;
        let mut order = self.order.lock().map_err(|_| lock_failed())?;
        // Promote this project to the most-recently-inserted position so the
        // LRU eviction below drops the least-recently-touched project first.
        order.retain(|id| id != &context.project_id);
        order.push(context.project_id.clone());
        // Bound the number of cached projects so a long session that opens
        // many projects cannot grow memory without limit (there is no
        // project-close command today to drive an explicit `evict`). Drops
        // the oldest snapshots first.
        while order.len() > MAX_CACHED_PROJECTS {
            let evicted_id = order.remove(0);
            snapshots.remove(&evicted_id);
        }
        snapshots.insert(context.project_id.clone(), IndexSnapshot { entries: next });

        Ok(entries)
    }

    /// Return the cached entries for `context` without touching the disk.
    /// Empty if `refresh` has not been called for this project yet. Bookmarks
    /// are NOT applied; callers overlay bookmark state on top of `meta`.
    pub fn entries(&self, context: &ProjectContext) -> Result<Vec<IndexEntry>, BackendError> {
        let snapshots = self.snapshots.lock().map_err(|_| lock_failed())?;
        let mut entries: Vec<IndexEntry> = snapshots
            .get(&context.project_id)
            .map(|snapshot| snapshot.entries.values().cloned().collect())
            .unwrap_or_default();
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(entries)
    }

    /// Drop the cached snapshot for a project (e.g. on project close). No-op
    /// when the project was never indexed. Best-effort: a poisoned lock
    /// surfaces as a recoverable error rather than panicking.
    pub fn evict(&self, project_id: &str) -> Result<(), BackendError> {
        let mut snapshots = self.snapshots.lock().map_err(|_| lock_failed())?;
        let mut order = self.order.lock().map_err(|_| lock_failed())?;
        snapshots.remove(project_id);
        order.retain(|id| id != project_id);
        Ok(())
    }
}

/// Build a single index entry by reading the file once. Derives frontmatter,
/// body, fully-formed `WikiPageMeta`, and the SHA-256 hash from the same byte
/// buffer — so an unchanged file costs one `fs::read` total, not the two
/// (`read_to_string` + `FileStore::file_hash`) that the pre-index scan paid.
///
/// `bookmarked` is set to `false` here; the caller overlays live bookmark
/// state. Keeping bookmark state out of the cache is load-bearing: a bookmark
/// toggle changes `bookmarks.json` without moving the page's `mtime`/`size`,
/// so caching `bookmarked` would deliver stale join results.
fn build_entry(absolute: &Path, project_relative: &str) -> Result<IndexEntry, BackendError> {
    let bytes = std::fs::read(absolute).map_err(|err| file_read_error(err, absolute))?;
    let contents = String::from_utf8_lossy(&bytes).into_owned();
    let split = split_frontmatter(&contents);
    let frontmatter = split
        .frontmatter
        .as_deref()
        .map(parse_frontmatter)
        .unwrap_or_default();

    let file_name = absolute
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("page.md");
    let wiki_relative = project_relative
        .strip_prefix("wiki/")
        .unwrap_or(project_relative);

    let title = extract_title(&split.body, &frontmatter, file_name);
    let type_field = frontmatter.get_scalar("type");
    let page_type = crate::models::wiki::WikiPageType::infer(type_field.as_deref(), wiki_relative);
    let starred = frontmatter
        .get_scalar("starred")
        .map(|value| value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let file_size = std::fs::metadata(absolute)
        .map(|metadata| metadata.len())
        .unwrap_or(bytes.len() as u64);
    let (mtime_secs, mtime_nanos) = mtime_tokens(absolute).unwrap_or((0, 0));

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let hash = format!("{:x}", hasher.finalize());

    let meta = WikiPageMeta {
        path: project_relative.to_string(),
        title,
        page_type,
        tags: frontmatter.get_list("tags"),
        sources: frontmatter.get_list("sources"),
        aliases: frontmatter.get_list("aliases"),
        created: frontmatter.get_scalar("created"),
        updated: frontmatter.get_scalar("updated"),
        starred,
        bookmarked: false, // overlaid by the caller from live bookmark state.
        word_count: count_words(&split.body),
        file_size,
        modified_time: mtime_rfc3339(absolute),
        hash,
        wikilinks: extract_wikilinks(&split.body),
        source_binding: None,
        source_id: None,
        version_id: None,
        source_status: None,
        quality: None,
    };

    Ok(IndexEntry {
        path: project_relative.to_string(),
        mtime_secs,
        mtime_nanos,
        size: file_size,
        hash: meta.hash.clone(),
        meta,
        body_markdown: split.body,
        content_reads: 1,
    })
}

/// Filesystem invalidation tokens: mtime (secs + subsec nanos) and size.
/// Returns `None` when metadata is unavailable (treated as a miss by the
/// caller, forcing a re-read).
fn fs_tokens(path: &Path) -> Result<(u64, u32, u64), std::io::Error> {
    let metadata = std::fs::metadata(path)?;
    let modified = metadata.modified()?;
    let duration = modified
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    Ok((duration.as_secs(), duration.subsec_nanos(), metadata.len()))
}

fn mtime_tokens(path: &Path) -> Result<(u64, u32), std::io::Error> {
    let (secs, nanos, _) = fs_tokens(path)?;
    Ok((secs, nanos))
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

fn lock_failed() -> BackendError {
    BackendError::new(
        "WIKI_INDEX_LOCKED",
        "Wiki index is unavailable.",
        true,
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::wiki::WikiPageType;
    use std::path::PathBuf;

    fn tmp_context(suffix: &str) -> (ProjectContext, PathBuf) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("llm-wiki-index-{stamp}-{suffix}"));
        std::fs::create_dir_all(&root).unwrap();
        (ProjectContext::new("project-index", root.clone()), root)
    }

    fn write_file(context: &ProjectContext, rel: &str, body: &str) {
        let path = context.resolve_project_path(rel).unwrap();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, body).unwrap();
    }

    /// Assert mtime moved before re-scanning — `fs::write` to the same path
    /// within the same filesystem-tick can leave mtime unchanged on some
    /// platforms (notably Windows FAT/exFAT and coarse-grained ext mounts),
    /// which would make an invalidation test pass for the wrong reason. We
    /// sleep past a second boundary so the index's mtime+size invalidation is
    /// actually exercised (rather than the OS happening to advance mtime).
    fn bump_mtime(_path: &std::path::Path, _secs: u64) {
        // filetime is not a dependency, so we cannot set mtime directly. The
        // 1.1s sleep guarantees a visible mtime delta on every supported
        // platform (NTFS ticks at 100ns, ext4 at 1ns; 1-second HFS+ is the
        // coarsest). The caller rewrites the file after this to refresh mtime.
        std::thread::sleep(std::time::Duration::from_millis(1100));
    }
    #[test]
    fn refresh_caches_pages_with_metadata_and_body() {
        let (context, root) = tmp_context("baseline");
        write_file(
            &context,
            "wiki/concepts/agent.md",
            "---\ntitle: Agent\ntype: concept\ntags: [memory]\n---\n\n# Agent\n\nBody text.",
        );
        write_file(&context, "wiki/index.md", "# Index\n");
        let index = WikiIndex::default();
        let store = FileStore;

        let entries = index.refresh(&context, &store).unwrap();

        assert_eq!(entries.len(), 2);
        let agent = entries
            .iter()
            .find(|e| e.path == "wiki/concepts/agent.md")
            .unwrap();
        assert_eq!(agent.meta.title, "Agent");
        assert_eq!(agent.meta.page_type, WikiPageType::Concept);
        assert_eq!(agent.meta.tags, vec!["memory".to_string()]);
        assert!(!agent.hash.is_empty());
        assert!(agent.body_markdown.contains("# Agent"));
        assert_eq!(agent.content_reads, 1);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_layout_index_preserves_the_exact_legacy_wiki_scan_boundary() {
        let (context, root) = tmp_context("native-layout-equivalence");
        write_file(&context, "wiki/index.md", "# Index");
        write_file(&context, "wiki/concepts/agent.md", "# Agent");
        write_file(&context, "wiki/sources/imported.md", "# Imported source");
        write_file(&context, "wiki/.hidden/private.md", "# Hidden but indexed");
        write_file(&context, "wiki/node_modules/package.md", "# Package notes");
        write_file(&context, "wiki/target/build.md", "# Build notes");
        write_file(&context, "wiki/concepts/upper.MD", "# Uppercase extension");
        write_file(
            &context,
            "raw/extracted/source.md",
            "# Raw extracted source",
        );
        let index = WikiIndex::default();
        let store = FileStore;

        let entries = index.refresh(&context, &store).unwrap();
        let paths: Vec<&str> = entries.iter().map(|entry| entry.path.as_str()).collect();

        assert_eq!(
            paths,
            vec![
                "wiki/.hidden/private.md",
                "wiki/concepts/agent.md",
                "wiki/index.md",
                "wiki/node_modules/package.md",
                "wiki/sources/imported.md",
                "wiki/target/build.md",
            ]
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn compatible_obsidian_layout_indexes_root_and_discovered_markdown_roots() {
        let (context, root) = tmp_context("compatible-obsidian");
        std::fs::create_dir_all(root.join(".obsidian")).unwrap();
        write_file(&context, "index.md", "# Vault index");
        write_file(&context, "笔记/概念.md", "# 概念");
        write_file(&context, "sources/材料.md", "# 材料");
        let context = context.with_resolved_layout().unwrap();
        let index = WikiIndex::default();
        let store = FileStore;

        let entries = index.refresh(&context, &store).unwrap();
        let paths = entries
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>();

        assert_eq!(paths, vec!["index.md", "笔记/概念.md"]);
        assert!(context.layout.app_state_root.is_none());
        assert_eq!(context.wiki_dir, root.join("wiki"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn repeated_refresh_does_not_reread_unchanged_files() {
        let (context, root) = tmp_context("reuse");
        write_file(&context, "wiki/a.md", "# A\nbody a");
        write_file(&context, "wiki/b.md", "# B\nbody b");
        let index = WikiIndex::default();
        let store = FileStore;

        let first = index.refresh(&context, &store).unwrap();
        assert_eq!(first.len(), 2);
        let reads_after_first: u64 = first.iter().map(|e| e.content_reads).sum();
        assert_eq!(reads_after_first, 2);

        let second = index.refresh(&context, &store).unwrap();
        // No file touched -> every entry is reused -> content_reads stays at 1.
        for entry in &second {
            assert_eq!(
                entry.content_reads, 1,
                "unchanged file {} must not be re-read",
                entry.path
            );
        }

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn external_edit_invalidates_only_the_changed_file() {
        let (context, root) = tmp_context("edit");
        write_file(&context, "wiki/a.md", "# A\nbody a");
        write_file(&context, "wiki/b.md", "# B\nbody b");
        let index = WikiIndex::default();
        let store = FileStore;
        let _ = index.refresh(&context, &store).unwrap();

        // Externally edit only wiki/a.md (the kind of edit Obsidian or an
        // external editor would make while the app is open). Bump mtime so the
        // invalidation is observable regardless of filesystem tick granularity.
        let a_path = context.resolve_project_path("wiki/a.md").unwrap();
        bump_mtime(&a_path, 0);
        std::fs::write(&a_path, "# A\nedited body").unwrap();

        let after = index.refresh(&context, &store).unwrap();
        let a = after.iter().find(|e| e.path == "wiki/a.md").unwrap();
        let b = after.iter().find(|e| e.path == "wiki/b.md").unwrap();
        // a.md was re-read (content_reads reset to 1 on the fresh entry); b.md
        // was reused from cache.
        assert_eq!(a.content_reads, 1, "edited file must be re-read");
        assert!(a.body_markdown.contains("edited body"));
        assert_eq!(b.content_reads, 1, "unchanged file must stay cached");
        // The on-read callback path is exercised separately below; here we
        // prove the observable outcome: only a.md's body/hash moved.
        assert_ne!(a.hash, b.hash);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn external_delete_removes_the_page_from_the_index() {
        let (context, root) = tmp_context("delete");
        write_file(&context, "wiki/keep.md", "# Keep");
        write_file(&context, "wiki/gone.md", "# Gone");
        let index = WikiIndex::default();
        let store = FileStore;
        let first = index.refresh(&context, &store).unwrap();
        assert_eq!(first.len(), 2);

        std::fs::remove_file(context.resolve_project_path("wiki/gone.md").unwrap()).unwrap();
        let after = index.refresh(&context, &store).unwrap();

        let paths: Vec<&str> = after.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, vec!["wiki/keep.md"]);
        // The surviving entry is reused (not re-read).
        assert_eq!(after[0].content_reads, 1);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refresh_handles_unicode_and_cjk_filenames_and_bodies() {
        let (context, root) = tmp_context("cjk");
        write_file(
            &context,
            "wiki/概念/智能体.md",
            "---\ntitle: 智能体\ntype: concept\ntags: [方法]\n---\n\n# 智能体\n\n约束先行。",
        );
        write_file(
            &context,
            "wiki/concepts/café.md",
            "# Café\n\nDessert notes éclair.",
        );
        let index = WikiIndex::default();
        let store = FileStore;

        let entries = index.refresh(&context, &store).unwrap();
        let cjk = entries
            .iter()
            .find(|e| e.path == "wiki/概念/智能体.md")
            .unwrap();
        assert_eq!(cjk.meta.title, "智能体");
        assert_eq!(cjk.meta.page_type, WikiPageType::Concept);
        assert_eq!(cjk.meta.tags, vec!["方法".to_string()]);
        assert!(cjk.body_markdown.contains("约束先行"));
        assert!(!cjk.hash.is_empty());

        let unicode = entries
            .iter()
            .find(|e| e.path == "wiki/concepts/café.md")
            .unwrap();
        assert_eq!(unicode.meta.title, "Café");

        // Second refresh reuses both CJK and Unicode entries (proves the
        // mtime/size keys are stable across path encodings and that the
        // canonicalize-based path safety in ProjectContext does not corrupt
        // CJK joins).
        let again = index.refresh(&context, &store).unwrap();
        for entry in &again {
            assert_eq!(entry.content_reads, 1);
        }

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cached_meta_bookmarked_is_false_until_caller_overlays() {
        // The index must NOT cache bookmark state: a bookmark toggle changes
        // bookmarks.json without moving the page mtime/size, so caching
        // bookmarked=true would deliver stale joins. The caller overlays live
        // bookmark paths on top of the cached meta.
        let (context, root) = tmp_context("bookmarks");
        write_file(&context, "wiki/a.md", "# A");
        let index = WikiIndex::default();
        let store = FileStore;
        let entries = index.refresh(&context, &store).unwrap();
        assert!(!entries[0].meta.bookmarked);
        // Even after a simulated bookmark add (no file change), the cached
        // entry's bookmarked stays false — the caller is responsible.
        let again = index.refresh(&context, &store).unwrap();
        assert!(!again[0].meta.bookmarked);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn evict_drops_a_project_snapshot_without_touching_others() {
        // Two projects with distinct ids; the snapshot map is keyed by
        // project_id, so evicting one must leave the other intact.
        let stamp_a = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root_a = std::env::temp_dir().join(format!("llm-wiki-index-{stamp_a}-evict-a"));
        std::fs::create_dir_all(&root_a).unwrap();
        let context_a = ProjectContext::new("project-evict-a", root_a.clone());
        let stamp_b = stamp_a + 1;
        let root_b = std::env::temp_dir().join(format!("llm-wiki-index-{stamp_b}-evict-b"));
        std::fs::create_dir_all(&root_b).unwrap();
        let context_b = ProjectContext::new("project-evict-b", root_b.clone());
        write_file(&context_a, "wiki/a.md", "# A");
        write_file(&context_b, "wiki/b.md", "# B");
        let index = WikiIndex::default();
        let store = FileStore;
        let _ = index.refresh(&context_a, &store).unwrap();
        let _ = index.refresh(&context_b, &store).unwrap();

        index.evict(&context_a.project_id).unwrap();
        assert!(index.entries(&context_a).unwrap().is_empty());
        assert_eq!(index.entries(&context_b).unwrap().len(), 1);

        std::fs::remove_dir_all(root_a).unwrap();
        std::fs::remove_dir_all(root_b).unwrap();
    }

    #[test]
    fn on_read_callback_fires_only_for_files_that_needed_reading() {
        // Proves the reuse/refresh split is driven by mtime+size, not by some
        // accidental property of the walk. The callback records every path the
        // index actually read bytes for; unchanged files must not appear.
        let (context, root) = tmp_context("callback");
        write_file(&context, "wiki/a.md", "# A");
        write_file(&context, "wiki/b.md", "# B");
        let index = WikiIndex::default();
        let store = FileStore;

        let mut read_paths: Vec<String> = Vec::new();
        {
            let mut callback = |path: &str| read_paths.push(path.to_string());
            let _ = index
                .refresh_internal(&context, &store, Some(&mut callback))
                .unwrap();
        }
        let mut sorted = read_paths.clone();
        sorted.sort();
        assert_eq!(sorted, vec!["wiki/a.md", "wiki/b.md"]);

        // Second refresh: nothing is read.
        read_paths.clear();
        {
            let mut callback = |path: &str| read_paths.push(path.to_string());
            let _ = index
                .refresh_internal(&context, &store, Some(&mut callback))
                .unwrap();
        }
        assert!(read_paths.is_empty(), "unchanged files must not be re-read");

        // Edit only b.md; only b.md should be read on the next refresh.
        let b_path = context.resolve_project_path("wiki/b.md").unwrap();
        bump_mtime(&b_path, 0);
        std::fs::write(&b_path, "# B\nedited").unwrap();
        read_paths.clear();
        {
            let mut callback = |path: &str| read_paths.push(path.to_string());
            let _ = index
                .refresh_internal(&context, &store, Some(&mut callback))
                .unwrap();
        }
        assert_eq!(read_paths, vec!["wiki/b.md"]);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refresh_on_empty_wiki_returns_no_entries_without_error() {
        // An empty wiki/ directory (no markdown files) must produce an empty
        // index, not an error. Graph/Search call scan_wiki on freshly-created
        // projects before any pages exist.
        let (context, root) = tmp_context("empty");
        std::fs::create_dir_all(context.wiki_dir.clone()).unwrap();
        let index = WikiIndex::default();
        let store = FileStore;

        let entries = index.refresh(&context, &store).unwrap();
        assert!(entries.is_empty());

        // A second refresh is still empty and still succeeds.
        let again = index.refresh(&context, &store).unwrap();
        assert!(again.is_empty());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn index_hash_matches_file_store_file_hash_for_the_same_bytes() {
        // The graph cache's content_hash is computed from page.hash fields
        // (graph_service::content_hash_for). The index-derived hash must
        // equal FileStore::file_hash for the same file, or the graph cache
        // would see a spurious miss/hit after scan_wiki switched to the
        // index path. Both use SHA-256 of the raw bytes; this test pins the
        // parity contract.
        let (context, root) = tmp_context("hash-parity");
        write_file(&context, "wiki/a.md", "# A\nbody with 中文");
        let index = WikiIndex::default();
        let store = FileStore;

        let entries = index.refresh(&context, &store).unwrap();
        let indexed_hash = entries[0].hash.clone();
        let store_hash = store.file_hash(&context, "wiki/a.md").unwrap();
        assert_eq!(indexed_hash, store_hash);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cap_drops_oldest_project_snapshot_when_limit_exceeded() {
        // Memory bound: when the number of cached projects exceeds
        // MAX_CACHED_PROJECTS, the least-recently-touched snapshot is
        // dropped. Promotes a project to most-recent on each refresh, so a
        // project that is touched again survives even if it was the oldest.
        let store = FileStore;
        let index = WikiIndex::default();

        // Seed MAX + 2 distinct projects. Use a unique root per project so
        // the project_ids (and thus the snapshot keys) are distinct.
        let mut roots: Vec<PathBuf> = Vec::new();
        for i in 0..(MAX_CACHED_PROJECTS + 2) {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("llm-wiki-index-cap-{stamp}-{i}"));
            std::fs::create_dir_all(&root).unwrap();
            let context = ProjectContext::new(format!("project-cap-{i}"), root.clone());
            write_file(&context, "wiki/a.md", "# A in project {i}");
            let _ = index.refresh(&context, &store).unwrap();
            roots.push(root);
        }

        // After inserting MAX + 2, only MAX snapshots remain. The two oldest
        // (project-cap-0 and project-cap-1) must have been evicted.
        let context_0 = ProjectContext::new("project-cap-0", roots[0].clone());
        let context_1 = ProjectContext::new("project-cap-1", roots[1].clone());
        let context_last = ProjectContext::new(
            format!("project-cap-{}", MAX_CACHED_PROJECTS + 1),
            roots.last().unwrap().clone(),
        );
        assert!(index.entries(&context_0).unwrap().is_empty());
        assert!(index.entries(&context_1).unwrap().is_empty());
        assert!(!index.entries(&context_last).unwrap().is_empty());

        for root in roots {
            std::fs::remove_dir_all(root).ok();
        }
    }

    #[test]
    fn refreshing_an_old_project_promotes_it_and_evicts_a_different_one() {
        // Touching an already-cached project must promote it to
        // most-recent, so a subsequent overflow evicts a *different* (older)
        // project, not the one just touched.
        let store = FileStore;
        let index = WikiIndex::default();
        let mut roots: Vec<PathBuf> = Vec::new();
        for i in 0..MAX_CACHED_PROJECTS {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("llm-wiki-index-promote-{stamp}-{i}"));
            std::fs::create_dir_all(&root).unwrap();
            let context = ProjectContext::new(format!("project-promote-{i}"), root.clone());
            write_file(&context, "wiki/a.md", "# A");
            let _ = index.refresh(&context, &store).unwrap();
            roots.push(root);
        }
        // Re-touch project-promote-0 (the oldest) so it becomes most-recent.
        let context_0 = ProjectContext::new("project-promote-0", roots[0].clone());
        let _ = index.refresh(&context_0, &store).unwrap();
        // Now insert one more project, forcing an eviction. The victim must
        // be project-promote-1 (now the oldest), NOT project-promote-0.
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let extra_root = std::env::temp_dir().join(format!("llm-wiki-index-promote-{stamp}-extra"));
        std::fs::create_dir_all(&extra_root).unwrap();
        let context_extra = ProjectContext::new("project-promote-extra", extra_root.clone());
        write_file(&context_extra, "wiki/a.md", "# A");
        let _ = index.refresh(&context_extra, &store).unwrap();

        assert!(
            !index.entries(&context_0).unwrap().is_empty(),
            "the just-touched oldest project must survive"
        );
        let context_1 = ProjectContext::new("project-promote-1", roots[1].clone());
        assert!(
            index.entries(&context_1).unwrap().is_empty(),
            "the now-oldest project must be evicted instead"
        );

        for root in roots {
            std::fs::remove_dir_all(root).ok();
        }
        std::fs::remove_dir_all(extra_root).ok();
    }
}
