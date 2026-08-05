use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

/// Resolve an existing project root to its canonical directory. A root-level
/// link or Windows junction is allowed only through this canonical handoff;
/// descendants still use the more specific helpers below and may never be
/// links or reparse points.
pub(crate) fn validate_existing_project_root(project_root: &Path) -> Result<PathBuf, String> {
    let canonical_root = project_root
        .canonicalize()
        .map_err(|error| format!("Project root is unavailable: {error}"))?;
    let metadata = fs::symlink_metadata(&canonical_root)
        .map_err(|error| format!("Project root is unavailable: {error}"))?;
    if metadata_is_link_or_reparse(&metadata) {
        return Err("Canonical project root is a link or reparse point".into());
    }
    if !metadata.is_dir() {
        return Err("Project root is not a directory".into());
    }
    Ok(canonical_root)
}

/// Resolve a project-owned directory without following any descendant link or
/// Windows reparse point. Missing suffix components are allowed so callers can
/// inspect a not-yet-created state root without mutating the project.
pub(crate) fn validate_project_directory(
    project_root: &Path,
    directory: &Path,
) -> Result<PathBuf, String> {
    let (canonical_root, relative) = canonical_root_and_relative(project_root, directory)?;
    validate_directory_components(&canonical_root, &relative, true)?;
    Ok(canonical_root.join(relative))
}

/// Resolve a project-owned directory and require the complete path to exist.
/// This is intended for the check immediately before a filesystem operation.
pub(crate) fn validate_existing_project_directory(
    project_root: &Path,
    directory: &Path,
) -> Result<PathBuf, String> {
    let (canonical_root, relative) = canonical_root_and_relative(project_root, directory)?;
    validate_directory_components(&canonical_root, &relative, false)?;
    Ok(canonical_root.join(relative))
}

/// Resolve an existing regular file while rejecting links/reparse points in
/// the file itself and in every directory component below the project root.
pub(crate) fn validate_existing_project_file(
    project_root: &Path,
    file: &Path,
) -> Result<PathBuf, String> {
    let (canonical_root, relative) = canonical_root_and_relative(project_root, file)?;
    let components = relative.components().collect::<Vec<_>>();
    let mut current = canonical_root.clone();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(segment) = component else {
            return Err("Project file contains an unsafe path component".into());
        };
        current.push(segment);
        let metadata = fs::symlink_metadata(&current).map_err(|error| {
            format!("Project file is unavailable {}: {error}", current.display())
        })?;
        if metadata_is_link_or_reparse(&metadata) {
            return Err(format!(
                "Project file contains a link or reparse point: {}",
                current.display()
            ));
        }
        let is_file = index + 1 == components.len();
        if (is_file && !metadata.is_file()) || (!is_file && !metadata.is_dir()) {
            return Err(format!(
                "Project file has an unexpected component type: {}",
                current.display()
            ));
        }
        let canonical = current.canonicalize().map_err(|error| {
            format!("Project file is unavailable {}: {error}", current.display())
        })?;
        if !canonical.starts_with(&canonical_root) {
            return Err("Project file resolves outside the project root".into());
        }
    }
    Ok(canonical_root.join(relative))
}

/// Create a project-owned directory one component at a time, checking every
/// existing or newly created component before continuing.
pub(crate) fn ensure_project_directory(
    project_root: &Path,
    directory: &Path,
) -> Result<PathBuf, String> {
    ensure_project_directory_with_created(project_root, directory).map(|(directory, _)| directory)
}

/// Like [`ensure_project_directory`], but also returns the exact components
/// this process created. Callers that need to roll back a failed, scoped
/// operation must only remove these returned paths after revalidating them;
/// a pre-existing or concurrently-created component never belongs to their
/// rollback.
pub(crate) fn ensure_project_directory_with_created(
    project_root: &Path,
    directory: &Path,
) -> Result<(PathBuf, Vec<PathBuf>), String> {
    let (canonical_root, relative) = canonical_root_and_relative(project_root, directory)?;
    let mut current = canonical_root.clone();
    let mut created = Vec::new();

    for component in relative.components() {
        let Component::Normal(segment) = component else {
            return Err("Project directory contains an unsafe path component".into());
        };
        current.push(segment);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => validate_directory_metadata(&canonical_root, &current, &metadata)?,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                match fs::create_dir(&current) {
                    Ok(()) => created.push(current.clone()),
                    Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
                    Err(error) => {
                        return Err(format!(
                            "Failed to create project directory {}: {error}",
                            current.display()
                        ));
                    }
                }
                let metadata = fs::symlink_metadata(&current).map_err(|error| {
                    format!(
                        "Created project directory is unavailable {}: {error}",
                        current.display()
                    )
                })?;
                validate_directory_metadata(&canonical_root, &current, &metadata)?;
            }
            Err(error) => {
                return Err(format!(
                    "Project directory is unavailable {}: {error}",
                    current.display()
                ));
            }
        }
    }

    // Re-walk immediately after creation. This catches an intermediate
    // component replaced while the suffix was being built. A later attacker
    // can still race a subsequent path-based open; callers must revalidate as
    // close to that open as possible and must not claim an atomic no-follow
    // guarantee from this helper alone.
    validate_directory_components(&canonical_root, &relative, false)?;
    Ok((canonical_root.join(relative), created))
}

