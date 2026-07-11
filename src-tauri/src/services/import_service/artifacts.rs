use std::fs;

use crate::errors::BackendError;
use crate::models::paths::ProjectContext;
use crate::services::file_store::FileStore;

pub(super) fn remove_project_files(context: &ProjectContext, paths: &[String]) {
    for path in paths {
        if let Ok(absolute) = context.resolve_project_path(path) {
            let _ = fs::remove_file(absolute);
        }
    }
}

pub(super) fn verify_project_hash(
    file_store: &FileStore,
    context: &ProjectContext,
    path: &str,
    expected: &str,
) -> Result<(), BackendError> {
    if file_store.file_hash(context, path)? != expected {
        return Err(BackendError::new(
            "CONFIRMATION_STATE_MISMATCH",
            "The original source changed after preview.",
            true,
            true,
        ));
    }
    Ok(())
}

pub(super) fn validate_artifact_paths(
    context: &ProjectContext,
    paths: &[String],
) -> Result<(), BackendError> {
    for path in paths {
        let normalized = path.replace('\\', "/");
        if !normalized.starts_with("raw/extracted/") && !normalized.starts_with("wiki/sources/") {
            return Err(BackendError::new(
                "SOURCE_ARTIFACT_PATH_INVALID",
                "Source artifacts must remain under raw/extracted or wiki/sources.",
                false,
                true,
            ));
        }
        context.resolve_project_path(&normalized)?;
    }
    Ok(())
}
