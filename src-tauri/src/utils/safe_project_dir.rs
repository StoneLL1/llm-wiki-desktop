use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Stable identity for a regular file opened through a retained directory.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct BoundFileIdentity(pub(crate) u64, pub(crate) u64);

/// A project-owned parent directory retained for one mutation transaction.
///
/// Child names are resolved relative to `anchor`: Unix uses the `*at` family;
/// Windows uses `NtCreateFile`/`NtSetInformationFile` with this handle as
/// `RootDirectory`. The stored paths are diagnostics only, never the final
/// mutation authority.
pub(crate) struct BoundProjectMutationRoot {
    requested_parent: PathBuf,
    parent: PathBuf,
    anchor: File,
}

/// A directory created while walking to a mutation parent. Keeping both the
/// child and its parent open lets transaction rollback remove the exact empty
/// directory without resolving its path again.
pub(crate) struct BoundCreatedProjectDirectory {
    parent: File,
    directory: File,
    name: OsString,
    path: PathBuf,
}

#[cfg(all(test, unix))]
thread_local! {
    static BEFORE_BOUND_FINAL_MUTATION_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        std::cell::RefCell::new(None);
}

#[cfg(all(test, unix))]
fn set_before_bound_final_mutation_hook(hook: impl FnOnce() + 'static) {
    BEFORE_BOUND_FINAL_MUTATION_HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

#[cfg(all(test, unix))]
fn run_before_bound_final_mutation_hook() {
    BEFORE_BOUND_FINAL_MUTATION_HOOK.with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook();
        }
    });
}

impl BoundCreatedProjectDirectory {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn remove_if_empty(&self) -> io::Result<()> {
        delete_relative_open_directory(&self.parent, &self.directory, &self.name)
    }
}

impl BoundProjectMutationRoot {
    pub(crate) fn try_clone(&self) -> io::Result<Self> {
        Ok(Self {
            requested_parent: self.requested_parent.clone(),
            parent: self.parent.clone(),
            anchor: self.anchor.try_clone()?,
        })
    }

    pub(crate) fn bind(project_root: &Path, target: &Path) -> io::Result<Self> {
        Self::walk(project_root, target, false, true).map(|(binding, _)| binding)
    }

    pub(crate) fn bind_read(project_root: &Path, target: &Path) -> io::Result<Self> {
        Self::walk(project_root, target, false, false).map(|(binding, _)| binding)
    }

    pub(crate) fn ensure_and_bind(
        project_root: &Path,
        target: &Path,
    ) -> io::Result<(Self, Vec<BoundCreatedProjectDirectory>)> {
        Self::walk(project_root, target, true, true)
    }

    fn walk(
        project_root: &Path,
        target: &Path,
        create_missing: bool,
        mutation_access: bool,
    ) -> io::Result<(Self, Vec<BoundCreatedProjectDirectory>)> {
        let open_anchor = if mutation_access {
            open_directory_anchor
        } else {
            open_directory_anchor_read
        };
        let open_child = if mutation_access {
            open_directory_relative
        } else {
            open_directory_relative_read
        };
        let lexical_anchor = open_anchor(project_root)?;
        require_directory(&lexical_anchor)?;
        let canonical_root = project_root.canonicalize()?;
        let canonical_anchor = open_anchor(&canonical_root)?;
        require_directory(&canonical_anchor)?;
        if identity_from_file(&lexical_anchor)? != identity_from_file(&canonical_anchor)? {
            return Err(conflict("project root changed while it was being bound"));
        }
        let requested_parent = target
            .parent()
            .ok_or_else(|| unsafe_path("mutation target has no parent"))?
            .to_path_buf();
        let relative_parent = requested_parent
            .strip_prefix(project_root)
            .or_else(|_| requested_parent.strip_prefix(&canonical_root))
            .map_err(|_| unsafe_path("mutation parent is outside the project root"))?
            .to_path_buf();
        validate_relative(&relative_parent)?;

        let mut anchor = lexical_anchor;
        let mut requested_current = project_root.to_path_buf();
        let mut created = Vec::new();
        let traversal = (|| -> io::Result<()> {
            for component in relative_parent.components() {
                let Component::Normal(segment) = component else {
                    return Err(unsafe_path("mutation parent contains an unsafe component"));
                };
                requested_current.push(segment);
                let (child, was_created) = match open_child(&anchor, segment) {
                    Ok(child) => (child, false),
                    Err(error) if create_missing && error.kind() == io::ErrorKind::NotFound => {
                        match create_directory_relative(&anchor, segment) {
                            Ok(child) => (child, true),
                            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                                (open_child(&anchor, segment)?, false)
                            }
                            Err(error) => return Err(error),
                        }
                    }
                    Err(error) => return Err(error),
                };
                require_directory(&child)?;
                if was_created {
                    created.push(BoundCreatedProjectDirectory {
                        parent: anchor.try_clone()?,
                        directory: child.try_clone()?,
                        name: segment.to_os_string(),
                        path: requested_current.clone(),
                    });
                }
                anchor = child;
            }
            Ok(())
        })();
        if let Err(error) = traversal {
            for directory in created.iter().rev() {
                let _ = directory.remove_if_empty();
            }
            return Err(error);
        }

