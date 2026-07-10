use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::errors::BackendError;
use crate::models::git::CheckpointPurpose;
use crate::models::paths::ProjectContext;
use crate::services::{file_store::FileStore, GitService};

use super::artifacts::{remove_project_files, validate_artifact_paths, verify_project_hash};

type FileBackup = Vec<(PathBuf, Option<Vec<u8>>)>;

impl super::ImportService {
    pub fn cleanup_replacement_artifacts(
        &self,
        context: &ProjectContext,
        old_artifacts: &[String],
        new_artifacts: &[String],
    ) {
        let current: HashSet<&str> = old_artifacts.iter().map(String::as_str).collect();
        let staged_only: Vec<String> = new_artifacts
            .iter()
            .filter(|path| !current.contains(path.as_str()))
            .cloned()
            .collect();
        remove_project_files(context, &staged_only);
    }

    pub fn apply_source_delete(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        git_service: &GitService,
        target_path: &str,
        target_hash: &str,
        artifacts: &[String],
    ) -> Result<bool, BackendError> {
        self.validate_imported_source_path(context, target_path)?;
        verify_project_hash(file_store, context, target_path, target_hash)?;
        validate_artifact_paths(context, artifacts)?;
        let mut scoped = vec![
            target_path.to_string(),
            ".app/source-index.json".to_string(),
        ];
        scoped.extend(artifacts.iter().cloned());
        let backup = backup_project_files(context, &scoped)?;
        let checkpoint = git_service.create_scoped_checkpoint(
            context,
            CheckpointPurpose::HighRiskOperation,
            "Before deleting original source",
            &scoped,
        )?;
        let result = (|| {
            fs::remove_file(context.resolve_project_path(target_path)?).map_err(|error| {
                BackendError::new("SOURCE_DELETE_FAILED", error.to_string(), true, false)
            })?;
            remove_project_files(context, artifacts);
            let mut index = self.read_source_index(context, file_store)?;
            index.sources.remove(target_path);
            file_store.write_json_atomic(context, ".app/source-index.json", &index)?;
            git_service.create_scoped_checkpoint(
                context,
                CheckpointPurpose::FinalResult,
                "Delete original source",
                &scoped,
            )?;
            Ok::<(), BackendError>(())
        })();
        if let Err(error) = result {
            restore_project_files(&backup);
            let _ = git_service.unstage_paths(context, &scoped);
            return Err(error);
        }
        Ok(checkpoint.commit_hash.is_some())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn apply_source_replace(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        git_service: &GitService,
        target_path: &str,
        target_hash: &str,
        replacement_path: &Path,
        replacement_hash: &str,
        old_artifacts: &[String],
        new_artifacts: &[String],
    ) -> Result<bool, BackendError> {
        let operation = (|| {
            self.validate_imported_source_path(context, target_path)?;
            verify_project_hash(file_store, context, target_path, target_hash)?;
            if self.hash_external_file(replacement_path)? != replacement_hash {
                return Err(BackendError::new(
                    "CONFIRMATION_STATE_MISMATCH",
                    "The replacement source changed after preview.",
                    true,
                    true,
                ));
            }
            validate_artifact_paths(context, old_artifacts)?;
            validate_artifact_paths(context, new_artifacts)?;
            let mut before_paths = vec![
                target_path.to_string(),
                ".app/source-index.json".to_string(),
            ];
            before_paths.extend(old_artifacts.iter().cloned());
            let mut result_paths = before_paths.clone();
            result_paths.extend(new_artifacts.iter().cloned());
            let backup = backup_project_files(context, &result_paths)?;
            let checkpoint = git_service.create_scoped_checkpoint(
                context,
                CheckpointPurpose::HighRiskOperation,
                "Before replacing original source",
                &before_paths,
            )?;
            let mutation = (|| {
                fs::copy(replacement_path, context.resolve_project_path(target_path)?).map_err(
                    |error| {
                        BackendError::new("SOURCE_REPLACE_FAILED", error.to_string(), true, false)
                    },
                )?;
                let keep: HashSet<&str> = new_artifacts.iter().map(String::as_str).collect();
                let obsolete: Vec<String> = old_artifacts
                    .iter()
                    .filter(|path| !keep.contains(path.as_str()))
                    .cloned()
                    .collect();
                remove_project_files(context, &obsolete);
                let mut index = self.read_source_index(context, file_store)?;
                index
                    .sources
                    .insert(target_path.to_string(), new_artifacts.to_vec());
                file_store.write_json_atomic(context, ".app/source-index.json", &index)?;
                git_service.create_scoped_checkpoint(
                    context,
                    CheckpointPurpose::FinalResult,
                    "Replace original source",
                    &result_paths,
                )?;
                Ok::<(), BackendError>(())
            })();
            if let Err(error) = mutation {
                restore_project_files(&backup);
                let _ = git_service.unstage_paths(context, &result_paths);
                return Err(error);
            }
            Ok(checkpoint.commit_hash.is_some())
        })();
        if operation.is_err() {
            self.cleanup_replacement_artifacts(context, old_artifacts, new_artifacts);
        }
        operation
    }
}

fn backup_project_files(
    context: &ProjectContext,
    paths: &[String],
) -> Result<FileBackup, BackendError> {
    paths
        .iter()
        .map(|path| {
            let absolute = context.resolve_project_path(path)?;
            let bytes = if absolute.exists() {
                Some(fs::read(&absolute).map_err(|error| {
                    BackendError::new("SOURCE_BACKUP_FAILED", error.to_string(), true, false)
                })?)
            } else {
                None
            };
            Ok((absolute, bytes))
        })
        .collect()
}

fn restore_project_files(backup: &FileBackup) {
    for (path, bytes) in backup {
        match bytes {
            Some(bytes) => {
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::write(path, bytes);
            }
            None => {
                let _ = fs::remove_file(path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fs};

    use crate::models::import::*;
    use crate::services::{FileStore, GitService};

    use super::super::{test_support::tmp_context, ImportService};

    #[test]
    fn replacement_artifact_cleanup_preserves_artifacts_shared_with_the_current_source() {
        let (context, root) = tmp_context("replacement-cleanup");
        let shared = root.join("raw/extracted/shared.md");
        let staged = root.join("raw/extracted/staged.md");
        fs::create_dir_all(shared.parent().unwrap()).unwrap();
        fs::write(&shared, "shared").unwrap();
        fs::write(&staged, "staged").unwrap();

        ImportService.cleanup_replacement_artifacts(
            &context,
            &["raw/extracted/shared.md".to_string()],
            &[
                "raw/extracted/shared.md".to_string(),
                "raw/extracted/staged.md".to_string(),
            ],
        );

        assert!(shared.exists());
        assert!(!staged.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn source_delete_removes_indexed_artifacts_and_commits_a_clean_result() {
        let (context, root) = tmp_context("source-delete");
        let source = root.join("raw/sources/markdown/source.md");
        let artifact = root.join("raw/extracted/source.md");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::create_dir_all(artifact.parent().unwrap()).unwrap();
        fs::create_dir_all(&context.app_dir).unwrap();
        fs::write(&source, "# Source").unwrap();
        fs::write(&artifact, "# Extracted").unwrap();
        let store = FileStore;
        store
            .write_json_atomic(
                &context,
                ".app/source-index.json",
                &SourceArtifactIndex {
                    sources: HashMap::from([(
                        "raw/sources/markdown/source.md".to_string(),
                        vec!["raw/extracted/source.md".to_string()],
                    )]),
                },
            )
            .unwrap();
        let git = GitService;
        git.initialize_repository(&context, "baseline").unwrap();
        let hash = store
            .file_hash(&context, "raw/sources/markdown/source.md")
            .unwrap();

        let checkpoint = ImportService
            .apply_source_delete(
                &context,
                &store,
                &git,
                "raw/sources/markdown/source.md",
                &hash,
                &["raw/extracted/source.md".to_string()],
            )
            .unwrap();

        assert!(checkpoint);
        assert!(!source.exists());
        assert!(!artifact.exists());
        assert!(ImportService
            .read_source_index(&context, &store)
            .unwrap()
            .sources
            .is_empty());
        assert!(!git.repository_status(&context).unwrap().has_changes);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejected_source_replacement_cleans_only_new_staged_artifacts() {
        let (context, root) = tmp_context("source-replace-rejected");
        let source = root.join("raw/sources/markdown/source.md");
        let shared = root.join("raw/extracted/shared.md");
        let staged = root.join("raw/extracted/staged.md");
        let replacement = root.join("replacement.md");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::create_dir_all(shared.parent().unwrap()).unwrap();
        fs::write(&source, "old").unwrap();
        fs::write(&shared, "shared").unwrap();
        fs::write(&staged, "staged").unwrap();
        fs::write(&replacement, "new").unwrap();
        let service = ImportService;
        let replacement_hash = service.hash_external_file(&replacement).unwrap();

        let error = service
            .apply_source_replace(
                &context,
                &FileStore,
                &GitService,
                "raw/sources/markdown/source.md",
                "stale-target-hash",
                &replacement,
                &replacement_hash,
                &["raw/extracted/shared.md".to_string()],
                &[
                    "raw/extracted/shared.md".to_string(),
                    "raw/extracted/staged.md".to_string(),
                ],
            )
            .unwrap_err();

        assert_eq!(error.code, "CONFIRMATION_STATE_MISMATCH");
        assert_eq!(fs::read_to_string(source).unwrap(), "old");
        assert!(shared.exists());
        assert!(!staged.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn source_replace_updates_archive_artifacts_index_and_commits_a_clean_result() {
        let (context, root) = tmp_context("source-replace");
        let source = root.join("raw/sources/markdown/source.md");
        let old_artifact = root.join("raw/extracted/old.md");
        let new_artifact = root.join("raw/extracted/new.md");
        let replacement = root.join("replacement.md");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::create_dir_all(old_artifact.parent().unwrap()).unwrap();
        fs::create_dir_all(&context.app_dir).unwrap();
        fs::write(&source, "old source").unwrap();
        fs::write(&old_artifact, "old extracted").unwrap();
        fs::write(&new_artifact, "new extracted").unwrap();
        fs::write(&replacement, "new source").unwrap();
        let store = FileStore;
        store
            .write_json_atomic(
                &context,
                ".app/source-index.json",
                &SourceArtifactIndex {
                    sources: HashMap::from([(
                        "raw/sources/markdown/source.md".to_string(),
                        vec!["raw/extracted/old.md".to_string()],
                    )]),
                },
            )
            .unwrap();
        let git = GitService;
        git.initialize_repository(&context, "baseline").unwrap();
        let target_hash = store
            .file_hash(&context, "raw/sources/markdown/source.md")
            .unwrap();
        let replacement_hash = ImportService.hash_external_file(&replacement).unwrap();

        let checkpoint = ImportService
            .apply_source_replace(
                &context,
                &store,
                &git,
                "raw/sources/markdown/source.md",
                &target_hash,
                &replacement,
                &replacement_hash,
                &["raw/extracted/old.md".to_string()],
                &["raw/extracted/new.md".to_string()],
            )
            .unwrap();

        assert!(checkpoint);
        assert_eq!(fs::read_to_string(source).unwrap(), "new source");
        assert!(!old_artifact.exists());
        assert!(new_artifact.exists());
        assert_eq!(
            ImportService
                .read_source_index(&context, &store)
                .unwrap()
                .sources
                .get("raw/sources/markdown/source.md"),
            Some(&vec!["raw/extracted/new.md".to_string()])
        );
        assert!(!git.repository_status(&context).unwrap().has_changes);
        fs::remove_dir_all(root).unwrap();
    }

    // ── Extraction status in preview test ──
}
