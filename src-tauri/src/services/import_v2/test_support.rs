use std::path::PathBuf;

use crate::models::import_v2::{ImportInput, ImportInputKind};
use crate::models::paths::ProjectContext;

pub(super) fn test_context(suffix: &str) -> (ProjectContext, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "llm-wiki-import-v2-{}-{}",
        suffix,
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).unwrap();
    (
        ProjectContext::new(format!("project-{suffix}"), root.clone()),
        root,
    )
}

pub(super) fn test_file_input(name: &str) -> ImportInput {
    ImportInput {
        source_identity: None,
        kind: ImportInputKind::File,
        display_name: name.to_string(),
        locator: format!("D:/fixtures/{name}"),
        normalized_locator: Some(format!("file:d:/fixtures/{}", name.to_lowercase())),
        media_save_mode: Default::default(),
    }
}