        Ok((
            Self {
                requested_parent,
                parent: canonical_root.join(&relative_parent),
                anchor,
            },
            created,
        ))
    }

    pub(crate) fn parent(&self) -> &Path {
        // Return the caller's lexical namespace for paths persisted in journals
        // and surfaced in diagnostics. `parent` itself may carry a Windows
        // extended-path prefix after canonicalization; child operations never
        // trust either path and remain relative to `anchor`.
        &self.requested_parent
    }

    pub(crate) fn read_regular(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.read_regular_with_identity(path)
            .map(|(bytes, _)| bytes)
    }

    pub(crate) fn read_regular_with_identity(
        &self,
        path: &Path,
    ) -> io::Result<(Vec<u8>, BoundFileIdentity)> {
        let name = self.entry_name(path)?;
        let mut file = open_existing_relative(&self.anchor, name, OpenPurpose::Read)?;
        require_regular(&file)?;
        let identity = identity_from_file(&file)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok((bytes, identity))
    }

    pub(crate) fn open_regular(&self, path: &Path) -> io::Result<File> {
        let name = self.entry_name(path)?;
        let file = open_existing_relative(&self.anchor, name, OpenPurpose::Read)?;
        require_regular(&file)?;
        Ok(file)
    }

    pub(crate) fn open_regular_pinned(&self, path: &Path) -> io::Result<File> {
        let name = self.entry_name(path)?;
        let file = open_existing_relative(&self.anchor, name, OpenPurpose::Pin)?;
        require_regular(&file)?;
        Ok(file)
    }

    /// Open an app-owned regular file for streaming mutation without resolving
    /// the final name through the ambient filesystem namespace.
    pub(crate) fn open_regular_mutate_or_create(
        &self,
        path: &Path,
        truncate: bool,
    ) -> io::Result<File> {
        let name = self.entry_name(path)?;
        let file = match open_existing_relative(&self.anchor, name, OpenPurpose::Mutate) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                create_new_relative(&self.anchor, name)?
            }
            Err(error) => return Err(error),
        };
        require_regular(&file)?;
        if truncate {
            file.set_len(0)?;
        }
        Ok(file)
    }

    pub(crate) fn create_regular_new(&self, path: &Path) -> io::Result<File> {
        let name = self.entry_name(path)?;
        let file = create_new_relative(&self.anchor, name)?;
        require_regular(&file)?;
        Ok(file)
    }

    pub(crate) fn create_directory(&self, path: &Path) -> io::Result<()> {
        let name = self.entry_name(path)?;
        let directory = create_directory_relative(&self.anchor, name)?;
        require_directory(&directory)?;
        self.sync()
    }

    pub(crate) fn file_identity(&self, path: &Path) -> io::Result<BoundFileIdentity> {
        let name = self.entry_name(path)?;
        let file = open_existing_relative(&self.anchor, name, OpenPurpose::ReadIdentity)?;
        require_regular(&file)?;
        identity_from_file(&file)
    }

    pub(crate) fn write_synced_temp(&self, target: &Path, bytes: &[u8]) -> io::Result<PathBuf> {
        self.create_synced_temp(target, |file| file.write_all(bytes))
    }

    pub(crate) fn copy_synced_temp(
        &self,
        target: &Path,
        source: &mut impl Read,
    ) -> io::Result<PathBuf> {
        self.create_synced_temp(target, |file| io::copy(source, file).map(|_| ()))
    }

    fn create_synced_temp(
        &self,
        target: &Path,
        mut populate: impl FnMut(&mut File) -> io::Result<()>,
    ) -> io::Result<PathBuf> {
        let target_name = self.entry_name(target)?;
        let prefix = target_name.to_string_lossy();
        for _ in 0..32 {
            let name = format!(".{prefix}.{}.tmp", uuid::Uuid::new_v4());
            match create_new_relative(&self.anchor, OsStr::new(&name)) {
                Ok(mut file) => {
                    require_regular(&file)?;
                    if let Err(error) = populate(&mut file).and_then(|()| file.sync_all()) {
                        let _ = delete_relative_open_file(&self.anchor, &file, OsStr::new(&name));
                        return Err(error);
                    }
                    return Ok(self.requested_parent.join(name));
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not reserve a unique bound temporary file",
        ))
    }

    pub(crate) fn write_atomic_replace(&self, target: &Path, bytes: &[u8]) -> io::Result<()> {
        let temporary = self.write_synced_temp(target, bytes)?;
        self.install_prepared(&temporary, target)
    }

    pub(crate) fn install_prepared(&self, temporary: &Path, target: &Path) -> io::Result<()> {
        let result = match self.file_identity(target) {
            Ok(_) => self.replace_existing(&temporary, target),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.publish_new(&temporary, target)
            }
            Err(error) => Err(error),
        };
        if result.is_err() {
            let _ = self.remove_file(&temporary);
        }
        result.and_then(|()| self.sync())
    }

    pub(crate) fn write_atomic_create_new(&self, target: &Path, bytes: &[u8]) -> io::Result<()> {
        let temporary = self.write_synced_temp(target, bytes)?;
        let result = self.publish_new(&temporary, target);
        if result.is_err() {
            let _ = self.remove_file(&temporary);
        }
        result.and_then(|()| self.sync())
    }

    pub(crate) fn replace_existing(&self, temporary: &Path, target: &Path) -> io::Result<()> {
        self.replace_existing_inner(temporary, target, None)
    }

    pub(crate) fn replace_existing_if_identity(
        &self,
        temporary: &Path,
        target: &Path,
        expected: BoundFileIdentity,
    ) -> io::Result<()> {
        self.replace_existing_inner(temporary, target, Some((expected, None)))
    }

    pub(crate) fn replace_existing_if_identity_and_hash(
        &self,
        temporary: &Path,
        target: &Path,
        expected: BoundFileIdentity,
        expected_sha256: &str,
    ) -> io::Result<()> {
        self.replace_existing_inner(temporary, target, Some((expected, Some(expected_sha256))))
    }

    fn replace_existing_inner(
        &self,
        temporary: &Path,
        target: &Path,
        expected: Option<(BoundFileIdentity, Option<&str>)>,
    ) -> io::Result<()> {
        let source_name = self.entry_name(temporary)?;
        let target_name = self.entry_name(target)?;
        let source = open_existing_relative(&self.anchor, source_name, OpenPurpose::Mutate)?;
        require_regular(&source)?;
        let mut displaced = open_existing_relative(&self.anchor, target_name, OpenPurpose::Mutate)?;
        require_regular(&displaced)?;
        if let Some((identity, expected_sha256)) = expected {
            if identity_from_file(&displaced)? != identity {
                return Err(conflict("mutation target identity changed"));
            }
            if let Some(expected_sha256) = expected_sha256 {
                let mut bytes = Vec::new();
                displaced.read_to_end(&mut bytes)?;
                if format!("{:x}", Sha256::digest(&bytes)) != expected_sha256 {
                    return Err(conflict("mutation target contents changed"));
                }
            }
        }
        #[cfg(all(test, unix))]
        run_before_bound_final_mutation_hook();
        #[cfg(windows)]
        if expected.is_none() {
            // NtSetInformationFile performs one handle-relative namespace
            // replacement. Unconditional callers intentionally replace
            // whichever regular file currently owns the name.
            drop(displaced);
            return rename_open_file(
                &self.anchor,
                &source,
                source_name,
                &self.anchor,
                target_name,
                true,
            );
        }
        #[cfg(unix)]
        {
            // Exchange preserves both names atomically. If a different inode
            // won the target name after validation, swap it straight back;
            // external content is never discarded.
            exchange_file_names(&self.anchor, source_name, &self.anchor, target_name)?;
            let mut swapped =
                open_existing_relative(&self.anchor, source_name, OpenPurpose::Mutate)?;
            let identity_matches = identity_from_file(&swapped)? == identity_from_file(&displaced)?;
            let hash_matches = if let Some((_, Some(expected_sha256))) = expected {
                let mut bytes = Vec::new();
                swapped.read_to_end(&mut bytes)?;
                format!("{:x}", Sha256::digest(&bytes)) == expected_sha256
            } else {
                true
            };
            if !identity_matches || !hash_matches {
                let _ = exchange_file_names(&self.anchor, source_name, &self.anchor, target_name);
                return Err(conflict(
                    "mutation target identity or contents changed during atomic exchange",
                ));
            }
            delete_relative_open_file(&self.anchor, &displaced, source_name)
        }
        #[cfg(all(windows, not(unix)))]
        {
            // Windows cannot exchange two names. Move the exact pinned target
            // handle to a recovery name, then publish the prepared file with
            // no-replace semantics. This keeps a concurrent creator intact;
            // FileTransaction journals the recovery name before installation.
            let guard_name = format!(".wiki-bound-guard-{}", uuid::Uuid::new_v4());
            let guard = OsStr::new(&guard_name);
            rename_open_file(
                &self.anchor,
                &displaced,
                target_name,
                &self.anchor,
                guard,
                false,
            )
            .map_err(|error| contextual_io("quarantine verified target", error))?;
            if let Err(error) = publish_open_file_no_replace(
                &self.anchor,
                &source,
                source_name,
                &self.anchor,
                target_name,
            ) {
                if rename_open_file(
                    &self.anchor,
                    &displaced,
                    guard,
                    &self.anchor,
                    target_name,
                    false,
                )
                .is_err()
                {
                    return Err(conflict(&format!(
                        "replacement was blocked and the original remains in {guard_name}: {error}"
                    )));
                }
                return Err(error);
            }
            delete_relative_open_file(&self.anchor, &displaced, guard)
                .map_err(|error| contextual_io("remove quarantined target", error))
        }
    }

    pub(crate) fn publish_new(&self, temporary: &Path, target: &Path) -> io::Result<()> {
        let source_name = self.entry_name(temporary)?;
        let target_name = self.entry_name(target)?;
        let source = open_existing_relative(&self.anchor, source_name, OpenPurpose::Mutate)?;
        require_regular(&source)?;
        publish_open_file_no_replace(
            &self.anchor,
            &source,
            source_name,
            &self.anchor,
            target_name,
        )
    }

    pub(crate) fn hard_link(&self, source: &Path, target: &Path) -> io::Result<()> {
        let source_name = self.entry_name(source)?;
        let target_name = self.entry_name(target)?;
        let source = open_existing_relative(&self.anchor, source_name, OpenPurpose::Mutate)?;
        require_regular(&source)?;
        hard_link_open_file(
            &self.anchor,
            &source,
            source_name,
            &self.anchor,
            target_name,
        )
    }

    pub(crate) fn rename(&self, source: &Path, target: &Path, replace: bool) -> io::Result<()> {
        self.rename_to(source, self, target, replace)
    }

    pub(crate) fn rename_to(
        &self,
        source: &Path,
        destination: &Self,
        target: &Path,
        replace: bool,
    ) -> io::Result<()> {
        let source_name = self.entry_name(source)?;
        let target_name = destination.entry_name(target)?;
        let source_file = open_existing_relative(&self.anchor, source_name, OpenPurpose::Mutate)?;
        require_regular(&source_file)?;
        if replace {
            rename_open_file(
                &self.anchor,
                &source_file,
                source_name,
                &destination.anchor,
                target_name,
                true,
            )?;
        } else {
            publish_open_file_no_replace(
                &self.anchor,
                &source_file,
                source_name,
                &destination.anchor,
                target_name,
            )?;
        }
        #[cfg(not(windows))]
        {
            let renamed =
                open_existing_relative(&destination.anchor, target_name, OpenPurpose::Mutate)?;
            if identity_from_file(&renamed)? != identity_from_file(&source_file)? {
                let _ = rename_open_file(
                    &destination.anchor,
                    &renamed,
                    target_name,
                    &self.anchor,
                    source_name,
                    false,
                );
                return Err(conflict(
                    "rename source changed during final namespace update",
                ));
            }
        }
        self.sync()?;
        if self.parent != destination.parent {
            destination.sync()?;
        }
        Ok(())
    }

    pub(crate) fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.remove_file_inner(path, None)
    }

    pub(crate) fn rename_directory_no_replace(
        &self,
        source: &Path,
        target: &Path,
    ) -> io::Result<()> {
        self.rename_directory_to_no_replace(source, self, target)
    }

    pub(crate) fn rename_directory_to_no_replace(
        &self,
        source: &Path,
        destination: &Self,
        target: &Path,
    ) -> io::Result<()> {
        let source_name = self.entry_name(source)?;
        let target_name = destination.entry_name(target)?;
        let directory = open_directory_relative_mutate(&self.anchor, source_name)?;
        require_directory(&directory)?;
        rename_open_file(
            &self.anchor,
            &directory,
            source_name,
            &destination.anchor,
            target_name,
            false,
        )?;
        let renamed = open_directory_relative_mutate(&destination.anchor, target_name)?;
        if identity_from_file(&renamed)? != identity_from_file(&directory)? {
            let _ = rename_open_file(
                &destination.anchor,
                &renamed,
                target_name,
                &self.anchor,
                source_name,
                false,
            );
            return Err(conflict("directory changed during rename"));
        }
        self.sync()?;
        if self.parent != destination.parent {
            destination.sync()?;
        }
        Ok(())
    }

    pub(crate) fn remove_empty_directory(&self, path: &Path) -> io::Result<()> {
        let name = self.entry_name(path)?;
        let directory = open_directory_relative_mutate(&self.anchor, name)?;
        require_directory(&directory)?;
        delete_relative_open_directory(&self.anchor, &directory, name)?;
        self.sync()
    }

    /// Remove an app-owned directory tree only after claiming the exact
    /// directory through the retained parent. The public name disappears
    /// before traversal, so a replacement at that name is never consumed by
    /// cleanup.
    pub(crate) fn remove_directory_tree(&self, path: &Path) -> io::Result<()> {
        let name = self.entry_name(path)?;
        let directory = open_directory_relative_mutate(&self.anchor, name)?;
        require_directory(&directory)?;
        let quarantine_name = format!(".wiki-bound-tree-{}", uuid::Uuid::new_v4());
        let quarantine = OsStr::new(&quarantine_name);
        rename_open_file(
            &self.anchor,
            &directory,
            name,
            &self.anchor,
            quarantine,
            false,
        )?;

        #[cfg(unix)]
        {
            if let Err(error) = remove_directory_tree_contents(&directory)
                .and_then(|()| delete_relative_open_directory(&self.anchor, &directory, quarantine))
            {
                return Err(contextual_io(
                    &format!("remove quarantined directory tree {quarantine_name}"),
                    error,
                ));
            }
        }
        #[cfg(windows)]
        {
            // The retained Windows parent denies delete sharing, so its
            // lexical namespace cannot be replaced while this cleanup runs.
            // Rust's remove_dir_all does not follow reparse points and the
            // unpredictable quarantine name is no longer externally owned.
            let quarantined = self.requested_parent.join(&quarantine_name);
            std::fs::remove_dir_all(&quarantined).map_err(|error| {
                contextual_io(
                    &format!("remove quarantined directory tree {quarantine_name}"),
                    error,
                )
            })?;
        }
        self.sync()
    }

    pub(crate) fn remove_file_if_identity(
        &self,
        path: &Path,
        expected: BoundFileIdentity,
    ) -> io::Result<()> {
        self.remove_file_inner(path, Some((expected, None)))
    }

    pub(crate) fn remove_file_if_identity_and_hash(
        &self,
        path: &Path,
        expected: BoundFileIdentity,
        expected_sha256: &str,
    ) -> io::Result<()> {
        self.remove_file_inner(path, Some((expected, Some(expected_sha256))))
    }

    fn remove_file_inner(
        &self,
        path: &Path,
        expected: Option<(BoundFileIdentity, Option<&str>)>,
    ) -> io::Result<()> {
        let name = self.entry_name(path)?;
        let mut file = open_existing_relative(&self.anchor, name, OpenPurpose::Mutate)?;
        require_regular(&file)?;
        if let Some((identity, expected_sha256)) = expected {
            if identity_from_file(&file)? != identity {
                return Err(conflict("mutation target identity changed"));
            }
            if let Some(expected_sha256) = expected_sha256 {
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes)?;
                if format!("{:x}", Sha256::digest(&bytes)) != expected_sha256 {
                    return Err(conflict("mutation target contents changed"));
                }
            }
        }
        #[cfg(all(test, unix))]
        run_before_bound_final_mutation_hook();
        #[cfg(unix)]
        {
            let guard_name = format!(".wiki-bound-delete-{}", uuid::Uuid::new_v4());
            let guard = OsStr::new(&guard_name);
            rename_open_file(&self.anchor, &file, name, &self.anchor, guard, false)?;
            let mut moved = open_existing_relative(&self.anchor, guard, OpenPurpose::Mutate)?;
            let identity_matches = identity_from_file(&moved)? == identity_from_file(&file)?;
            let hash_matches = if let Some((_, Some(expected_sha256))) = expected {
                let mut bytes = Vec::new();
                moved.read_to_end(&mut bytes)?;
                format!("{:x}", Sha256::digest(&bytes)) == expected_sha256
            } else {
                true
            };
            if !identity_matches || !hash_matches {
                let _ = rename_open_file(&self.anchor, &moved, guard, &self.anchor, name, false);
                return Err(conflict(
                    "mutation target identity or contents changed during delete quarantine",
                ));
            }
            delete_relative_open_file(&self.anchor, &moved, guard)?;
        }
        #[cfg(not(unix))]
        delete_relative_open_file(&self.anchor, &file, name)?;
        self.sync()
    }

    pub(crate) fn sync(&self) -> io::Result<()> {
        self.anchor.sync_all()
    }

    fn entry_name<'path>(&self, path: &'path Path) -> io::Result<&'path OsStr> {
        let parent = path
            .parent()
            .ok_or_else(|| unsafe_path("mutation entry has no parent"))?;
        if parent != self.parent && parent != self.requested_parent {
            return Err(unsafe_path(
                "mutation entry does not belong to the bound parent",
            ));
        }
        let name = path
            .file_name()
            .ok_or_else(|| unsafe_path("mutation entry has no file name"))?;
        validate_single_name(name)?;
        Ok(name)
    }
}

