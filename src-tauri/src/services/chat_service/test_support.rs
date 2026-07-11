use std::path::PathBuf;

use crate::models::chat::{ChatMessage, ChatRole, ChatSourceRef};
use crate::models::paths::ProjectContext;

pub(super) fn tmp_context(suffix: &str) -> (ProjectContext, PathBuf) {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("llm-wiki-chat-{stamp}-{suffix}"));
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

pub(super) fn seed_vault(context: &ProjectContext) {
    write_file(
        context,
        "wiki/concepts/react-pattern.md",
        "---\ntitle: ReAct Pattern\ntype: concept\ntags: [reasoning]\nsources:\n  - wiki/sources/shared.md\n---\n\n# ReAct Pattern\n\nReason then act loop for agents. See [[agent-memory]].",
    );
    write_file(
        context,
        "wiki/concepts/agent-memory.md",
        "---\ntitle: Agent Memory\ntype: concept\ntags: [memory]\nsources:\n  - wiki/sources/shared.md\n---\n\n# Agent Memory\n\nCovers short context windows and RAG.",
    );
    write_file(
        context,
        "wiki/sources/shared.md",
        "---\ntitle: Shared Source\ntype: source\n---\n\n# Shared Source\n\nOriginal source.",
    );
    write_file(context, "wiki/index.md", "# Index\n");
    write_file(
        context,
        "purpose.md",
        "# Purpose\n\nThis wiki explains agents.",
    );
}

pub(super) fn user_message(content: &str) -> ChatMessage {
    ChatMessage {
        id: format!("u-{}", content.len()),
        role: ChatRole::User,
        content: content.into(),
        created_at: "2026-06-20T00:00:00Z".into(),
        citations: Vec::new(),
        route: None,
        provider: None,
        task_id: None,
        convenience_edit: None,
        retrieval_diagnostics: None,
    }
}

pub(super) fn source_ref(id: &str, path: &str, title: &str) -> ChatSourceRef {
    ChatSourceRef {
        id: id.into(),
        page_path: path.into(),
        title: title.into(),
        excerpt: Some(format!("{title} excerpt")),
        score: 100,
        is_pinned: false,
    }
}
