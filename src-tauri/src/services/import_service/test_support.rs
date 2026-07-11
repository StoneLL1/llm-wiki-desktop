use std::fs;
use std::path::PathBuf;

use crate::models::paths::ProjectContext;

#[cfg(test)]
pub(super) fn tmp_context(suffix: &str) -> (ProjectContext, PathBuf) {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("llm-wiki-import-{stamp}-{suffix}"));
    fs::create_dir_all(&root).unwrap();
    (ProjectContext::new("project-1", root.clone()), root)
}
