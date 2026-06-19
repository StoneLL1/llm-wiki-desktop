use std::path::{Component, Path, PathBuf};

use crate::errors::{
    BackendError, PATH_ABSOLUTE_NOT_ALLOWED, PATH_INVALID, PATH_OUTSIDE_PROJECT, PATH_TRAVERSAL,
};
use crate::utils::path_utils::normalize_project_path;

#[derive(Debug, Clone)]
pub struct ProjectContext {
    pub project_id: String,
    pub root: PathBuf,
    pub app_dir: PathBuf,
    pub raw_dir: PathBuf,
    pub wiki_dir: PathBuf,
    pub exports_dir: PathBuf,
    pub skills_dir: PathBuf,
}

impl ProjectContext {
    pub fn new(project_id: impl Into<String>, root: PathBuf) -> Self {
        Self {
            project_id: project_id.into(),
            app_dir: root.join(".app"),
            raw_dir: root.join("raw"),
            wiki_dir: root.join("wiki"),
            exports_dir: root.join("exports"),
            skills_dir: root.join("skills"),
            root,
        }
    }

    pub fn resolve_project_path(&self, relative_path: &str) -> Result<PathBuf, BackendError> {
        validate_project_relative_path(relative_path)?;

        let normalized = normalize_project_path(relative_path);
        let mut resolved = self.root.clone();

        for segment in normalized.split('/').filter(|segment| !segment.is_empty()) {
            resolved.push(segment);
        }

        self.ensure_no_detectable_escape(&resolved)?;

        Ok(resolved)
    }

    pub fn resolve_wiki_path(&self, relative_path: &str) -> Result<PathBuf, BackendError> {
        validate_project_relative_path(relative_path)?;

        let normalized = normalize_project_path(relative_path);
        let mut resolved = self.wiki_dir.clone();

        for segment in normalized.split('/').filter(|segment| !segment.is_empty()) {
            resolved.push(segment);
        }

        self.ensure_no_detectable_escape(&resolved)?;

        Ok(resolved)
    }

    pub fn to_project_relative(&self, absolute_path: &Path) -> Result<String, BackendError> {
        self.ensure_no_detectable_escape(absolute_path)?;

        let relative = absolute_path.strip_prefix(&self.root).map_err(|_| {
            BackendError::new(
                PATH_OUTSIDE_PROJECT,
                "Path is outside the current project.",
                false,
                true,
            )
        })?;

        let normalized = normalize_project_path(&relative.to_string_lossy());
        validate_project_relative_path(&normalized)?;

        Ok(normalized)
    }

    fn ensure_no_detectable_escape(&self, target: &Path) -> Result<(), BackendError> {
        let Some(existing_target) = nearest_existing_ancestor(target) else {
            return Ok(());
        };

        let Ok(canonical_target) = existing_target.canonicalize() else {
            return Ok(());
        };

        let Ok(canonical_root) = self.root.canonicalize() else {
            return Ok(());
        };

        if canonical_target.starts_with(&canonical_root) {
            Ok(())
        } else {
            Err(BackendError::new(
                PATH_OUTSIDE_PROJECT,
                "Path resolves outside the current project.",
                false,
                true,
            ))
        }
    }
}

fn validate_project_relative_path(relative_path: &str) -> Result<(), BackendError> {
    let normalized = normalize_project_path(relative_path);
    let path = Path::new(&normalized);

    if normalized.trim().is_empty() || normalized == "." {
        return Err(BackendError::new(
            PATH_INVALID,
            "Project-relative path cannot be empty.",
            true,
            true,
        ));
    }

    if normalized.starts_with('/') || normalized.starts_with("//") {
        return Err(BackendError::new(
            PATH_ABSOLUTE_NOT_ALLOWED,
            "Absolute paths are not allowed for project-relative operations.",
            false,
            true,
        ));
    }

    if normalized.contains(':') || path.is_absolute() {
        return Err(BackendError::new(
            PATH_ABSOLUTE_NOT_ALLOWED,
            "Absolute paths are not allowed for project-relative operations.",
            false,
            true,
        ));
    }

    for component in path.components() {
        match component {
            Component::ParentDir => {
                return Err(BackendError::new(
                    PATH_TRAVERSAL,
                    "Project-relative paths cannot escape the project root.",
                    false,
                    true,
                ));
            }
            Component::Prefix(_) | Component::RootDir => {
                return Err(BackendError::new(
                    PATH_ABSOLUTE_NOT_ALLOWED,
                    "Absolute paths are not allowed for project-relative operations.",
                    false,
                    true,
                ));
            }
            Component::CurDir | Component::Normal(_) => {}
        }
    }

    Ok(())
}

fn nearest_existing_ancestor(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|candidate| candidate.exists())
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::ProjectContext;
    use std::path::PathBuf;

    #[test]
    fn derives_standard_project_directories() {
        let root = PathBuf::from("D:/Projects/wiki");
        let context = ProjectContext::new("project-1", root.clone());

        assert_eq!(context.project_id, "project-1");
        assert_eq!(context.app_dir, root.join(".app"));
        assert_eq!(context.raw_dir, root.join("raw"));
        assert_eq!(context.wiki_dir, root.join("wiki"));
        assert_eq!(context.exports_dir, root.join("exports"));
        assert_eq!(context.skills_dir, root.join("skills"));
    }
}
