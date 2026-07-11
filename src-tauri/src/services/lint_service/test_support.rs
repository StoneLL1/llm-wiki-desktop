use std::path::PathBuf;

use crate::models::paths::ProjectContext;

pub(super) fn tmp_context(suffix: &str) -> (ProjectContext, PathBuf) {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("llm-wiki-lint-{stamp}-{suffix}"));
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

/// A vault where every rule is satisfied: frontmatter on content pages,
/// bidirectional links, index lists the page, no collisions.
pub(super) fn seed_clean_vault(context: &ProjectContext) {
    write_file(
            context,
            "wiki/concepts/agent.md",
            "---\ntitle: Agent\ntype: concept\ntags: [ai]\nsources:\n  - wiki/sources/source.md\n---\n\n# Agent\n\nLinks to [[react]].\n\n> Sources: [[source]]",
        );
    write_file(
            context,
            "wiki/concepts/react.md",
            "---\ntitle: ReAct\ntype: concept\ntags: [ai]\nsources:\n  - wiki/sources/source.md\n---\n\n# ReAct\n\nLinks back to [[agent]].\n\n> Sources: [[source]]",
        );
    write_file(
        context,
        "wiki/sources/source.md",
        "---\ntitle: Source\ntype: source\n---\n\n# Source\n\nOriginal.",
    );
    write_file(
        context,
        "wiki/index.md",
        "# Index\n\n- [[agent]]\n- [[react]]\n",
    );
    write_file(context, "wiki/log.md", "# Log\n");
}
