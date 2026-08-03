use sha2::{Digest, Sha256};
use std::collections::HashMap;

use crate::errors::BackendError;
use crate::models::graph::{
    GraphBuildResult, GraphData, GraphEdge, GraphLayout, GraphNode, SaveGraphLayoutRequest,
};
use crate::models::paths::ProjectContext;
use crate::models::wiki::WikiPageMeta;
use crate::services::file_store::FileStore;

/// Tag groups larger than this skip tag-co-occurrence edges, keeping the MVP
/// edge set bounded for very large wikis. Wikilink edges are unaffected.
const MAX_TAG_GROUP_FOR_EDGES: usize = 64;

/// Builds wiki graph topology (one node per page, edges from `[[wikilinks]]`
/// plus tag co-occurrence) and owns the `.app/graph-cache.json` round-trip.
///
/// ForceAtlas2 layout and Louvain communities are computed on the frontend
/// (graphology) and persisted back through [`Self::save_layout`]; the backend
/// only stores topology, a content hash for staleness, and any layout the
/// frontend has written.
#[derive(Default)]
pub struct GraphService {
    file_store: FileStore,
}

impl GraphService {
    /// Build graph data from a flat page list (typically the output of
    /// `SearchService::scan_wiki`). Pure: no filesystem access.
    pub fn build_from_pages(&self, pages: &[WikiPageMeta]) -> GraphData {
        let lookup = build_target_lookup(pages);
        let mut signal_weights: HashMap<(String, String), usize> = HashMap::new();

        // Primary signal: resolved [[wikilinks]].
        for page in pages {
            for target in &page.wikilinks {
                let Some(resolved_path) = resolve_wikilink(target, &lookup) else {
                    continue;
                };
                if *resolved_path == page.path {
                    continue;
                }
                add_signal(&mut signal_weights, &page.path, resolved_path);
            }
        }

        // Secondary signal: shared tags. Group once per tag, emit pairwise edges
        // within bounded groups so a single oversized tag cannot blow up the
        // edge set.
        let mut by_tag: HashMap<String, Vec<String>> = HashMap::new();
        for page in pages {
            for tag in &page.tags {
                let key = tag.trim().to_ascii_lowercase();
                if key.is_empty() {
                    continue;
                }
                by_tag.entry(key).or_default().push(page.path.clone());
            }
        }
        for (_, mut group) in by_tag {
            if group.len() < 2 || group.len() > MAX_TAG_GROUP_FOR_EDGES {
                continue;
            }
            group.sort();
            for i in 0..group.len() {
                for j in (i + 1)..group.len() {
                    add_signal(&mut signal_weights, &group[i], &group[j]);
                }
            }
        }

        let mut degree: HashMap<String, usize> = HashMap::new();
        let mut edges: Vec<GraphEdge> = signal_weights
            .into_iter()
            .map(|((a, b), weight)| {
                *degree.entry(a.clone()).or_insert(0) += 1;
                *degree.entry(b.clone()).or_insert(0) += 1;
                GraphEdge {
                    source: a,
                    target: b,
                    relation: "related".to_string(),
                    weight,
                }
            })
            .collect();
        edges.sort_by(|a, b| {
            a.source
                .cmp(&b.source)
                .then_with(|| a.target.cmp(&b.target))
        });

        let mut nodes: Vec<GraphNode> = pages
            .iter()
            .map(|page| GraphNode {
                id: page.path.clone(),
                path: page.path.clone(),
                label: page.title.clone(),
                page_type: page.page_type,
                tags: page.tags.clone(),
                starred: page.starred,
                degree: *degree.get(&page.path).unwrap_or(&0),
            })
            .collect();
        nodes.sort_by(|a, b| a.id.cmp(&b.id));

        GraphData {
            content_hash: content_hash_for(pages),
            built_at: crate::utils::time_utils::now_rfc3339(),
            nodes,
            edges,
            layout: None,
        }
    }

    /// SHA-256 over sorted `path:hash` lines. Stable across runs and platform
    /// path-separator styles because page paths are already forward-slashed.
    pub fn content_hash(&self, pages: &[WikiPageMeta]) -> String {
        content_hash_for(pages)
    }