pub(crate) fn remove_project_file(project_root: &Path, path: &Path) -> io::Result<()> {
    BoundProjectMutationRoot::bind(project_root, path)?.remove_file(path)
}

pub(crate) fn rename_project_file(
    project_root: &Path,
    source: &Path,
    target: &Path,
    replace: bool,
) -> io::Result<()> {
    if source.parent() == target.parent() {
        let binding = BoundProjectMutationRoot::bind(project_root, source)?;
        return binding.rename_to(source, &binding, target, replace);
    }
    let source_binding = BoundProjectMutationRoot::bind(project_root, source)?;
    let (target_binding, _) = BoundProjectMutationRoot::ensure_and_bind(project_root, target)?;
    source_binding.rename_to(source, &target_binding, target, replace)
}

fn validate_relative(path: &Path) -> io::Result<()> {
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(unsafe_path("path contains an unsafe component"));
        }
    }
    Ok(())
}

fn validate_single_name(name: &OsStr) -> io::Result<()> {
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(unsafe_path("entry name is not a single normal component"));
    }
    Ok(())
}

fn require_directory(file: &File) -> io::Result<()> {
    let metadata = file.metadata()?;
    if !metadata.is_dir() || metadata_is_link_or_reparse(&metadata) {
        return Err(unsafe_path("bound entry is not a regular directory"));
    }
    Ok(())
}

fn require_regular(file: &File) -> io::Result<()> {
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata_is_link_or_reparse(&metadata) {
        return Err(unsafe_path("bound entry is not a regular file"));
    }
    Ok(())
}

fn unsafe_path(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

fn conflict(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::WouldBlock, message.into())
}

fn contextual_io(operation: &str, error: io::Error) -> io::Error {
    io::Error::new(error.kind(), format!("{operation}: {error}"))
}

fn metadata_is_link_or_reparse(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink() || metadata_is_reparse(metadata)
}

#[cfg(windows)]
fn metadata_is_reparse(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes()
        & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
        != 0
}

#[cfg(not(windows))]
fn metadata_is_reparse(_: &std::fs::Metadata) -> bool {
    false
}

#[derive(Clone, Copy)]
enum OpenPurpose {
    Read,
    ReadIdentity,
    Pin,
    Mutate,
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    pub(super) fn open_directory_anchor(path: &Path) -> io::Result<File> {
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
    }

    pub(super) fn open_directory_anchor_read(path: &Path) -> io::Result<File> {
        open_directory_anchor(path)
    }