fn canonical_root_and_relative(
    project_root: &Path,
    directory: &Path,
) -> Result<(PathBuf, PathBuf), String> {
    let canonical_root = project_root
        .canonicalize()
        .map_err(|error| format!("Project root is unavailable: {error}"))?;
    let candidate = if directory.is_absolute() {
        directory
            .strip_prefix(project_root)
            .or_else(|_| directory.strip_prefix(&canonical_root))
            .map_err(|_| "Project directory is outside the project root".to_string())?
    } else {
        directory
    };

    let mut relative = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::Normal(segment) => relative.push(segment),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err("Project directory contains an unsafe path component".into());
            }
        }
    }
    if relative.as_os_str().is_empty() {
        return Err("Project directory must be below the project root".into());
    }
    Ok((canonical_root, relative))
}

fn validate_directory_components(
    canonical_root: &Path,
    relative: &Path,
    allow_missing_suffix: bool,
) -> Result<(), String> {
    let mut current = canonical_root.to_path_buf();
    let mut missing = false;
    for component in relative.components() {
        let Component::Normal(segment) = component else {
            return Err("Project directory contains an unsafe path component".into());
        };
        current.push(segment);
        if missing {
            continue;
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) => validate_directory_metadata(canonical_root, &current, &metadata)?,
            Err(error) if allow_missing_suffix && error.kind() == ErrorKind::NotFound => {
                missing = true;
            }
            Err(error) => {
                return Err(format!(
                    "Project directory is unavailable {}: {error}",
                    current.display()
                ));
            }
        }
    }
    Ok(())
}

fn validate_directory_metadata(
    canonical_root: &Path,
    path: &Path,
    metadata: &fs::Metadata,
) -> Result<(), String> {
    if metadata_is_link_or_reparse(metadata) {
        return Err(format!(
            "Project directory contains a link or reparse point: {}",
            path.display()
        ));
    }
    if !metadata.is_dir() {
        return Err(format!(
            "Project directory component is not a directory: {}",
            path.display()
        ));
    }
    let canonical = path.canonicalize().map_err(|error| {
        format!(
            "Project directory component is unavailable {}: {error}",
            path.display()
        )
    })?;
    if !canonical.starts_with(canonical_root) {
        return Err("Project directory resolves outside the project root".into());
    }
    Ok(())
}

fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink() || metadata_is_reparse_point(metadata)
}

#[cfg(windows)]
fn metadata_is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn metadata_is_reparse_point(_: &fs::Metadata) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_project_directory, validate_existing_project_directory, validate_project_directory,
    };
    use std::path::Path;

    #[test]
    fn creates_cjk_directory_components_and_revalidates_them() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join(".app").join("任务").join("持久化");

        let created = ensure_project_directory(root.path(), &target).unwrap();

        assert!(created.is_dir());
        assert_eq!(
            validate_existing_project_directory(root.path(), &target).unwrap(),
            created
        );
    }

    #[test]
    fn rejects_parent_components_and_the_project_root_itself() {
        let root = tempfile::tempdir().unwrap();

        assert!(validate_project_directory(root.path(), Path::new("../outside")).is_err());
        assert!(validate_project_directory(root.path(), root.path()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn canonicalizes_a_symlink_used_as_the_project_root() {
        use std::os::unix::fs::symlink;

        let target = tempfile::tempdir().unwrap();
        let parent = tempfile::tempdir().unwrap();
        let linked_root = parent.path().join("linked-project");
        symlink(target.path(), &linked_root).unwrap();

        assert_eq!(
            super::validate_existing_project_root(&linked_root).unwrap(),
            target.path().canonicalize().unwrap()
        );
    }

    #[cfg(windows)]
    #[test]
    fn canonicalizes_a_junction_used_as_the_project_root() {
        let target = tempfile::tempdir().unwrap();
        let parent = tempfile::tempdir().unwrap();
        let linked_root = parent.path().join("linked-project");
        let status = std::process::Command::new("cmd")
            .args([
                "/C",
                "mklink",
                "/J",
                linked_root.to_string_lossy().as_ref(),
                target.path().to_string_lossy().as_ref(),
            ])
            .status()
            .unwrap();
        assert!(status.success(), "junction setup failed");

        assert_eq!(
            super::validate_existing_project_root(&linked_root).unwrap(),
            target.path().canonicalize().unwrap()
        );
        std::fs::remove_dir(linked_root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn accepts_windows_and_posix_separator_styles() {
        let root = tempfile::tempdir().unwrap();

        let windows = ensure_project_directory(root.path(), Path::new(r".app\tasks-win")).unwrap();
        let posix = ensure_project_directory(root.path(), Path::new(".app/tasks-posix")).unwrap();

        assert!(windows.is_dir());
        assert!(posix.is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_replacement_between_validation_and_creation() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let app = root.path().join(".app");
        std::fs::create_dir(&app).unwrap();
        let target = app.join("tasks");
        validate_project_directory(root.path(), &target).unwrap();

        std::fs::remove_dir(&app).unwrap();
        symlink(outside.path(), &app).unwrap();

        assert!(ensure_project_directory(root.path(), &target).is_err());
        assert!(!outside.path().join("tasks").exists());
        std::fs::remove_file(app).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn rejects_junction_replacement_between_validation_and_creation() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let app = root.path().join(".app");
        std::fs::create_dir(&app).unwrap();
        let target = app.join("tasks");
        validate_project_directory(root.path(), &target).unwrap();

        std::fs::remove_dir(&app).unwrap();
        let status = std::process::Command::new("cmd")
            .args([
                "/C",
                "mklink",
                "/J",
                app.to_string_lossy().as_ref(),
                outside.path().to_string_lossy().as_ref(),
            ])
            .status()
            .unwrap();
        assert!(status.success(), "junction setup failed");

        assert!(ensure_project_directory(root.path(), &target).is_err());
        assert!(!outside.path().join("tasks").exists());
        std::fs::remove_dir(app).unwrap();
    }
}