    /// Read the cached graph. Returns `None` when the cache is missing or
    /// corrupt so callers can rebuild transparently (corrupted-cache recovery).
    pub fn read_cache(&self, context: &ProjectContext) -> Option<GraphData> {
        let path = context
            .resolve_project_write_path(".app/graph-cache.json")
            .ok()?;
        self.file_store.read_json_file::<GraphData>(&path).ok()
    }

    /// Atomic write of the full graph payload.
    pub fn write_cache(
        &self,
        context: &ProjectContext,
        data: &GraphData,
    ) -> Result<(), BackendError> {
        self.file_store
            .write_json_atomic(context, ".app/graph-cache.json", data)
    }

    /// Resolve the graph for a project: serve from cache when the content hash
    /// matches, otherwise rebuild and persist. `pages` is the freshly scanned
    /// page list used both to compute the live hash and to rebuild.
    pub fn resolve(
        &self,
        context: &ProjectContext,
        pages: &[WikiPageMeta],
    ) -> Result<GraphBuildResult, BackendError> {
        let live_hash = content_hash_for(pages);

        if let Some(cached) = self.read_cache(context) {
            if cached.content_hash == live_hash {
                // Treat a layout that doesn't cover every current node as stale
                // so the frontend recomputes instead of mixing cached + random
                // positions.
                let layout_stale = cached
                    .layout
                    .as_ref()
                    .map(|layout| !layout_covers_nodes(layout, &cached.nodes))
                    .unwrap_or(true);
                return Ok(GraphBuildResult {
                    data: cached,
                    cached: true,
                    layout_stale,
                });
            }
        }

        let data = self.build_from_pages(pages);
        self.write_cache(context, &data)?;
        Ok(GraphBuildResult {
            layout_stale: true,
            data,
            cached: false,
        })
    }

    /// Force a rebuild regardless of cache state and persist it.
    pub fn rebuild(
        &self,
        context: &ProjectContext,
        pages: &[WikiPageMeta],
    ) -> Result<GraphBuildResult, BackendError> {
        let data = self.build_from_pages(pages);
        self.write_cache(context, &data)?;
        Ok(GraphBuildResult {
            layout_stale: true,
            data,
            cached: false,
        })
    }

    /// Persist a frontend-computed layout onto an existing, hash-matching cache.
    /// Stale requests (missing cache or mismatched hash) are ignored so we never
    /// attach a layout computed for a different wiki version.
    pub fn save_layout(
        &self,
        context: &ProjectContext,
        request: SaveGraphLayoutRequest,
    ) -> Result<Option<GraphData>, BackendError> {
        let Some(mut cached) = self.read_cache(context) else {
            return Ok(None);
        };
        if cached.content_hash != request.content_hash {
            return Ok(None);
        }
        cached.layout = Some(GraphLayout {
            positions: request.positions,
            communities: request.communities,
        });
        self.write_cache(context, &cached)?;
        Ok(Some(cached))
    }
}

fn layout_covers_nodes(layout: &GraphLayout, nodes: &[GraphNode]) -> bool {
    nodes
        .iter()
        .all(|node| layout.positions.contains_key(&node.id))
}

/// Build a case-insensitive lookup from note-name/title/alias -> page path.
/// The filename stem is the primary key (Obsidian-compatible); title and
/// aliases are added as alternates. First page wins on collision.
fn build_target_lookup(pages: &[WikiPageMeta]) -> HashMap<String, String> {
    let mut lookup: HashMap<String, String> = HashMap::new();
    for page in pages {
        let stems = resolution_keys(page);
        for key in stems {
            lookup.entry(key).or_insert_with(|| page.path.clone());
        }
    }
    lookup
}

/// Keys under which a page can be reached from a `[[wikilink]]`.
fn resolution_keys(page: &WikiPageMeta) -> Vec<String> {
    let mut keys = Vec::new();
    if let Some(stem) = file_stem(&page.path) {
        keys.push(stem.to_ascii_lowercase());
    }
    keys.push(page.title.trim().to_ascii_lowercase());
    for alias in &page.aliases {
        keys.push(alias.trim().to_ascii_lowercase());
    }
    keys
}

fn file_stem(path: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");
    let file_name = normalized.rsplit('/').next()?;
    file_name
        .strip_suffix(".md")
        .map(|stem| stem.to_string())
        .or_else(|| Some(file_name.to_string()))
}