    pub(super) fn open_directory_relative(parent: &File, name: &OsStr) -> io::Result<File> {
        let name = std::ffi::CString::new(name.as_bytes())?;
        // SAFETY: parent and the single-component C string remain valid; the
        // returned descriptor is transferred exactly once.
        let descriptor = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0,
            )
        };
        if descriptor < 0 {
            Err(io::Error::last_os_error())
        } else {
            // SAFETY: openat returned a new owned descriptor.
            Ok(unsafe { File::from_raw_fd(descriptor) })
        }
    }

    pub(super) fn open_directory_relative_read(parent: &File, name: &OsStr) -> io::Result<File> {
        open_directory_relative(parent, name)
    }

    pub(super) fn open_directory_relative_mutate(parent: &File, name: &OsStr) -> io::Result<File> {
        open_directory_relative(parent, name)
    }

    pub(super) fn create_directory_relative(parent: &File, name: &OsStr) -> io::Result<File> {
        let name_c = std::ffi::CString::new(name.as_bytes())?;
        // SAFETY: parent and the single-component C string remain valid.
        let result = unsafe { libc::mkdirat(parent.as_raw_fd(), name_c.as_ptr(), 0o777) };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        open_directory_relative(parent, name)
    }

    pub(super) fn open_existing_relative(
        parent: &File,
        name: &OsStr,
        _purpose: OpenPurpose,
    ) -> io::Result<File> {
        let name = std::ffi::CString::new(name.as_bytes())?;
        // SAFETY: parent and the single-component C string remain valid; the
        // returned descriptor is transferred exactly once.
        let descriptor = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0,
            )
        };
        if descriptor < 0 {
            Err(io::Error::last_os_error())
        } else {
            // SAFETY: openat returned a new owned descriptor.
            Ok(unsafe { File::from_raw_fd(descriptor) })
        }
    }

    pub(super) fn create_new_relative(parent: &File, name: &OsStr) -> io::Result<File> {
        let name = std::ffi::CString::new(name.as_bytes())?;
        // SAFETY: parent and name are valid; O_EXCL rejects every existing
        // filesystem object and the descriptor is owned by the result.
        let descriptor = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o666,
            )
        };
        if descriptor < 0 {
            Err(io::Error::last_os_error())
        } else {
            // SAFETY: openat returned a new owned descriptor.
            Ok(unsafe { File::from_raw_fd(descriptor) })
        }
    }

    pub(super) fn rename_open_file(
        source_parent: &File,
        source: &File,
        source_name: &OsStr,
        destination_parent: &File,
        destination_name: &OsStr,
        replace: bool,
    ) -> io::Result<()> {
        verify_named_identity(source_parent, source_name, source)?;
        if !replace {
            return rename_no_replace(
                source_parent,
                source_name,
                destination_parent,
                destination_name,
            );
        }
        let source_name = std::ffi::CString::new(source_name.as_bytes())?;
        let destination_name = std::ffi::CString::new(destination_name.as_bytes())?;
        // SAFETY: both retained dirfds and both names remain live.
        let result = unsafe {
            libc::renameat(
                source_parent.as_raw_fd(),
                source_name.as_ptr(),
                destination_parent.as_raw_fd(),
                destination_name.as_ptr(),
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub(super) fn publish_open_file_no_replace(
        source_parent: &File,
        source: &File,
        source_name: &OsStr,
        destination_parent: &File,
        destination_name: &OsStr,
    ) -> io::Result<()> {
        verify_named_identity(source_parent, source_name, source)?;
        rename_no_replace(
            source_parent,
            source_name,
            destination_parent,
            destination_name,
        )
    }

    #[cfg(target_os = "linux")]
    fn rename_no_replace(
        source_parent: &File,
        source_name: &OsStr,
        destination_parent: &File,
        destination_name: &OsStr,
    ) -> io::Result<()> {
        let source_name = std::ffi::CString::new(source_name.as_bytes())?;
        let destination_name = std::ffi::CString::new(destination_name.as_bytes())?;
        // SAFETY: both retained descriptors and component names remain live.
        let result = unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                source_parent.as_raw_fd(),
                source_name.as_ptr(),
                destination_parent.as_raw_fd(),
                destination_name.as_ptr(),
                1_u32, // RENAME_NOREPLACE
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    #[cfg(target_os = "linux")]
    pub(super) fn exchange_file_names(
        source_parent: &File,
        source_name: &OsStr,
        destination_parent: &File,
        destination_name: &OsStr,
    ) -> io::Result<()> {
        let source_name = std::ffi::CString::new(source_name.as_bytes())?;
        let destination_name = std::ffi::CString::new(destination_name.as_bytes())?;
        // SAFETY: both retained descriptors and component names remain live.
        let result = unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                source_parent.as_raw_fd(),
                source_name.as_ptr(),
                destination_parent.as_raw_fd(),
                destination_name.as_ptr(),
                2_u32, // RENAME_EXCHANGE
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    #[cfg(target_os = "macos")]
    fn rename_no_replace(
        source_parent: &File,
        source_name: &OsStr,
        destination_parent: &File,
        destination_name: &OsStr,
    ) -> io::Result<()> {
        unsafe extern "C" {
            fn renameatx_np(
                fromfd: libc::c_int,
                from: *const libc::c_char,
                tofd: libc::c_int,
                to: *const libc::c_char,
                flags: libc::c_uint,
            ) -> libc::c_int;
        }
        let source_name = std::ffi::CString::new(source_name.as_bytes())?;
        let destination_name = std::ffi::CString::new(destination_name.as_bytes())?;
        // SAFETY: both retained descriptors and component names remain live.
        let result = unsafe {
            renameatx_np(
                source_parent.as_raw_fd(),
                source_name.as_ptr(),
                destination_parent.as_raw_fd(),
                destination_name.as_ptr(),
                0x0000_0004, // RENAME_EXCL
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    #[cfg(target_os = "macos")]
    pub(super) fn exchange_file_names(
        source_parent: &File,
        source_name: &OsStr,
        destination_parent: &File,
        destination_name: &OsStr,
    ) -> io::Result<()> {
        unsafe extern "C" {
            fn renameatx_np(
                fromfd: libc::c_int,
                from: *const libc::c_char,
                tofd: libc::c_int,
                to: *const libc::c_char,
                flags: libc::c_uint,
            ) -> libc::c_int;
        }
        let source_name = std::ffi::CString::new(source_name.as_bytes())?;
        let destination_name = std::ffi::CString::new(destination_name.as_bytes())?;
        // SAFETY: both retained descriptors and component names remain live.
        let result = unsafe {
            renameatx_np(
                source_parent.as_raw_fd(),
                source_name.as_ptr(),
                destination_parent.as_raw_fd(),
                destination_name.as_ptr(),
                0x0000_0002, // RENAME_SWAP
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub(super) fn exchange_file_names(
        _source_parent: &File,
        _source_name: &OsStr,
        _destination_parent: &File,
        _destination_name: &OsStr,
    ) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "atomic file exchange is unavailable on this Unix platform",
        ))
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    fn rename_no_replace(
        source_parent: &File,
        source_name: &OsStr,
        destination_parent: &File,
        destination_name: &OsStr,
    ) -> io::Result<()> {
        let source = open_existing_relative(source_parent, source_name, OpenPurpose::Mutate)?;
        hard_link_open_file(
            source_parent,
            &source,
            source_name,
            destination_parent,
            destination_name,
        )?;
        delete_relative_open_file(source_parent, &source, source_name)
    }

    pub(super) fn hard_link_open_file(
        source_parent: &File,
        source: &File,
        source_name: &OsStr,
        destination_parent: &File,
        destination_name: &OsStr,
    ) -> io::Result<()> {
        verify_named_identity(source_parent, source_name, source)?;
        let source_name = std::ffi::CString::new(source_name.as_bytes())?;
        let destination_name = std::ffi::CString::new(destination_name.as_bytes())?;
        // SAFETY: both retained dirfds and both names remain live; flags=0
        // neither follows a source symlink nor replaces a destination.
        let result = unsafe {
            libc::linkat(
                source_parent.as_raw_fd(),
                source_name.as_ptr(),
                destination_parent.as_raw_fd(),
                destination_name.as_ptr(),
                0,
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub(super) fn delete_relative_open_file(
        parent: &File,
        file: &File,
        name: &OsStr,
    ) -> io::Result<()> {
        verify_named_identity(parent, name, file)?;
        let name = std::ffi::CString::new(name.as_bytes())?;
        // SAFETY: the retained dirfd and name remain live; flags=0 deletes a
        // non-directory entry only.
        let result = unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), 0) };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub(super) fn delete_relative_open_directory(
        parent: &File,
        directory: &File,
        name: &OsStr,
    ) -> io::Result<()> {
        verify_named_identity(parent, name, directory)?;
        let name = std::ffi::CString::new(name.as_bytes())?;
        // SAFETY: the retained dirfd and name remain live; AT_REMOVEDIR refuses
        // files and non-empty directories.
        let result =
            unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), libc::AT_REMOVEDIR) };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub(super) fn remove_directory_tree_contents(directory: &File) -> io::Result<()> {
        struct DirectoryStream(*mut libc::DIR);
        impl Drop for DirectoryStream {
            fn drop(&mut self) {
                // SAFETY: fdopendir returned this stream and it is closed once.
                unsafe { libc::closedir(self.0) };
            }
        }

        // fdopendir owns its descriptor, so duplicate the retained directory
        // handle and keep the original alive for child-relative mutation.
        let duplicate = unsafe { libc::dup(directory.as_raw_fd()) };
        if duplicate < 0 {
            return Err(io::Error::last_os_error());
        }
        let stream = unsafe { libc::fdopendir(duplicate) };
        if stream.is_null() {
            let error = io::Error::last_os_error();
            unsafe { libc::close(duplicate) };
            return Err(error);
        }
        let stream = DirectoryStream(stream);
        loop {
            let entry = unsafe { libc::readdir(stream.0) };
            if entry.is_null() {
                break;
            }
            let name = unsafe { std::ffi::CStr::from_ptr((*entry).d_name.as_ptr()) };
            if name.to_bytes() == b"." || name.to_bytes() == b".." {
                continue;
            }
            let name = OsStr::from_bytes(name.to_bytes());
            match open_directory_relative_mutate(directory, name) {
                Ok(child) => {
                    require_directory(&child)?;
                    remove_directory_tree_contents(&child)?;
                    delete_relative_open_directory(directory, &child, name)?;
                }
                Err(error)
                    if matches!(
                        error.raw_os_error(),
                        Some(libc::ENOENT) | Some(libc::ENOTDIR)
                    ) =>
                {
                    match open_existing_relative(directory, name, OpenPurpose::Mutate) {
                        Ok(child) => {
                            require_regular(&child)?;
                            delete_relative_open_file(directory, &child, name)?;
                        }
                        Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                        Err(error) => return Err(error),
                    }
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    fn verify_named_identity(parent: &File, name: &OsStr, opened: &File) -> io::Result<()> {
        let reopened = open_existing_relative(parent, name, OpenPurpose::ReadIdentity)?;
        if identity_from_file(&reopened)? == identity_from_file(opened)? {
            Ok(())
        } else {
            Err(conflict("bound entry changed before mutation"))
        }
    }

    pub(super) fn identity_from_file(file: &File) -> io::Result<BoundFileIdentity> {
        let metadata = file.metadata()?;
        Ok(BoundFileIdentity(metadata.dev(), metadata.ino()))
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
    use windows_sys::Wdk::Storage::FileSystem::{
        FileDispositionInformation, FileDispositionInformationEx, FileLinkInformation,
        FileRenameInformation, FileRenameInformationEx, NtCreateFile, NtSetInformationFile,
        FILE_CREATE, FILE_DIRECTORY_FILE, FILE_DISPOSITION_DELETE,
        FILE_DISPOSITION_IGNORE_READONLY_ATTRIBUTE, FILE_DISPOSITION_INFORMATION,
        FILE_DISPOSITION_INFORMATION_EX, FILE_DISPOSITION_POSIX_SEMANTICS, FILE_NON_DIRECTORY_FILE,
        FILE_OPEN, FILE_OPEN_REPARSE_POINT, FILE_RENAME_INFORMATION, FILE_RENAME_POSIX_SEMANTICS,
        FILE_RENAME_REPLACE_IF_EXISTS, FILE_SYNCHRONOUS_IO_NONALERT,
    };
    use windows_sys::Win32::Foundation::{
        RtlNtStatusToDosError, HANDLE, OBJ_CASE_INSENSITIVE, OBJ_DONT_REPARSE, UNICODE_STRING,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, DELETE, FILE_ADD_FILE,
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_READ_DATA,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_DATA, SYNCHRONIZE,
    };
    use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

    pub(super) fn open_directory_anchor(path: &Path) -> io::Result<File> {
        std::fs::OpenOptions::new()
            .access_mode(FILE_LIST_DIRECTORY | FILE_ADD_FILE | FILE_READ_ATTRIBUTES | SYNCHRONIZE)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
    }

    pub(super) fn open_directory_anchor_read(path: &Path) -> io::Result<File> {
        std::fs::OpenOptions::new()
            .access_mode(FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
    }

    pub(super) fn open_directory_relative(parent: &File, name: &OsStr) -> io::Result<File> {
        nt_open_directory_relative(
            parent,
            name,
            FILE_LIST_DIRECTORY | FILE_ADD_FILE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_OPEN,
        )
    }

    pub(super) fn open_directory_relative_mutate(parent: &File, name: &OsStr) -> io::Result<File> {
        nt_open_directory_relative(
            parent,
            name,
            FILE_LIST_DIRECTORY | FILE_ADD_FILE | FILE_READ_ATTRIBUTES | DELETE | SYNCHRONIZE,
            // The mutation is issued through this exact handle. Allowing
            // delete sharing lets NtSetInformationFile rename/dispose the
            // opened directory without weakening the retained parent anchor.
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_OPEN,
        )
    }

    pub(super) fn open_directory_relative_read(parent: &File, name: &OsStr) -> io::Result<File> {
        nt_open_directory_relative(
            parent,
            name,
            FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_OPEN,
        )
    }

    pub(super) fn create_directory_relative(parent: &File, name: &OsStr) -> io::Result<File> {
        nt_open_directory_relative(
            parent,
            name,
            FILE_LIST_DIRECTORY | FILE_ADD_FILE | FILE_READ_ATTRIBUTES | DELETE | SYNCHRONIZE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_CREATE,
        )
    }

    pub(super) fn open_existing_relative(
        parent: &File,
        name: &OsStr,
        purpose: OpenPurpose,
    ) -> io::Result<File> {
        let (desired, share_mode) = match purpose {
            OpenPurpose::Read => (
                FILE_READ_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ),
            OpenPurpose::ReadIdentity => (
                FILE_READ_ATTRIBUTES | SYNCHRONIZE,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ),
            OpenPurpose::Pin => (
                FILE_READ_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
                FILE_SHARE_READ,
            ),
            OpenPurpose::Mutate => (
                FILE_READ_DATA | FILE_WRITE_DATA | FILE_READ_ATTRIBUTES | DELETE | SYNCHRONIZE,
                FILE_SHARE_READ,
            ),
        };
        nt_open_relative(parent, name, desired, share_mode, FILE_OPEN)
    }

    pub(super) fn create_new_relative(parent: &File, name: &OsStr) -> io::Result<File> {
        nt_open_relative(
            parent,
            name,
            FILE_READ_DATA | FILE_WRITE_DATA | FILE_READ_ATTRIBUTES | DELETE | SYNCHRONIZE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_CREATE,
        )
    }

    fn nt_open_relative(
        parent: &File,
        name: &OsStr,
        desired_access: u32,
        share_mode: u32,
        disposition: u32,
    ) -> io::Result<File> {
        validate_single_name(name)?;
        let mut wide = name.encode_wide().collect::<Vec<_>>();
        let byte_length = wide
            .len()
            .checked_mul(std::mem::size_of::<u16>())
            .and_then(|length| u16::try_from(length).ok())
            .ok_or_else(|| unsafe_path("Windows entry name is too long"))?;
        let unicode = UNICODE_STRING {
            Length: byte_length,
            MaximumLength: byte_length,
            Buffer: wide.as_mut_ptr(),
        };
        let attributes = OBJECT_ATTRIBUTES {
            Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
            RootDirectory: parent.as_raw_handle() as HANDLE,
            ObjectName: &unicode,
            Attributes: OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE,
            SecurityDescriptor: std::ptr::null(),
            SecurityQualityOfService: std::ptr::null(),
        };
        let mut status = IO_STATUS_BLOCK::default();
        let mut handle: HANDLE = std::ptr::null_mut();
        // SAFETY: every pointer remains live for the synchronous call; a
        // successful handle is transferred exactly once to File.
        let result = unsafe {
            NtCreateFile(
                &mut handle,
                desired_access,
                &attributes,
                &mut status,
                std::ptr::null(),
                FILE_ATTRIBUTE_NORMAL,
                share_mode,
                disposition,
                FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
                std::ptr::null(),
                0,
            )
        };
        if result < 0 {
            return Err(ntstatus_error(result));
        }
        if handle.is_null() {
            return Err(io::Error::other("NtCreateFile returned a null handle"));
        }
        // SAFETY: NtCreateFile returned a new owned handle.
        Ok(unsafe { File::from_raw_handle(handle.cast()) })
    }

    fn nt_open_directory_relative(
        parent: &File,
        name: &OsStr,
        desired_access: u32,
        share_mode: u32,
        disposition: u32,
    ) -> io::Result<File> {
        validate_single_name(name)?;
        let mut wide = name.encode_wide().collect::<Vec<_>>();
        let byte_length = wide
            .len()
            .checked_mul(std::mem::size_of::<u16>())
            .and_then(|length| u16::try_from(length).ok())
            .ok_or_else(|| unsafe_path("Windows entry name is too long"))?;
        let unicode = UNICODE_STRING {
            Length: byte_length,
            MaximumLength: byte_length,
            Buffer: wide.as_mut_ptr(),
        };
        let attributes = OBJECT_ATTRIBUTES {
            Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
            RootDirectory: parent.as_raw_handle() as HANDLE,
            ObjectName: &unicode,
            Attributes: OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE,
            SecurityDescriptor: std::ptr::null(),
            SecurityQualityOfService: std::ptr::null(),
        };
        let mut status = IO_STATUS_BLOCK::default();
        let mut handle: HANDLE = std::ptr::null_mut();
        // SAFETY: every pointer remains live for the synchronous call; a
        // successful handle is transferred exactly once to File.
        let result = unsafe {
            NtCreateFile(
                &mut handle,
                desired_access,
                &attributes,
                &mut status,
                std::ptr::null(),
                FILE_ATTRIBUTE_DIRECTORY,
                share_mode,
                disposition,
                FILE_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
                std::ptr::null(),
                0,
            )
        };
        if result < 0 {
            return Err(ntstatus_error(result));
        }
        if handle.is_null() {
            return Err(io::Error::other("NtCreateFile returned a null handle"));
        }
        // SAFETY: NtCreateFile returned a new owned handle.
        Ok(unsafe { File::from_raw_handle(handle.cast()) })
    }

    pub(super) fn rename_open_file(
        _source_parent: &File,
        source: &File,
        _source_name: &OsStr,
        destination_parent: &File,
        destination_name: &OsStr,
        replace: bool,
    ) -> io::Result<()> {
        let flags = FILE_RENAME_POSIX_SEMANTICS
            | if replace {
                FILE_RENAME_REPLACE_IF_EXISTS
            } else {
                0
            };
        let mut buffer = NameInformationBuffer::new(destination_parent, destination_name, flags)?;
        match nt_set_information(source, &mut buffer, FileRenameInformationEx) {
            Ok(()) => Ok(()),
            Err(error) if error.raw_os_error() == Some(87) => {
                let mut legacy = NameInformationBuffer::new(
                    destination_parent,
                    destination_name,
                    u32::from(replace),
                )?;
                nt_set_information(source, &mut legacy, FileRenameInformation)
            }
            Err(error) => Err(error),
        }
    }

    pub(super) fn publish_open_file_no_replace(
        source_parent: &File,
        source: &File,
        source_name: &OsStr,
        destination_parent: &File,
        destination_name: &OsStr,
    ) -> io::Result<()> {
        rename_open_file(
            source_parent,
            source,
            source_name,
            destination_parent,
            destination_name,
            false,
        )
    }

    pub(super) fn hard_link_open_file(
        _source_parent: &File,
        source: &File,
        _source_name: &OsStr,
        destination_parent: &File,
        destination_name: &OsStr,
    ) -> io::Result<()> {
        let mut buffer = NameInformationBuffer::new(destination_parent, destination_name, 0)?;
        nt_set_information(source, &mut buffer, FileLinkInformation)
    }

    pub(super) fn delete_relative_open_file(
        _parent: &File,
        file: &File,
        _name: &OsStr,
    ) -> io::Result<()> {
        let disposition = FILE_DISPOSITION_INFORMATION_EX {
            Flags: FILE_DISPOSITION_DELETE
                | FILE_DISPOSITION_POSIX_SEMANTICS
                | FILE_DISPOSITION_IGNORE_READONLY_ATTRIBUTE,
        };
        let mut buffer = AlignedBuffer::from_value(&disposition);
        match nt_set_information(file, &mut buffer, FileDispositionInformationEx) {
            Ok(()) => Ok(()),
            Err(error) if error.raw_os_error() == Some(87) => {
                let legacy = FILE_DISPOSITION_INFORMATION { DeleteFile: true };
                let mut buffer = AlignedBuffer::from_value(&legacy);
                nt_set_information(file, &mut buffer, FileDispositionInformation)
            }
            Err(error) => Err(error),
        }
    }

    pub(super) fn delete_relative_open_directory(
        _parent: &File,
        directory: &File,
        _name: &OsStr,
    ) -> io::Result<()> {
        delete_open_handle(directory)
    }

    fn delete_open_handle(file: &File) -> io::Result<()> {
        let disposition = FILE_DISPOSITION_INFORMATION_EX {
            Flags: FILE_DISPOSITION_DELETE
                | FILE_DISPOSITION_POSIX_SEMANTICS
                | FILE_DISPOSITION_IGNORE_READONLY_ATTRIBUTE,
        };
        let mut buffer = AlignedBuffer::from_value(&disposition);
        match nt_set_information(file, &mut buffer, FileDispositionInformationEx) {
            Ok(()) => Ok(()),
            Err(error) if error.raw_os_error() == Some(87) => {
                let legacy = FILE_DISPOSITION_INFORMATION { DeleteFile: true };
                let mut buffer = AlignedBuffer::from_value(&legacy);
                nt_set_information(file, &mut buffer, FileDispositionInformation)
            }
            Err(error) => Err(error),
        }
    }

    struct AlignedBuffer {
        words: Vec<usize>,
        length: usize,
    }

    impl AlignedBuffer {
        fn with_length(length: usize) -> Self {
            Self {
                words: vec![0; length.div_ceil(std::mem::size_of::<usize>())],
                length,
            }
        }

        fn from_value<T: Copy>(value: &T) -> Self {
            let mut buffer = Self::with_length(std::mem::size_of::<T>());
            // SAFETY: destination is aligned and has exactly enough storage.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    (value as *const T).cast::<u8>(),
                    buffer.words.as_mut_ptr().cast::<u8>(),
                    buffer.length,
                );
            }
            buffer
        }

        fn as_ptr(&self) -> *const std::ffi::c_void {
            self.words.as_ptr().cast()
        }
    }

    struct NameInformationBuffer(AlignedBuffer);

    impl NameInformationBuffer {
        fn new(parent: &File, name: &OsStr, flags: u32) -> io::Result<Self> {
            validate_single_name(name)?;
            let name = name.encode_wide().collect::<Vec<_>>();
            let name_offset = std::mem::offset_of!(FILE_RENAME_INFORMATION, FileName);
            let name_bytes = name
                .len()
                .checked_mul(std::mem::size_of::<u16>())
                .ok_or_else(|| unsafe_path("Windows name is too long"))?;
            let mut buffer = AlignedBuffer::with_length(
                name_offset
                    .checked_add(name_bytes)
                    .ok_or_else(|| unsafe_path("Windows information buffer is too large"))?,
            );
            // SAFETY: the aligned allocation includes the complete fixed
            // header and UTF-16 tail, and each field is initialized once.
            unsafe {
                let header = buffer.words.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
                (*header).Anonymous.Flags = flags;
                (*header).RootDirectory = parent.as_raw_handle() as HANDLE;
                (*header).FileNameLength = u32::try_from(name_bytes)
                    .map_err(|_| unsafe_path("Windows name is too long"))?;
                std::ptr::copy_nonoverlapping(
                    name.as_ptr().cast::<u8>(),
                    buffer.words.as_mut_ptr().cast::<u8>().add(name_offset),
                    name_bytes,
                );
            }
            Ok(Self(buffer))
        }
    }

    trait InformationBuffer {
        fn pointer(&self) -> *const std::ffi::c_void;
        fn length(&self) -> usize;
    }

    impl InformationBuffer for AlignedBuffer {
        fn pointer(&self) -> *const std::ffi::c_void {
            self.as_ptr()
        }
        fn length(&self) -> usize {
            self.length
        }
    }

    impl InformationBuffer for NameInformationBuffer {
        fn pointer(&self) -> *const std::ffi::c_void {
            self.0.as_ptr()
        }
        fn length(&self) -> usize {
            self.0.length
        }
    }

    fn nt_set_information(
        file: &File,
        buffer: &mut impl InformationBuffer,
        class: i32,
    ) -> io::Result<()> {
        let mut status = IO_STATUS_BLOCK::default();
        // SAFETY: file and the aligned buffer remain live for this synchronous call.
        let result = unsafe {
            NtSetInformationFile(
                file.as_raw_handle() as HANDLE,
                &mut status,
                buffer.pointer(),
                u32::try_from(buffer.length())
                    .map_err(|_| unsafe_path("Windows information buffer is too large"))?,
                class,
            )
        };
        if result < 0 {
            Err(ntstatus_error(result))
        } else {
            Ok(())
        }
    }

    fn ntstatus_error(status: i32) -> io::Error {
        // SAFETY: this conversion accepts every NTSTATUS and has no side effects.
        let code = unsafe { RtlNtStatusToDosError(status) };
        io::Error::from_raw_os_error(code as i32)
    }

    pub(super) fn identity_from_file(file: &File) -> io::Result<BoundFileIdentity> {
        let mut information = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: the file owns a live handle and information is valid output.
        let succeeded =
            unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), &mut information) };
        if succeeded == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(BoundFileIdentity(
            u64::from(information.dwVolumeSerialNumber),
            (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow),
        ))
    }
}

#[cfg(not(any(unix, windows)))]
mod platform {
    use super::*;

    fn unsupported() -> io::Error {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "handle-relative project mutations are unavailable on this platform",
        )
    }

    pub(super) fn open_directory_anchor(_path: &Path) -> io::Result<File> {
        Err(unsupported())
    }
    pub(super) fn open_directory_anchor_read(_path: &Path) -> io::Result<File> {
        Err(unsupported())
    }
    pub(super) fn open_directory_relative(_parent: &File, _name: &OsStr) -> io::Result<File> {
        Err(unsupported())
    }
    pub(super) fn open_directory_relative_read(_parent: &File, _name: &OsStr) -> io::Result<File> {
        Err(unsupported())
    }
    pub(super) fn open_directory_relative_mutate(
        _parent: &File,
        _name: &OsStr,
    ) -> io::Result<File> {
        Err(unsupported())
    }
    pub(super) fn create_directory_relative(_parent: &File, _name: &OsStr) -> io::Result<File> {
        Err(unsupported())
    }
    pub(super) fn open_existing_relative(
        _parent: &File,
        name: &OsStr,
        _purpose: OpenPurpose,
    ) -> io::Result<File> {
        Err(unsupported())
    }
    pub(super) fn create_new_relative(_parent: &File, _name: &OsStr) -> io::Result<File> {
        Err(unsupported())
    }
    pub(super) fn rename_open_file(
        _source_parent: &File,
        _source: &File,
        source_name: &OsStr,
        _destination_parent: &File,
        destination_name: &OsStr,
        _replace: bool,
    ) -> io::Result<()> {
        Err(unsupported())
    }
    pub(super) fn publish_open_file_no_replace(
        _source_parent: &File,
        _source: &File,
        source_name: &OsStr,
        _destination_parent: &File,
        destination_name: &OsStr,
    ) -> io::Result<()> {
        Err(unsupported())
    }
    pub(super) fn hard_link_open_file(
        _source_parent: &File,
        _source: &File,
        source_name: &OsStr,
        _destination_parent: &File,
        destination_name: &OsStr,
    ) -> io::Result<()> {
        Err(unsupported())
    }
    pub(super) fn delete_relative_open_file(
        _parent: &File,
        _file: &File,
        name: &OsStr,
    ) -> io::Result<()> {
        Err(unsupported())
    }
    pub(super) fn delete_relative_open_directory(
        _parent: &File,
        _directory: &File,
        _name: &OsStr,
    ) -> io::Result<()> {
        Err(unsupported())
    }
    pub(super) fn identity_from_file(_file: &File) -> io::Result<BoundFileIdentity> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "stable file identity is unavailable on this platform",
        ))
    }
}

use platform::{
    create_directory_relative, create_new_relative, delete_relative_open_directory,
    delete_relative_open_file, hard_link_open_file, identity_from_file, open_directory_anchor,
    open_directory_anchor_read, open_directory_relative, open_directory_relative_mutate,
    open_directory_relative_read, open_existing_relative, publish_open_file_no_replace,
    rename_open_file,
};
#[cfg(unix)]
use platform::{exchange_file_names, remove_directory_tree_contents};

#[cfg(test)]
mod tests {
    use super::BoundProjectMutationRoot;

    #[cfg(unix)]
    use super::set_before_bound_final_mutation_hook;

    #[test]
    fn create_overwrite_rename_delete_and_type_guards_are_handle_bound() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("wiki").join("CJK-知识");
        std::fs::create_dir_all(&folder).unwrap();
        let first = folder.join("first.md");
        let second = folder.join("second.md");
        let binding = BoundProjectMutationRoot::bind(root.path(), &first).unwrap();

        binding
            .write_atomic_create_new(&first, "初稿".as_bytes())
            .unwrap();
        assert_eq!(binding.read_regular(&first).unwrap(), "初稿".as_bytes());
        assert_eq!(
            binding
                .write_atomic_create_new(&first, b"clobber")
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::AlreadyExists
        );
        binding
            .write_atomic_replace(&first, "终稿".as_bytes())
            .unwrap();
        binding.rename(&first, &second, false).unwrap();
        assert_eq!(binding.read_regular(&second).unwrap(), "终稿".as_bytes());
        binding.remove_file(&second).unwrap();
        assert!(!second.exists());

        std::fs::create_dir(&first).unwrap();
        assert!(binding.remove_file(&first).is_err());
        assert!(first.is_dir());
    }

    #[test]
    fn recursive_cleanup_claims_the_exact_directory_tree() {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("workspace");
        std::fs::create_dir_all(workspace.join("nested")).unwrap();
        std::fs::write(workspace.join("nested/output.json"), b"owned").unwrap();
        let binding = BoundProjectMutationRoot::bind(root.path(), &workspace).unwrap();

        binding.remove_directory_tree(&workspace).unwrap();

        assert!(!workspace.exists());
        assert!(std::fs::read_dir(root.path()).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".wiki-bound-tree-")));
    }

    #[test]
    fn directory_rename_across_retained_parents_is_no_replace() {
        let root = tempfile::tempdir().unwrap();
        let source_parent = root.path().join("staging");
        let destination_parent = root.path().join("installed");
        let source = source_parent.join("v1");
        let destination = destination_parent.join("v1");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&destination_parent).unwrap();
        std::fs::write(source.join("manifest.json"), b"signed").unwrap();
        let source_binding = BoundProjectMutationRoot::bind(root.path(), &source).unwrap();
        let destination_binding =
            BoundProjectMutationRoot::bind(root.path(), &destination).unwrap();

        source_binding
            .rename_directory_to_no_replace(&source, &destination_binding, &destination)
            .unwrap();
        assert!(!source.exists());
        assert_eq!(
            std::fs::read(destination.join("manifest.json")).unwrap(),
            b"signed"
        );

        let replacement = source_parent.join("v2");
        std::fs::create_dir_all(&replacement).unwrap();
        std::fs::write(replacement.join("manifest.json"), b"replacement").unwrap();
        let replacement_binding =
            BoundProjectMutationRoot::bind(root.path(), &replacement).unwrap();
        assert!(replacement_binding
            .rename_directory_to_no_replace(&replacement, &destination_binding, &destination)
            .is_err());
        assert_eq!(
            std::fs::read(destination.join("manifest.json")).unwrap(),
            b"signed"
        );
        assert_eq!(
            std::fs::read(replacement.join("manifest.json")).unwrap(),
            b"replacement"
        );
    }

    #[test]
    fn retained_parent_blocks_or_survives_parent_replacement() {
        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("wiki");
        let displaced = root.path().join("wiki-displaced");
        std::fs::create_dir(&parent).unwrap();
        let target = parent.join("page.md");
        let binding = BoundProjectMutationRoot::bind(root.path(), &target).unwrap();

        std::fs::rename(&parent, &displaced).unwrap();
        std::fs::create_dir(&parent).unwrap();
        let sentinel = parent.join("page.md");
        std::fs::write(&sentinel, b"sentinel").unwrap();
        binding.write_atomic_create_new(&target, b"owned").unwrap();
        assert_eq!(std::fs::read(displaced.join("page.md")).unwrap(), b"owned");
        assert_eq!(std::fs::read(sentinel).unwrap(), b"sentinel");
    }

    #[test]
    fn checked_replace_preserves_a_concurrent_target_replacement() {
        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("wiki");
        std::fs::create_dir(&parent).unwrap();
        let target = parent.join("page.md");
        let displaced = parent.join("page-displaced.md");
        std::fs::write(&target, b"expected").unwrap();
        let binding = BoundProjectMutationRoot::bind(root.path(), &target).unwrap();
        let expected = binding.file_identity(&target).unwrap();
        let temporary = binding.write_synced_temp(&target, b"generated").unwrap();

        std::fs::rename(&target, &displaced).unwrap();
        std::fs::write(&target, b"external").unwrap();
        assert!(binding
            .replace_existing_if_identity(&temporary, &target, expected)
            .is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"external");
        assert_eq!(std::fs::read(&displaced).unwrap(), b"expected");
        binding.remove_file(&temporary).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn conditional_mutations_restore_same_inode_edits_from_the_final_window() {
        use sha2::{Digest, Sha256};

        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("wiki/page.md");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, b"baseline").unwrap();
        let binding = BoundProjectMutationRoot::bind(root.path(), &target).unwrap();
        let (_, identity) = binding.read_regular_with_identity(&target).unwrap();
        let baseline_hash = format!("{:x}", Sha256::digest(b"baseline"));
        let temporary = binding.write_synced_temp(&target, b"replacement").unwrap();
        let overwrite_target = target.clone();
        set_before_bound_final_mutation_hook(move || {
            std::fs::write(overwrite_target, b"external-overwrite").unwrap();
        });

        assert!(binding
            .replace_existing_if_identity_and_hash(&temporary, &target, identity, &baseline_hash,)
            .is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"external-overwrite");
        binding.remove_file(&temporary).unwrap();

        let (_, identity) = binding.read_regular_with_identity(&target).unwrap();
        let external_hash = format!("{:x}", Sha256::digest(b"external-overwrite"));
        let delete_target = target.clone();
        set_before_bound_final_mutation_hook(move || {
            std::fs::write(delete_target, b"external-delete").unwrap();
        });
        assert!(binding
            .remove_file_if_identity_and_hash(&target, identity, &external_hash)
            .is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"external-delete");
    }

    #[test]
    fn permission_change_keeps_complete_original_or_replacement() {
        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("wiki");
        std::fs::create_dir(&parent).unwrap();
        let target = parent.join("readonly.md");
        std::fs::write(&target, b"original").unwrap();
        let binding = BoundProjectMutationRoot::bind(root.path(), &target).unwrap();
        let mut permissions = std::fs::metadata(&target).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&target, permissions).unwrap();

        let _ = binding.write_atomic_replace(&target, b"replacement");
        let bytes = std::fs::read(&target).unwrap();
        assert!(bytes == b"original" || bytes == b"replacement");
        assert!(!std::fs::read_dir(&parent).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".wiki-bound-guard-")
        }));
    }

    #[cfg(windows)]
    #[test]
    fn windows_locked_target_fails_without_losing_original_bytes() {
        use std::os::windows::fs::OpenOptionsExt;

        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("wiki");
        std::fs::create_dir(&parent).unwrap();
        let target = parent.join("locked.md");
        std::fs::write(&target, b"locked-original").unwrap();
        let binding = BoundProjectMutationRoot::bind(root.path(), &target).unwrap();
        let locked = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0x1 | 0x2)
            .open(&target)
            .unwrap();
        assert!(binding
            .write_atomic_replace(&target, b"replacement")
            .is_err());
        drop(locked);
        assert_eq!(std::fs::read(&target).unwrap(), b"locked-original");
    }

    #[cfg(unix)]
    #[test]
    fn unix_special_file_is_rejected_without_blocking() {
        use std::os::unix::ffi::OsStrExt;

        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("wiki");
        std::fs::create_dir(&parent).unwrap();
        let target = parent.join("page.md");
        let binding = BoundProjectMutationRoot::bind(root.path(), &target).unwrap();
        let fifo = std::ffi::CString::new(target.as_os_str().as_bytes()).unwrap();
        // SAFETY: the C string is NUL terminated and points at a writable test directory.
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);

        let started = std::time::Instant::now();
        assert!(binding.read_regular(&target).is_err());
        assert!(
            started.elapsed() < std::time::Duration::from_secs(1),
            "opening a FIFO must not wait for a writer"
        );
    }

    #[cfg(unix)]
    #[test]
    fn unix_symlink_swap_race_never_changes_outside_sentinel() {
        use std::os::unix::fs::symlink;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        const ROUNDS: usize = 16;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let live = root.path().join("wiki");
        let parked = root.path().join("wiki-owned");
        std::fs::create_dir(&live).unwrap();
        let sentinel = outside.path().join("sentinel.md");
        std::fs::write(&sentinel, b"outside").unwrap();
        let phase = Arc::new(AtomicUsize::new(9));
        let swaps = Arc::new(AtomicUsize::new(0));
        let attacker_phase = Arc::clone(&phase);
        let attacker_swaps = Arc::clone(&swaps);
        let live_for_attacker = live.clone();
        let parked_for_attacker = parked.clone();
        let outside_for_attacker = outside.path().to_path_buf();
        let attacker = std::thread::spawn(move || {
            for _ in 0..ROUNDS {
                while attacker_phase.load(Ordering::Acquire) != 0 {
                    std::thread::yield_now();
                }
                std::fs::rename(&live_for_attacker, &parked_for_attacker).unwrap();
                symlink(&outside_for_attacker, &live_for_attacker).unwrap();
                attacker_swaps.fetch_add(1, Ordering::AcqRel);
                attacker_phase.store(1, Ordering::Release);
                while attacker_phase.load(Ordering::Acquire) != 2 {
                    std::thread::yield_now();
                }
                std::fs::remove_file(&live_for_attacker).unwrap();
                std::fs::rename(&parked_for_attacker, &live_for_attacker).unwrap();
                attacker_phase.store(3, Ordering::Release);
                while attacker_phase.load(Ordering::Acquire) != 4 {
                    std::thread::yield_now();
                }
            }
        });

        let mut mutations = 0;
        for round in 0..ROUNDS {
            let target = live.join(format!("inside-{round}.md"));
            let renamed = live.join(format!("inside-{round}-renamed.md"));
            let binding = BoundProjectMutationRoot::bind(root.path(), &target).unwrap();
            phase.store(0, Ordering::Release);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while phase.load(Ordering::Acquire) != 1 {
                assert!(
                    std::time::Instant::now() < deadline,
                    "symlink swap timed out"
                );
                std::thread::yield_now();
            }
            assert!(
                BoundProjectMutationRoot::bind(root.path(), &live.join("sentinel.md")).is_err()
            );
            binding.write_atomic_create_new(&target, b"inside").unwrap();
            binding.write_atomic_replace(&target, b"updated").unwrap();
            binding.rename(&target, &renamed, false).unwrap();
            binding.remove_file(&renamed).unwrap();
            mutations += 1;
            assert_eq!(std::fs::read(&sentinel).unwrap(), b"outside");
            phase.store(2, Ordering::Release);

            while phase.load(Ordering::Acquire) != 3 {
                assert!(
                    std::time::Instant::now() < deadline,
                    "symlink restore timed out"
                );
                std::thread::yield_now();
            }
            assert_eq!(std::fs::read(&sentinel).unwrap(), b"outside");
            phase.store(4, Ordering::Release);
        }
        attacker.join().unwrap();
        assert_eq!(swaps.load(Ordering::Acquire), ROUNDS);
        assert_eq!(mutations, ROUNDS);
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"outside");
    }

    #[cfg(windows)]
    #[test]
    fn windows_junction_swap_race_never_changes_outside_sentinel() {
        use std::sync::mpsc;

        const ROUNDS: usize = 4;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let live = root.path().join("wiki");
        let parked = root.path().join("wiki-owned");
        std::fs::create_dir(&live).unwrap();
        let sentinel = outside.path().join("sentinel.md");
        std::fs::write(&sentinel, b"outside").unwrap();

        let probe = root.path().join("junction-probe");
        let status = std::process::Command::new("cmd")
            .args([
                "/C",
                "mklink",
                "/J",
                probe.to_string_lossy().as_ref(),
                outside.path().to_string_lossy().as_ref(),
            ])
            .status()
            .unwrap();
        assert!(status.success(), "junction test setup failed");
        std::fs::remove_dir(&probe).unwrap();

        let (request_swap, await_swap) = mpsc::sync_channel::<()>(0);
        let (swapped, swap_complete) = mpsc::sync_channel::<()>(0);
        let (request_restore, await_restore) = mpsc::sync_channel::<()>(0);
        let (restored, restore_complete) = mpsc::sync_channel::<()>(0);
        let live_for_attacker = live.clone();
        let parked_for_attacker = parked.clone();
        let outside_for_attacker = outside.path().to_path_buf();
        let attacker = std::thread::spawn(move || {
            for _ in 0..ROUNDS {
                await_swap.recv().unwrap();
                std::fs::rename(&live_for_attacker, &parked_for_attacker).unwrap();
                let status = std::process::Command::new("cmd")
                    .args([
                        "/C",
                        "mklink",
                        "/J",
                        live_for_attacker.to_string_lossy().as_ref(),
                        outside_for_attacker.to_string_lossy().as_ref(),
                    ])
                    .status()
                    .unwrap();
                assert!(status.success(), "junction swap setup failed");
                swapped.send(()).unwrap();
                await_restore.recv().unwrap();
                std::fs::remove_dir(&live_for_attacker).unwrap();
                std::fs::rename(&parked_for_attacker, &live_for_attacker).unwrap();
                restored.send(()).unwrap();
            }
            ROUNDS
        });

        let mut mutations = 0;
        for round in 0..ROUNDS {
            let target = live.join(format!("inside-{round}.md"));
            let renamed = live.join(format!("inside-{round}-renamed.md"));
            let binding = BoundProjectMutationRoot::bind(root.path(), &target).unwrap();
            request_swap.send(()).unwrap();
            swap_complete
                .recv_timeout(std::time::Duration::from_secs(60))
                .unwrap_or_else(|error| panic!("junction swap failed in round {round}: {error}"));
            assert!(
                BoundProjectMutationRoot::bind(root.path(), &live.join("sentinel.md")).is_err()
            );
            binding.write_atomic_create_new(&target, b"inside").unwrap();
            binding.write_atomic_replace(&target, b"updated").unwrap();
            binding.rename(&target, &renamed, false).unwrap();
            binding.remove_file(&renamed).unwrap();
            mutations += 1;
            assert_eq!(std::fs::read(&sentinel).unwrap(), b"outside");
            request_restore.send(()).unwrap();
            restore_complete
                .recv_timeout(std::time::Duration::from_secs(60))
                .unwrap_or_else(|error| {
                    panic!("junction restore failed in round {round}: {error}")
                });
            assert_eq!(std::fs::read(&sentinel).unwrap(), b"outside");
        }
        assert_eq!(attacker.join().unwrap(), ROUNDS);
        assert_eq!(mutations, ROUNDS);
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"outside");
    }
}
