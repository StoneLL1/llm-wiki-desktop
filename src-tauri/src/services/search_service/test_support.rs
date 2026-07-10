use std::path::PathBuf;

use crate::models::paths::ProjectContext;
use crate::models::search::SearchRequest;
use crate::models::wiki::WikiTreeNode;

pub(super) fn tmp_context(suffix: &str) -> (ProjectContext, PathBuf) {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("llm-wiki-search-{stamp}-{suffix}"));
    std::fs::create_dir_all(&root).unwrap();
    (ProjectContext::new("project-1", root.clone()), root)
}

pub(super) fn write_file(context: &ProjectContext, rel: &str, body: &str) {
    let path = context.resolve_project_path(rel).unwrap();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, body).unwrap();
}

pub(super) fn seed_sample_vault(context: &ProjectContext) {
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

pub(super) fn seed_chinese_question_page(context: &ProjectContext) {
    write_file(
            context,
            "wiki/concepts/constraints-first.md",
            "---\ntitle: 约束先行\naliases: [约束先行2]\ntags: [方法]\n---\n\n# 约束先行\n\n约束先行是一种先定义限制条件再生成方案的工作方式。",
        );
}

pub(super) fn search_request(context: &ProjectContext, query: &str) -> SearchRequest {
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

pub(super) fn find_tree_node<'a>(node: &'a WikiTreeNode, path: &str) -> Option<&'a WikiTreeNode> {
    if node.path == path {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| find_tree_node(child, path))
}

pub(super) fn tmp_index_context(suffix: &str) -> (ProjectContext, PathBuf) {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("llm-wiki-search-idx-{stamp}-{suffix}"));
    std::fs::create_dir_all(&root).unwrap();
    (ProjectContext::new("project-idx", root.clone()), root)
}

pub(super) fn write_index_file(context: &ProjectContext, rel: &str, body: &str) {
    let path = context.resolve_project_path(rel).unwrap();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, body).unwrap();
}

/// Sleep past a 1-second mtime boundary so an external edit is observable
/// to the index's mtime+size invalidation on every supported filesystem.
pub(super) fn cross_mtime_boundary() {
    std::thread::sleep(std::time::Duration::from_millis(1100));
}

pub(super) fn seed_index(context: &ProjectContext) {
    write_index_file(
            context,
            "wiki/concepts/agent.md",
            "---\ntitle: Agent\ntype: concept\ntags: [memory]\n---\n\n# Agent\n\nCovers short context windows.",
        );
    write_index_file(
            context,
            "wiki/concepts/react.md",
            "---\ntitle: ReAct\ntype: concept\ntags: [reasoning]\n---\n\n# ReAct\n\nReason then act loop. See [[agent]].",
        );
    write_index_file(context, "wiki/index.md", "# Index\nWelcome.");
}