/// Resolve a wikilink target against the lookup. Strips alias/anchor syntax
/// already removed by the markdown parser; we match on the bare target.
fn resolve_wikilink<'a>(target: &str, lookup: &'a HashMap<String, String>) -> Option<&'a String> {
    let key = target.trim().to_ascii_lowercase();
    lookup.get(&key)
}

fn add_signal(weights: &mut HashMap<(String, String), usize>, a: &str, b: &str) {
    let pair = if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    };
    *weights.entry(pair).or_insert(0) += 1;
}

fn content_hash_for(pages: &[WikiPageMeta]) -> String {
    let mut sorted: Vec<&WikiPageMeta> = pages.iter().collect();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));

    let mut hasher = Sha256::new();
    for page in &sorted {
        hasher.update(page.path.as_bytes());
        hasher.update(b":");
        hasher.update(page.hash.as_bytes());
        hasher.update(b"\n");
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::paths::ProjectContext;
    use crate::models::wiki::{WikiPageMeta, WikiPageType};
    use std::path::PathBuf;

    fn meta(path: &str, title: &str, page_type: WikiPageType) -> WikiPageMeta {
        WikiPageMeta {
            path: path.to_string(),
            title: title.to_string(),
            page_type,
            tags: Vec::new(),
            sources: Vec::new(),
            aliases: Vec::new(),
            created: None,
            updated: None,
            starred: false,
            bookmarked: false,
            word_count: 0,
            file_size: 0,
            modified_time: String::new(),
            hash: format!("h-{path}"),
            wikilinks: Vec::new(),
            source_binding: None,
            source_id: None,
            version_id: None,
            source_status: None,
            quality: None,
        }
    }

    fn tmp_context(suffix: &str) -> (ProjectContext, PathBuf) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("llm-wiki-graph-{stamp}-{suffix}"));
        std::fs::create_dir_all(&root).unwrap();
        (ProjectContext::new("project-1", root.clone()), root)
    }

    #[test]
    fn builds_one_node_per_page_with_type_and_label() {
        let pages = vec![
            meta("wiki/concepts/agent.md", "Agent", WikiPageType::Concept),
            meta("wiki/entities/claude.md", "Claude", WikiPageType::Entity),
            meta("wiki/概念/记忆.md", "记忆", WikiPageType::Concept),
        ];
        let service = GraphService::default();
        let data = service.build_from_pages(&pages);

        assert_eq!(data.nodes.len(), 3);
        let labels: Vec<&str> = data.nodes.iter().map(|n| n.label.as_str()).collect();
        assert!(labels.contains(&"Agent"));
        assert!(labels.contains(&"记忆")); // CJK preserved
        let concept = data.nodes.iter().find(|n| n.label == "Agent").unwrap();
        assert_eq!(concept.page_type, WikiPageType::Concept);
        assert_eq!(concept.id, "wiki/concepts/agent.md");
    }

    #[test]
    fn wikilink_creates_edge_to_resolved_target() {
        let mut agent = meta("wiki/concepts/agent.md", "Agent", WikiPageType::Concept);
        agent.wikilinks = vec!["react".to_string()];
        let react = meta("wiki/concepts/react.md", "ReAct", WikiPageType::Concept);

        let data = GraphService::default().build_from_pages(&[agent, react]);
        assert_eq!(data.edges.len(), 1);
        let edge = &data.edges[0];
        assert_eq!(edge.relation, "related");
        assert_eq!(edge.source, "wiki/concepts/agent.md");
        assert_eq!(edge.target, "wiki/concepts/react.md");
    }

    #[test]
    fn unresolved_wikilink_creates_no_edge() {
        let mut agent = meta("wiki/concepts/agent.md", "Agent", WikiPageType::Concept);
        agent.wikilinks = vec!["does-not-exist".to_string()];
        let data = GraphService::default().build_from_pages(&[agent]);
        assert!(data.edges.is_empty());
    }

    #[test]
    fn shared_tag_creates_edge_and_signals_combine() {
        let mut agent = meta("wiki/concepts/agent.md", "Agent", WikiPageType::Concept);
        agent.tags = vec!["memory".to_string()];
        agent.wikilinks = vec!["react".to_string()];
        let mut react = meta("wiki/concepts/react.md", "ReAct", WikiPageType::Concept);
        react.tags = vec!["memory".to_string()];

        let data = GraphService::default().build_from_pages(&[agent, react]);
        assert_eq!(data.edges.len(), 1);
        // wikilink + shared tag => weight 2
        assert_eq!(data.edges[0].weight, 2);
    }

    #[test]
    fn edges_are_deduped_and_directed_pairs_collapse() {
        let mut a = meta("wiki/a.md", "A", WikiPageType::Other);
        a.wikilinks = vec!["b".to_string()];
        let mut b = meta("wiki/b.md", "B", WikiPageType::Other);
        b.wikilinks = vec!["a".to_string()];

        let data = GraphService::default().build_from_pages(&[a, b]);
        assert_eq!(data.edges.len(), 1);
    }

    #[test]
    fn self_loops_are_avoided() {
        let mut a = meta("wiki/a.md", "A", WikiPageType::Other);
        a.wikilinks = vec!["a".to_string()];
        let data = GraphService::default().build_from_pages(&[a]);
        assert!(data.edges.is_empty());
    }

    #[test]
    fn degree_counts_incident_edges() {
        let mut hub = meta("wiki/hub.md", "Hub", WikiPageType::Other);
        hub.wikilinks = vec!["a".to_string(), "b".to_string()];
        let a = meta("wiki/a.md", "A", WikiPageType::Other);
        let b = meta("wiki/b.md", "B", WikiPageType::Other);

        let data = GraphService::default().build_from_pages(&[hub, a, b]);
        let hub_node = data.nodes.iter().find(|n| n.label == "Hub").unwrap();
        assert_eq!(hub_node.degree, 2);
    }

    #[test]
    fn content_hash_is_stable_and_changes_with_content() {
        let pages = vec![meta("wiki/a.md", "A", WikiPageType::Other)];
        let service = GraphService::default();
        let h1 = service.content_hash(&pages);
        let h2 = service.content_hash(&pages);
        assert_eq!(h1, h2);

        let mut changed = pages.clone();
        changed[0].hash = "different".to_string();
        let h3 = service.content_hash(&changed);
        assert_ne!(h1, h3);
    }

    #[test]
    fn cache_roundtrip_preserves_data() {
        let (context, root) = tmp_context("roundtrip");
        let pages = vec![
            meta("wiki/concepts/agent.md", "Agent", WikiPageType::Concept),
            meta("wiki/entities/claude.md", "Claude", WikiPageType::Entity),
        ];
        let service = GraphService::default();
        let data = service.build_from_pages(&pages);
        service.write_cache(&context, &data).unwrap();

        let back = service.read_cache(&context).expect("cache should exist");
        assert_eq!(back.nodes.len(), 2);
        assert_eq!(back.content_hash, data.content_hash);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn corrupted_cache_is_recovered_as_missing() {
        let (context, root) = tmp_context("corrupt");
        std::fs::create_dir_all(&context.app_dir).unwrap();
        std::fs::write(context.app_dir.join("graph-cache.json"), "{ not valid json").unwrap();

        let service = GraphService::default();
        // Corrupt JSON must not panic; it is treated as no cache.
        assert!(service.read_cache(&context).is_none());

        // resolve rebuilds and overwrites the corrupt file.
        let pages = vec![meta("wiki/a.md", "A", WikiPageType::Other)];
        let result = service.resolve(&context, &pages).unwrap();
        assert!(!result.cached);
        let healthy = service.read_cache(&context);
        assert!(healthy.is_some());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolve_serves_cache_when_hash_matches() {
        let (context, root) = tmp_context("cache-hit");
        let pages = vec![meta("wiki/a.md", "A", WikiPageType::Other)];
        let service = GraphService::default();
        service
            .write_cache(&context, &service.build_from_pages(&pages))
            .unwrap();

        let result = service.resolve(&context, &pages).unwrap();
        assert!(result.cached);
        assert!(result.layout_stale); // no layout persisted yet

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolve_rebuilds_when_hash_diverges() {
        let (context, root) = tmp_context("stale");
        let old_pages = vec![meta("wiki/a.md", "A", WikiPageType::Other)];
        let service = GraphService::default();
        service
            .write_cache(&context, &service.build_from_pages(&old_pages))
            .unwrap();

        // New page set -> different content hash -> cache miss.
        let new_pages = vec![
            meta("wiki/a.md", "A", WikiPageType::Other),
            meta("wiki/b.md", "B", WikiPageType::Other),
        ];
        let result = service.resolve(&context, &new_pages).unwrap();
        assert!(!result.cached);
        assert_eq!(result.data.nodes.len(), 2);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolve_rebuilds_stale_empty_cache_when_live_pages_exist() {
        let (context, root) = tmp_context("stale-empty");
        let mut a = meta("wiki/A.md", "A", WikiPageType::Other);
        a.wikilinks = vec!["B".to_string()];
        let pages = vec![a, meta("wiki/B.md", "B", WikiPageType::Other)];
        let service = GraphService::default();
        let stale = GraphData {
            nodes: Vec::new(),
            edges: Vec::new(),
            content_hash: "stale-hash".into(),
            built_at: "2026-07-04T00:00:00Z".into(),
            layout: None,
        };
        service.write_cache(&context, &stale).unwrap();

        let result = service.resolve(&context, &pages).unwrap();

        assert!(!result.cached);
        assert!(result.layout_stale);
        assert_eq!(result.data.nodes.len(), 2);
        assert_eq!(result.data.edges.len(), 1);
        assert_eq!(result.data.content_hash, service.content_hash(&pages));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolve_marks_layout_stale_when_positions_do_not_cover_nodes() {
        let (context, root) = tmp_context("partial-layout");
        let mut a = meta("wiki/A.md", "A", WikiPageType::Other);
        a.wikilinks = vec!["B".to_string()];
        let pages = vec![a, meta("wiki/B.md", "B", WikiPageType::Other)];
        let service = GraphService::default();
        let mut data = service.rebuild(&context, &pages).unwrap().data;

        data.layout = Some(GraphLayout {
            positions: HashMap::from([("wiki/A.md".to_string(), [1.0, 2.0])]),
            communities: HashMap::new(),
        });
        service.write_cache(&context, &data).unwrap();

        let result = service.resolve(&context, &pages).unwrap();

        assert!(result.cached);
        assert!(result.layout_stale);
        assert_eq!(result.data.nodes.len(), 2);

        data.layout = Some(GraphLayout {
            positions: HashMap::from([
                ("wiki/ghost-1.md".to_string(), [1.0, 2.0]),
                ("wiki/ghost-2.md".to_string(), [3.0, 4.0]),
            ]),
            communities: HashMap::new(),
        });
        service.write_cache(&context, &data).unwrap();

        let wrong_keys = service.resolve(&context, &pages).unwrap();

        assert!(wrong_keys.cached);
        assert!(wrong_keys.layout_stale);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn save_layout_persists_when_hash_matches_and_ignores_when_stale() {
        let (context, root) = tmp_context("layout");
        let pages = vec![meta("wiki/a.md", "A", WikiPageType::Other)];
        let service = GraphService::default();
        let built = service.build_from_pages(&pages);
        let hash = built.content_hash.clone();
        service.write_cache(&context, &built).unwrap();

        let mut positions = std::collections::HashMap::new();
        positions.insert("wiki/a.md".to_string(), [1.0_f64, 2.0_f64]);
        let mut communities = std::collections::HashMap::new();
        communities.insert("wiki/a.md".to_string(), 0_usize);

        let saved = service
            .save_layout(
                &context,
                SaveGraphLayoutRequest {
                    project_id: "p".into(),
                    project_root_path: context.root.to_string_lossy().to_string(),
                    content_hash: hash.clone(),
                    positions: positions.clone(),
                    communities: communities.clone(),
                },
            )
            .unwrap();
        assert!(saved.is_some());
        let after = service.read_cache(&context).unwrap();
        assert_eq!(after.layout.unwrap().positions, positions);

        // Stale hash -> no-op.
        let stale = service
            .save_layout(
                &context,
                SaveGraphLayoutRequest {
                    project_id: "p".into(),
                    project_root_path: context.root.to_string_lossy().to_string(),
                    content_hash: "wrong-hash".to_string(),
                    positions,
                    communities,
                },
            )
            .unwrap();
        assert!(stale.is_none());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn file_stem_handles_cjk_and_subdirs() {
        assert_eq!(file_stem("wiki/概念/记忆.md").as_deref(), Some("记忆"));
        assert_eq!(file_stem("wiki/a.md").as_deref(), Some("a"));
        assert_eq!(file_stem("readme").as_deref(), Some("readme"));
    }
}
