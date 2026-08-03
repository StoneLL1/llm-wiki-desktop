use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use crate::errors::{
    BackendError, PATH_ABSOLUTE_NOT_ALLOWED, PATH_INVALID, PATH_OUTSIDE_PROJECT, PATH_TRAVERSAL,
};
use crate::models::layout::{
    resolve_layout, ProjectLayout, ProjectLayoutConfidence, ProjectLayoutWarning,
    ProjectMarkdownRootRole,
};
use crate::utils::path_safety::{
    validate_existing_project_file, validate_existing_project_root, validate_project_directory,
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
    pub layout: ProjectLayout,
    pub layout_confidence: ProjectLayoutConfidence,
    pub layout_warnings: Vec<ProjectLayoutWarning>,
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
            layout: ProjectLayout::native(),
            layout_confidence: ProjectLayoutConfidence::High,
            layout_warnings: Vec::new(),
            root,
        }
    }

    pub fn with_resolved_layout(mut self) -> Result<Self, BackendError> {
        let resolution = resolve_layout(&self.root)?;
        self.layout = resolution.layout;
        self.layout_confidence = resolution.confidence;
        self.layout_warnings = resolution.warnings;
        Ok(self)
    }

    pub fn list_markdown_files_for_roles(
        &self,
        roles: &[ProjectMarkdownRootRole],
    ) -> Result<Vec<PathBuf>, BackendError> {
        self.layout.list_markdown_files(&self.root, roles)
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

    /// Resolve a path for a mutating operation. Unlike the read resolver,
    /// this rejects every existing descendant link/reparse component so a
    /// page discovered through a read-only internal link cannot be edited,
    /// renamed, or deleted through its logical alias.
    pub fn resolve_project_write_path(&self, relative_path: &str) -> Result<PathBuf, BackendError> {
        let resolved = self.resolve_project_path(relative_path)?;
        let parent = resolved.parent().ok_or_else(|| {
            BackendError::new(
                PATH_INVALID,
                "Project write path must have a parent directory.",
                false,
                true,
            )
        })?;
        let name = resolved.file_name().ok_or_else(|| {
            BackendError::new(
                PATH_INVALID,
                "Project write path must name a file.",
                false,
                true,
            )
        })?;
        let safe_parent = self
            .validate_project_write_parent(parent)
            .map_err(|message| {
                BackendError::new(
                    PATH_OUTSIDE_PROJECT,
                    "Project write path contains a link or unsafe directory.",
                    false,
                    true,
                )
                .with_details(serde_json::json!({ "error": message }))
            })?;
        match fs::symlink_metadata(&resolved) {
            Ok(_) => {
                validate_existing_project_file(&self.root, &resolved).map_err(|message| {
                    BackendError::new(
                        PATH_OUTSIDE_PROJECT,
                        "Project write path contains a link or unsafe file.",
                        false,
                        true,
                    )
                    .with_details(serde_json::json!({ "error": message }))
                })?;
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(BackendError::new(
                    PATH_OUTSIDE_PROJECT,
                    "Project write path is unavailable.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({ "error": error.to_string() })));
            }
        }
        Ok(safe_parent.join(name))
    }

    /// Resolve a page write to the layout's explicitly writable Wiki root.
    /// Markdown reached through an internal read-only link is deliberately
    /// indexed under its physical project-relative path, so accepting every
    /// project path here would make a linked page mutable through that alias.
    pub fn resolve_wiki_write_path(&self, relative_path: &str) -> Result<PathBuf, BackendError> {
        validate_project_relative_path(relative_path)?;
        let wiki_root = self.layout.wiki_write_root.as_deref().ok_or_else(|| {
            BackendError::new(
                PATH_OUTSIDE_PROJECT,
                "This project layout does not provide a writable wiki root.",
                false,
                true,
            )
        })?;
        let normalized = normalize_project_path(relative_path);
        let normalized_wiki_root = normalize_project_path(wiki_root);
        if normalized != normalized_wiki_root
            && !normalized.starts_with(&format!("{normalized_wiki_root}/"))
        {
            return Err(BackendError::new(
                PATH_OUTSIDE_PROJECT,
                "Wiki writes must stay under the layout's writable wiki root.",
                false,
                true,
            ));
        }
        self.resolve_project_write_path(&normalized)
    }

    pub fn resolve_project_write_directory(
        &self,
        relative_path: &str,
    ) -> Result<PathBuf, BackendError> {
        let resolved = self.resolve_project_path(relative_path)?;
        validate_project_directory(&self.root, &resolved).map_err(|message| {
            BackendError::new(
                PATH_OUTSIDE_PROJECT,
                "Project write directory contains a link or unsafe component.",
                false,
                true,
            )
            .with_details(serde_json::json!({ "error": message }))
        })
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

        let relative = match (self.root.canonicalize(), absolute_path.canonicalize()) {
            (Ok(canonical_root), Ok(canonical_path)) => canonical_path
                .strip_prefix(&canonical_root)
                .map(Path::to_path_buf)
                .map_err(|_| {
                    BackendError::new(
                        PATH_OUTSIDE_PROJECT,
                        "Path is outside the current project.",
                        false,
                        true,
                    )
                })?,
            _ => absolute_path
                .strip_prefix(&self.root)
                .map(Path::to_path_buf)
                .map_err(|_| {
                    BackendError::new(
                        PATH_OUTSIDE_PROJECT,
                        "Path is outside the current project.",
                        false,
                        true,
                    )
                })?,
        };

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

    fn validate_project_write_parent(&self, parent: &Path) -> Result<PathBuf, String> {
        if let (Ok(canonical_root), Ok(canonical_parent)) =
            (self.root.canonicalize(), parent.canonicalize())
        {
            if canonical_parent == canonical_root {
                return validate_existing_project_root(&self.root);
            }
        }
        validate_project_directory(&self.root, parent)
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
        assert_eq!(
            context.layout,
            crate::models::layout::ProjectLayout::native()
        );
        assert_eq!(
            context.layout_confidence,
            crate::models::layout::ProjectLayoutConfidence::High
        );
    }

    #[test]
    fn converts_canonical_existing_paths_to_project_relative_paths() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("wiki").join("nested").join("page.md");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "# Page").unwrap();
        let context = ProjectContext::new("project-1", root.path().to_path_buf());

        assert_eq!(
            context
                .to_project_relative(&file.canonicalize().unwrap())
                .unwrap(),
            "wiki/nested/page.md"
        );
    }

    #[test]
    fn rejects_project_write_through_a_descendant_link() {
        let root = tempfile::tempdir().unwrap();
        let shared = tempfile::tempdir().unwrap();
        let wiki = root.path().join("wiki");
        std::fs::create_dir_all(&wiki).unwrap();
        std::fs::write(shared.path().join("linked.md"), "# Linked").unwrap();
        let link = wiki.join("linked");
        create_directory_link(shared.path(), &link).unwrap();
        let context = ProjectContext::new("project-1", root.path().to_path_buf());

        assert!(context
            .resolve_project_write_path("wiki/linked/linked.md")
            .is_err());

        remove_directory_link(&link);
    }

    #[cfg(unix)]
    fn create_directory_link(
        target: &std::path::Path,
        link: &std::path::Path,
    ) -> std::io::Result<()> {
        std::os::unix::fs::symlink(target, link)
    }

    #[cfg(windows)]
    fn create_directory_link(
        target: &std::path::Path,
        link: &std::path::Path,
    ) -> std::io::Result<()> {
        let output = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(link)
            .arg(target)
            .output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(String::from_utf8_lossy(
                &output.stderr,
            )))
        }
    }

    fn remove_directory_link(link: &std::path::Path) {
        let _ = std::fs::remove_dir(link);
    }
}
