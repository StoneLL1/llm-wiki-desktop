mod catalog;
mod excerpts;
mod pages;
mod query;

#[cfg(test)]
mod test_support;

use crate::services::file_store::FileStore;
use crate::services::wiki_index::WikiIndex;

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
    pub(super) file_store: FileStore,
    pub(super) index: WikiIndex,
}
