use std::collections::{HashMap, HashSet};
use std::path::{Component, Path};

use crate::errors::BackendError;
use crate::models::compile::{CompileConflictResolution, CompileFile, CompileManifest};
use crate::models::import::SourceArtifactIndex;
use crate::models::paths::ProjectContext;
use crate::services::{FileStore, WriteMode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileApplyOutcome {
    pub affected_paths: Vec<String>,
    pub conflicts: Vec<String>,
}

pub struct CompileBackup {
    entries: Vec<(String, Option<Vec<u8>>)>,
}

#[derive(Default)]
pub struct CompileService;

impl CompileService {
    pub fn provider_prompt(workspace: &Path, language: &str) -> Result<String, BackendError> {
        let mut prompt = String::from("Return only JSON matching {files:[{path,content}],deletions:[],summary}. Paths must be wiki/*.md and include wiki/index.md, wiki/overview.md, wiki/log.md. Never delete pages.\n");
        prompt.push_str(&crate::utils::i18n::language_instruction(language));
        prompt.push_str(" Write each page's prose body in that language; keep frontmatter keys, paths, and this JSON structure in English.\n");
        for name in ["purpose.md", "schema.md"] {
            let content = std::fs::read_to_string(workspace.join(name)).map_err(|error| {
                BackendError::new("COMPILE_INPUT_READ_FAILED", error.to_string(), true, false)
            })?;
            prompt.push_str(&format!("\n--- {name} ---\n{content}"));
        }
        for root in [workspace.join("raw/extracted"), workspace.join("wiki")] {
            for file in FileStore.list_markdown_files(&root)? {
                let relative = file
                    .strip_prefix(workspace)
                    .unwrap_or(&file)
                    .to_string_lossy()
                    .replace('\\', "/");
                let content = std::fs::read_to_string(&file).map_err(|error| {
                    BackendError::new("COMPILE_INPUT_READ_FAILED", error.to_string(), true, false)
                })?;
                prompt.push_str(&format!("\n--- {relative} ---\n{content}"));
            }
        }
        Ok(prompt)
    }

    pub fn extracted_markdown_files(
        context: &ProjectContext,
    ) -> Result<Vec<std::path::PathBuf>, BackendError> {
        let index_path = context.app_dir.join("source-index.json");
        let index_exists = index_path.exists();
        let index = if index_exists {
            FileStore.read_json_file::<SourceArtifactIndex>(&index_path)?
        } else {
            SourceArtifactIndex::default()
        };

        // Confirmed sources are the index keys; their extracted Markdown
        // artifacts are raw/extracted/*.md values. Track both so the empty
        // error can explain *why* compile has no input rather than just that
        // it doesn't — the generic message sends users hunting for a pipeline
        // bug when the real cause is "I imported an image-only PDF".
        let confirmed_sources: Vec<String> = {
            let mut keys: Vec<String> = index.sources.keys().cloned().collect();
            keys.sort();
            keys
        };
        let mut relative_paths: Vec<String> = index
            .sources
            .values()
            .flatten()
            .filter(|path| path.starts_with("raw/extracted/") && path.ends_with(".md"))
            .cloned()
            .collect();
        relative_paths.sort();
        relative_paths.dedup();

        let mut files = Vec::new();
        let mut empty_on_disk: Vec<String> = Vec::new();
        for relative in &relative_paths {
            let file = context.resolve_project_path(relative)?;
            let content = std::fs::read_to_string(&file)
                .map_err(|error| io_error("COMPILE_INPUT_READ_FAILED", error, &file))?;
            if content.trim().is_empty() {
                empty_on_disk.push(relative.clone());
            } else {
                files.push(file);
            }
        }
        files.sort();
        if files.is_empty() {
            return Err(compile_input_empty_error(
                index_exists,
                &confirmed_sources,
                &relative_paths,
                &empty_on_disk,
            ));
        }
        Ok(files)
    }

    pub fn resolve_conflict_manifest(
        manifest: &CompileManifest,
        conflict_paths: &[String],
        resolution: CompileConflictResolution,
        manual_files: &[CompileFile],
    ) -> Result<CompileManifest, BackendError> {
        Self::validate_manifest(manifest)?;
        let conflicts: HashSet<&str> = conflict_paths.iter().map(String::as_str).collect();
        let manifest_paths: HashSet<&str> = manifest
            .files
            .iter()
            .map(|file| file.path.as_str())
            .chain(manifest.deletions.iter().map(String::as_str))
            .collect();
        if conflicts.iter().any(|path| !manifest_paths.contains(path)) {
            return Err(BackendError::new(
                "COMPILE_CONFLICT_PATH_INVALID",
                "A conflict path is not part of the generated manifest.",
                false,
                true,
            ));
        }
        if resolution == CompileConflictResolution::UseGenerated {
            return Ok(manifest.clone());
        }
        let mut resolved = CompileManifest {
            files: manifest
                .files
                .iter()
                .filter(|file| !conflicts.contains(file.path.as_str()))
                .cloned()
                .collect(),
            deletions: manifest
                .deletions
                .iter()
                .filter(|path| !conflicts.contains(path.as_str()))
                .cloned()
                .collect(),
            summary: manifest.summary.clone(),
        };
        if resolution == CompileConflictResolution::KeepCurrent {
            return Ok(resolved);
        }

        let mut manual_by_path = HashMap::new();
        for file in manual_files {
            if !conflicts.contains(file.path.as_str())
                || manual_by_path.insert(file.path.as_str(), file).is_some()
            {
                return Err(BackendError::new(
                    "COMPILE_MANUAL_MERGE_INVALID",
                    "Manual merge files must map one-to-one to conflicting paths.",
                    true,
                    true,
                ));
            }
        }
        if manual_by_path.len() != conflicts.len() {
            return Err(BackendError::new(
                "COMPILE_MANUAL_MERGE_INCOMPLETE",
                "Manual merge content is required for every conflicting path.",
                true,
                true,
            ));
        }
        for path in conflict_paths {
            resolved
                .files
                .push((*manual_by_path[path.as_str()]).clone());
        }
        Self::validate_manifest(&resolved)?;
        Ok(resolved)
    }

    pub fn parse_manifest(raw: &str) -> Result<CompileManifest, BackendError> {
        let trimmed = raw.trim();
        let json = if let Some(start) = trimmed.find("```json") {
            let rest = &trimmed[start + 7..];
            let end = rest.find("```").ok_or_else(|| {
                BackendError::new(
                    "COMPILE_OUTPUT_INVALID",
                    "Unclosed JSON code fence.",
                    true,
                    false,
                )
            })?;
            rest[..end].trim()
        } else if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
            &trimmed[start..=end]
        } else {
            trimmed
        };
        let manifest: CompileManifest = serde_json::from_str(json).map_err(|error| {
            BackendError::new("COMPILE_OUTPUT_INVALID", error.to_string(), true, false)
        })?;
        Self::validate_manifest(&manifest)?;
        Ok(manifest)
    }

    pub fn create_workspace(
        context: &ProjectContext,
        task_id: &str,
    ) -> Result<std::path::PathBuf, BackendError> {
        let workspace = std::env::temp_dir().join("llm-wiki-desktop").join(task_id);
        if workspace.exists() {
            std::fs::remove_dir_all(&workspace)
                .map_err(|error| io_error("COMPILE_WORKSPACE_FAILED", error, &workspace))?;
        }
        std::fs::create_dir_all(&workspace)
            .map_err(|error| io_error("COMPILE_WORKSPACE_FAILED", error, &workspace))?;
        let result = Self::populate_workspace(context, &workspace);
        if let Err(error) = result {
            let _ = std::fs::remove_dir_all(&workspace);
            return Err(error);
        }
        Ok(workspace)
    }

    fn populate_workspace(context: &ProjectContext, workspace: &Path) -> Result<(), BackendError> {
        for name in ["purpose.md", "schema.md"] {
            let source = context.root.join(name);
            if !source.exists() {
                return Err(BackendError::new(
                    "COMPILE_INPUT_MISSING",
                    format!("Required input is missing: {name}"),
                    true,
                    true,
                ));
            }
            std::fs::copy(&source, workspace.join(name))
                .map_err(|error| io_error("COMPILE_WORKSPACE_FAILED", error, &source))?;
        }
        let workspace_extracted = workspace.join("raw/extracted");
        std::fs::create_dir_all(&workspace_extracted)
            .map_err(|error| io_error("COMPILE_WORKSPACE_FAILED", error, &workspace_extracted))?;
        for source in Self::extracted_markdown_files(context)? {
            let file_name = source.file_name().ok_or_else(|| {
                BackendError::new(
                    "COMPILE_INPUT_PATH_INVALID",
                    "An extracted Markdown path has no file name.",
                    false,
                    true,
                )
            })?;
            std::fs::copy(&source, workspace_extracted.join(file_name))
                .map_err(|error| io_error("COMPILE_WORKSPACE_FAILED", error, &source))?;
        }
        copy_tree(&context.wiki_dir, &workspace.join("wiki"))?;
        let skill_dir = workspace.join("skills/wiki-ingest");
        std::fs::create_dir_all(&skill_dir)
            .map_err(|error| io_error("COMPILE_WORKSPACE_FAILED", error, &skill_dir))?;
        std::fs::write(
            skill_dir.join("SKILL.md"),
            include_str!("../../templates/skills/wiki-ingest/SKILL.md"),
        )
        .map_err(|error| io_error("COMPILE_WORKSPACE_FAILED", error, &skill_dir))?;
        Ok(())
    }

    pub fn manifest_from_workspace(
        workspace: &Path,
        original_paths: impl Iterator<Item = String>,
    ) -> Result<CompileManifest, BackendError> {
        let wiki = workspace.join("wiki");
        let mut files = Vec::new();
        for absolute in FileStore.list_markdown_files(&wiki)? {
            let relative = absolute
                .strip_prefix(workspace)
                .map_err(|_| {
                    BackendError::new(
                        "COMPILE_PATH_INVALID",
                        "Candidate path escaped workspace.",
                        false,
                        false,
                    )
                })?
                .to_string_lossy()
                .replace('\\', "/");
            let content = std::fs::read_to_string(&absolute)
                .map_err(|error| io_error("COMPILE_OUTPUT_READ_FAILED", error, &absolute))?;
            files.push(crate::models::compile::CompileFile::new(relative, content));
        }
        let candidate_paths: HashSet<&str> = files.iter().map(|file| file.path.as_str()).collect();
        let deletions = original_paths
            .filter(|path| !candidate_paths.contains(path.as_str()))
            .collect();
        let manifest = CompileManifest {
            files,
            deletions,
            summary: "Agent wiki compile".into(),
        };
        Self::validate_manifest(&manifest)?;
        Ok(manifest)
    }

    pub fn compile_prompt(workspace: &Path, language: &str) -> String {
        let mut prompt = format!(
            "Compile this local Markdown wiki in {}. Read purpose.md, schema.md, raw/extracted/, existing wiki/, and skills/wiki-ingest/SKILL.md. Update wiki/index.md, wiki/overview.md, and wiki/log.md. Do not access or modify anything outside this workspace.",
            workspace.to_string_lossy()
        );
        // Steer generated wiki page prose to the user's language; structural
        // fields (frontmatter keys, file paths, section headings used by
        // lint/graph parsing) stay English so schema is unaffected.
        prompt.push('\n');
        prompt.push_str(&crate::utils::i18n::language_instruction(language));
        prompt.push_str(" Write each page's prose body in that language; keep frontmatter keys, file paths, and section headings in English.");
        prompt
    }

    pub fn validate_manifest(manifest: &CompileManifest) -> Result<(), BackendError> {
        let mut seen = HashSet::new();
        for path in manifest
            .files
            .iter()
            .map(|file| file.path.as_str())
            .chain(manifest.deletions.iter().map(String::as_str))
        {
            if !is_safe_wiki_markdown(path) || !seen.insert(path.to_string()) {
                return Err(BackendError::new(
                    "COMPILE_PATH_INVALID",
                    "Compile output contains an unsafe or duplicate path.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({ "path": path })));
            }
        }

        for required in ["wiki/index.md", "wiki/overview.md", "wiki/log.md"] {
            if !manifest.files.iter().any(|file| file.path == required) {
                return Err(BackendError::new(
                    "COMPILE_CORE_PAGE_MISSING",
                    "Compile output must include index, overview, and log pages.",
                    true,
                    false,
                )
                .with_details(serde_json::json!({ "path": required })));
            }
        }
        Ok(())
    }

    pub fn snapshot_wiki(
        context: &ProjectContext,
    ) -> Result<HashMap<String, String>, BackendError> {
        let store = FileStore;
        let mut hashes = HashMap::new();
        for absolute in store.list_markdown_files(&context.wiki_dir)? {
            let relative = context.to_project_relative(&absolute)?;
            hashes.insert(relative.clone(), store.file_hash(context, &relative)?);
        }
        Ok(hashes)
    }

    pub fn apply_manifest(
        context: &ProjectContext,
        manifest: &CompileManifest,
        baseline: &HashMap<String, String>,
    ) -> Result<CompileApplyOutcome, BackendError> {
        Self::validate_manifest(manifest)?;
        let store = FileStore;
        let mut affected_paths = Vec::new();
        let mut conflicts = Vec::new();
        for file in &manifest.files {
            let target = context.resolve_project_path(&file.path)?;
            if let Some(expected) = baseline.get(&file.path) {
                let needs_confirmation =
                    if !target.exists() || store.file_hash(context, &file.path)? != *expected {
                        true
                    } else {
                        std::fs::read_to_string(&target).map_err(|error| {
                            io_error("COMPILE_INPUT_READ_FAILED", error, &target)
                        })? != file.content
                    };
                if needs_confirmation {
                    conflicts.push(file.path.clone());
                }
            } else if target.exists() {
                conflicts.push(file.path.clone());
            }
        }
        for deletion in &manifest.deletions {
            conflicts.push(deletion.clone());
        }
        affected_paths.sort();
        conflicts.sort();
        conflicts.dedup();
        if !conflicts.is_empty() {
            return Ok(CompileApplyOutcome {
                affected_paths,
                conflicts,
            });
        }
        for file in &manifest.files {
            let mode = baseline
                .get(&file.path)
                .map(|expected| WriteMode::OverwriteIfHashMatches(expected.clone()))
                .unwrap_or(WriteMode::CreateNew);
            store.write_markdown_checked(context, &file.path, &file.content, mode)?;
            affected_paths.push(file.path.clone());
        }
        affected_paths.sort();
        Ok(CompileApplyOutcome {
            affected_paths,
            conflicts,
        })
    }

    pub fn apply_confirmed_manifest(
        context: &ProjectContext,
        manifest: &CompileManifest,
        expected_current_hashes: &HashMap<String, String>,
    ) -> Result<Vec<String>, BackendError> {
        let store = FileStore;
        let mut affected = Vec::new();
        for file in &manifest.files {
            let target = context.resolve_project_path(&file.path)?;
            match expected_current_hashes.get(&file.path) {
                Some(expected)
                    if !target.exists() || store.file_hash(context, &file.path)? != *expected =>
                {
                    return Err(BackendError::new(
                        "CONFIRMATION_STATE_MISMATCH",
                        "A confirmed page changed again.",
                        true,
                        true,
                    ));
                }
                None if target.exists() => {
                    return Err(BackendError::new(
                        "CONFIRMATION_STATE_MISMATCH",
                        "A confirmed new page now exists.",
                        true,
                        true,
                    ));
                }
                _ => {}
            }
        }
        for deletion in &manifest.deletions {
            let target = context.resolve_project_path(deletion)?;
            match expected_current_hashes.get(deletion) {
                Some(expected)
                    if !target.exists() || store.file_hash(context, deletion)? != *expected =>
                {
                    return Err(BackendError::new(
                        "CONFIRMATION_STATE_MISMATCH",
                        "A confirmed page changed again.",
                        true,
                        true,
                    ));
                }
                None if target.exists() => {
                    return Err(BackendError::new(
                        "CONFIRMATION_STATE_MISMATCH",
                        "A confirmed deletion target appeared after review.",
                        true,
                        true,
                    ));
                }
                _ => {}
            }
        }
        for file in &manifest.files {
            if !is_safe_wiki_markdown(&file.path) {
                return Err(BackendError::new(
                    "COMPILE_PATH_INVALID",
                    "Compile output contains an unsafe path.",
                    false,
                    true,
                ));
            }
            let target = context.resolve_project_path(&file.path)?;
            let mode = match expected_current_hashes.get(&file.path) {
                Some(expected) => WriteMode::OverwriteIfHashMatches(expected.clone()),
                None if !target.exists() => WriteMode::CreateNew,
                None => unreachable!("confirmed paths were preflighted"),
            };
            store.write_markdown_checked(context, &file.path, &file.content, mode)?;
            affected.push(file.path.clone());
        }
        for deletion in &manifest.deletions {
            if !is_safe_wiki_markdown(deletion) {
                return Err(BackendError::new(
                    "COMPILE_PATH_INVALID",
                    "Compile deletion contains an unsafe path.",
                    false,
                    true,
                ));
            }
            let target = context.resolve_project_path(deletion)?;
            let Some(expected) = expected_current_hashes.get(deletion) else {
                debug_assert!(!target.exists());
                continue;
            };
            debug_assert_eq!(store.file_hash(context, deletion)?, *expected);
            std::fs::remove_file(target).map_err(|error| {
                BackendError::new("FILE_DELETE_FAILED", error.to_string(), true, false)
            })?;
            affected.push(deletion.clone());
        }
        affected.sort();
        Ok(affected)
    }

    pub fn candidate_diff(manifest: &CompileManifest) -> String {
        let mut diff = String::from("```diff\n");
        for file in &manifest.files {
            diff.push_str(&format!(
                "--- {} (current)\n+++ {} (candidate)\n",
                file.path, file.path
            ));
            for line in file.content.lines() {
                diff.push_str(&format!("+{line}\n"));
            }
        }
        for path in &manifest.deletions {
            diff.push_str(&format!("--- {path}\n+++ /dev/null\n"));
        }
        diff.push_str("```");
        diff
    }

    pub fn backup_outputs(
        context: &ProjectContext,
        manifest: &CompileManifest,
    ) -> Result<CompileBackup, BackendError> {
        let mut paths: Vec<String> = manifest
            .files
            .iter()
            .map(|file| file.path.clone())
            .chain(manifest.deletions.iter().cloned())
            .chain(std::iter::once(".app/graph-cache.json".to_string()))
            .collect();
        paths.sort();
        paths.dedup();
        let mut entries = Vec::with_capacity(paths.len());
        for relative in paths {
            let absolute = context.resolve_project_path(&relative)?;
            let bytes = if absolute.exists() {
                Some(
                    std::fs::read(&absolute)
                        .map_err(|error| io_error("COMPILE_BACKUP_FAILED", error, &absolute))?,
                )
            } else {
                None
            };
            entries.push((relative, bytes));
        }
        Ok(CompileBackup { entries })
    }

    pub fn restore_outputs(
        context: &ProjectContext,
        backup: &CompileBackup,
    ) -> Result<(), BackendError> {
        for (relative, bytes) in &backup.entries {
            let absolute = context.resolve_project_path(relative)?;
            match bytes {
                Some(bytes) => {
                    if let Some(parent) = absolute.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|error| io_error("COMPILE_ROLLBACK_FAILED", error, parent))?;
                    }
                    std::fs::write(&absolute, bytes)
                        .map_err(|error| io_error("COMPILE_ROLLBACK_FAILED", error, &absolute))?;
                }
                None if absolute.exists() => {
                    std::fs::remove_file(&absolute)
                        .map_err(|error| io_error("COMPILE_ROLLBACK_FAILED", error, &absolute))?;
                }
                None => {}
            }
        }
        Ok(())
    }
}

fn copy_tree(source: &Path, target: &Path) -> Result<(), BackendError> {
    if !source.exists() {
        std::fs::create_dir_all(target)
            .map_err(|error| io_error("COMPILE_WORKSPACE_FAILED", error, target))?;
        return Ok(());
    }
    std::fs::create_dir_all(target)
        .map_err(|error| io_error("COMPILE_WORKSPACE_FAILED", error, target))?;
    for entry in std::fs::read_dir(source)
        .map_err(|error| io_error("COMPILE_WORKSPACE_FAILED", error, source))?
    {
        let entry = entry.map_err(|error| io_error("COMPILE_WORKSPACE_FAILED", error, source))?;
        let file_type = entry
            .file_type()
            .map_err(|error| io_error("COMPILE_WORKSPACE_FAILED", error, &entry.path()))?;
        let destination = target.join(entry.file_name());
        if file_type.is_symlink() {
            return Err(BackendError::new(
                "COMPILE_SYMLINK_REJECTED",
                "Compile inputs cannot contain symbolic links.",
                true,
                true,
            ));
        }
        if file_type.is_dir() {
            copy_tree(&entry.path(), &destination)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), &destination)
                .map_err(|error| io_error("COMPILE_WORKSPACE_FAILED", error, &destination))?;
        }
    }
    Ok(())
}

fn io_error(code: &str, error: std::io::Error, path: &Path) -> BackendError {
    BackendError::new(code, error.to_string(), true, false)
        .with_details(serde_json::json!({ "path": path.to_string_lossy() }))
}

fn compile_input_empty_error(
    index_exists: bool,
    confirmed_sources: &[String],
    relative_paths: &[String],
    empty_on_disk: &[String],
) -> BackendError {
    let (summary, details) = if !index_exists || confirmed_sources.is_empty() {
        (
            "No extracted Markdown was found under raw/extracted. \
             No imports have been confirmed yet — confirm an import preview before compiling."
                .to_string(),
            serde_json::json!({
                "stage": "no_confirmed_imports",
                "index_exists": index_exists,
                "confirmed_sources": confirmed_sources,
                "hint": "Run an import (raw/sources → preview → confirm) so raw/extracted/*.md is populated.",
            }),
        )
    } else if relative_paths.is_empty() {
        (
            "No extracted Markdown was found under raw/extracted. \
             Confirmed sources produced no raw/extracted/*.md (likely image-only or \
             unsupported sources, or extraction failed silently)."
                .to_string(),
            serde_json::json!({
                "stage": "no_extracted_markdown",
                "confirmed_sources": confirmed_sources,
                "hint": "Re-import these sources or add textual sources; verify raw/extracted/ is written.",
            }),
        )
    } else {
        (
            "No extracted Markdown was found under raw/extracted. \
             Confirmed extracted files exist but are empty on disk."
                .to_string(),
            serde_json::json!({
                "stage": "extracted_files_empty",
                "confirmed_sources": confirmed_sources,
                "empty_on_disk": empty_on_disk,
                "hint": "Re-import these sources; extraction wrote empty raw/extracted/*.md files.",
            }),
        )
    };

    BackendError::new("COMPILE_INPUT_EMPTY", summary, true, true).with_details(details)
}

fn is_safe_wiki_markdown(raw: &str) -> bool {
    if raw.contains('\\') || !raw.starts_with("wiki/") || !raw.ends_with(".md") {
        return false;
    }
    let path = Path::new(raw);
    !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::compile::{CompileFile, CompileManifest};
    use crate::models::paths::ProjectContext;
    use std::fs;

    #[test]
    fn compile_rejects_an_empty_extracted_markdown_directory() {
        let root = std::env::temp_dir().join(format!("compile-empty-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("raw/extracted")).unwrap();
        let context = ProjectContext::new("project", root.clone());

        let error = CompileService::extracted_markdown_files(&context).unwrap_err();

        assert_eq!(error.code, "COMPILE_INPUT_EMPTY");
        assert!(error.message.contains("raw/extracted"));
        let details = error.details.expect("details present");
        assert_eq!(
            details.get("stage").and_then(|v| v.as_str()),
            Some("no_confirmed_imports")
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn compile_ignores_unconfirmed_orphan_extracted_markdown() {
        let root = std::env::temp_dir().join(format!("compile-orphan-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("raw/extracted")).unwrap();
        fs::create_dir_all(root.join(".app")).unwrap();
        fs::write(root.join("raw/extracted/confirmed.md"), "confirmed").unwrap();
        fs::write(root.join("raw/extracted/cancelled-preview.md"), "orphan").unwrap();
        fs::write(
            root.join(".app/source-index.json"),
            r#"{"sources":{"raw/sources/markdown/source.txt":["raw/extracted/confirmed.md"]}}"#,
        )
        .unwrap();
        let context = ProjectContext::new("project", root.clone());

        let files = CompileService::extracted_markdown_files(&context).unwrap();

        assert_eq!(files, vec![root.join("raw/extracted/confirmed.md")]);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn compile_rejects_confirmed_extracted_markdown_when_all_content_is_blank() {
        let root = std::env::temp_dir().join(format!("compile-blank-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("raw/extracted")).unwrap();
        fs::create_dir_all(root.join(".app")).unwrap();
        fs::write(root.join("raw/extracted/blank.md"), " \n\t").unwrap();
        fs::write(
            root.join(".app/source-index.json"),
            r#"{"sources":{"raw/sources/markdown/blank.txt":["raw/extracted/blank.md"]}}"#,
        )
        .unwrap();
        let context = ProjectContext::new("project", root.clone());

        let error = CompileService::extracted_markdown_files(&context).unwrap_err();

        assert_eq!(error.code, "COMPILE_INPUT_EMPTY");
        let details = error.details.expect("details present");
        assert_eq!(
            details.get("stage").and_then(|v| v.as_str()),
            Some("extracted_files_empty")
        );
        let empty_count = details
            .get("empty_on_disk")
            .and_then(|v| v.as_array())
            .map(Vec::len);
        assert_eq!(empty_count, Some(1));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn compile_reports_when_confirmed_sources_have_no_extracted_markdown() {
        let root =
            std::env::temp_dir().join(format!("compile-no-md-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("raw/extracted")).unwrap();
        fs::create_dir_all(root.join(".app")).unwrap();
        fs::write(
            root.join(".app/source-index.json"),
            // Source confirmed but points only at an image artifact, no *.md.
            r#"{"sources":{"raw/sources/image-only/source.pdf":["raw/extracted/source.png"]}}"#,
        )
        .unwrap();
        let context = ProjectContext::new("project", root.clone());

        let error = CompileService::extracted_markdown_files(&context).unwrap_err();

        assert_eq!(error.code, "COMPILE_INPUT_EMPTY");
        let details = error.details.expect("details present");
        assert_eq!(
            details.get("stage").and_then(|v| v.as_str()),
            Some("no_extracted_markdown")
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn byok_prompt_reads_cjk_extracted_markdown_from_compile_workspace() {
        let root = std::env::temp_dir().join(format!("compile-input-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("raw/extracted")).unwrap();
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::create_dir_all(root.join(".app")).unwrap();
        fs::write(root.join("purpose.md"), "# Purpose").unwrap();
        fs::write(root.join("schema.md"), "# Schema").unwrap();
        fs::write(root.join("raw/extracted/资料.md"), "# 提取内容\n\n关键事实").unwrap();
        fs::write(
            root.join(".app/source-index.json"),
            r#"{"sources":{"raw/sources/markdown/资料.txt":["raw/extracted/资料.md"]}}"#,
        )
        .unwrap();
        let context = ProjectContext::new("project", root.clone());
        let workspace = CompileService::create_workspace(
            &context,
            &format!("prompt-test-{}", uuid::Uuid::new_v4()),
        )
        .unwrap();

        let prompt = CompileService::provider_prompt(&workspace, "zh-CN").unwrap();

        assert!(prompt.contains("raw/extracted/资料.md"));
        assert!(prompt.contains("# 提取内容"));
        assert!(prompt.contains("关键事实"));
        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(workspace).ok();
    }

    #[test]
    fn apply_manifest_rejects_external_edits_before_writing_any_candidate() {
        let root = std::env::temp_dir().join(format!("compile-conflict-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::write(root.join("wiki/index.md"), "before").unwrap();
        fs::write(root.join("wiki/overview.md"), "overview").unwrap();
        fs::write(root.join("wiki/log.md"), "log").unwrap();
        let context = ProjectContext::new("project", root.clone());
        let baseline = CompileService::snapshot_wiki(&context).unwrap();
        fs::write(root.join("wiki/index.md"), "external").unwrap();

        let manifest = CompileManifest {
            files: vec![
                CompileFile::new("wiki/index.md", "candidate"),
                CompileFile::new("wiki/overview.md", "overview 2"),
                CompileFile::new("wiki/log.md", "log 2"),
            ],
            deletions: vec![],
            summary: "compile".into(),
        };
        let result = CompileService::apply_manifest(&context, &manifest, &baseline).unwrap();
        assert_eq!(
            result.conflicts,
            vec!["wiki/index.md", "wiki/log.md", "wiki/overview.md"]
        );
        assert_eq!(
            fs::read_to_string(root.join("wiki/index.md")).unwrap(),
            "external"
        );
        assert_eq!(
            fs::read_to_string(root.join("wiki/overview.md")).unwrap(),
            "overview"
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn apply_manifest_requires_confirmation_before_overwriting_existing_page() {
        let root = std::env::temp_dir().join(format!("compile-overwrite-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::write(root.join("wiki/index.md"), "current index").unwrap();
        fs::write(root.join("wiki/overview.md"), "current overview").unwrap();
        fs::write(root.join("wiki/log.md"), "current log").unwrap();
        let context = ProjectContext::new("project", root.clone());
        let baseline = CompileService::snapshot_wiki(&context).unwrap();
        let manifest = CompileManifest {
            files: vec![
                CompileFile::new("wiki/index.md", "generated index"),
                CompileFile::new("wiki/overview.md", "current overview"),
                CompileFile::new("wiki/log.md", "current log"),
            ],
            deletions: vec![],
            summary: "compile".into(),
        };

        let outcome = CompileService::apply_manifest(&context, &manifest, &baseline).unwrap();

        assert_eq!(outcome.conflicts, vec!["wiki/index.md"]);
        assert_eq!(
            fs::read_to_string(root.join("wiki/index.md")).unwrap(),
            "current index"
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn parses_json_manifest_from_fenced_provider_response() {
        let raw = "result:\n```json\n{\"files\":[{\"path\":\"wiki/index.md\",\"content\":\"i\"},{\"path\":\"wiki/overview.md\",\"content\":\"o\"},{\"path\":\"wiki/log.md\",\"content\":\"l\"}],\"deletions\":[],\"summary\":\"ok\"}\n```";
        let manifest = CompileService::parse_manifest(raw).unwrap();
        assert_eq!(manifest.summary, "ok");
        assert_eq!(manifest.files.len(), 3);
    }

    #[test]
    fn conflict_resolution_keeps_current_paths_but_applies_uncontested_candidates() {
        let manifest = CompileManifest {
            files: vec![
                CompileFile::new("wiki/conflict.md", "generated conflict"),
                CompileFile::new("wiki/safe.md", "generated safe"),
                CompileFile::new("wiki/index.md", "# Index"),
                CompileFile::new("wiki/overview.md", "# Overview"),
                CompileFile::new("wiki/log.md", "# Log"),
            ],
            deletions: vec!["wiki/delete-conflict.md".to_string()],
            summary: "compile".to_string(),
        };

        let resolved = CompileService::resolve_conflict_manifest(
            &manifest,
            &[
                "wiki/conflict.md".to_string(),
                "wiki/delete-conflict.md".to_string(),
            ],
            crate::models::compile::CompileConflictResolution::KeepCurrent,
            &[],
        )
        .unwrap();

        assert!(resolved
            .files
            .iter()
            .any(|file| file.path == "wiki/safe.md"));
        assert!(!resolved
            .files
            .iter()
            .any(|file| file.path == "wiki/conflict.md"));
        assert!(resolved.deletions.is_empty());
    }

    #[test]
    fn manual_conflict_resolution_requires_content_for_every_conflicting_path() {
        let manifest = CompileManifest {
            files: vec![
                CompileFile::new("wiki/conflict.md", "generated"),
                CompileFile::new("wiki/index.md", "# Index"),
                CompileFile::new("wiki/overview.md", "# Overview"),
                CompileFile::new("wiki/log.md", "# Log"),
            ],
            deletions: vec!["wiki/deleted.md".to_string()],
            summary: "compile".to_string(),
        };
        let conflicts = vec![
            "wiki/conflict.md".to_string(),
            "wiki/deleted.md".to_string(),
        ];

        let missing = CompileService::resolve_conflict_manifest(
            &manifest,
            &conflicts,
            crate::models::compile::CompileConflictResolution::ManualMerge,
            &[CompileFile::new("wiki/conflict.md", "merged")],
        )
        .expect_err("manual merge must cover deletions too");
        assert_eq!(missing.code, "COMPILE_MANUAL_MERGE_INCOMPLETE");

        let resolved = CompileService::resolve_conflict_manifest(
            &manifest,
            &conflicts,
            crate::models::compile::CompileConflictResolution::ManualMerge,
            &[
                CompileFile::new("wiki/conflict.md", "merged"),
                CompileFile::new("wiki/deleted.md", "kept and edited"),
            ],
        )
        .unwrap();
        assert_eq!(resolved.files.len(), 5);
        assert!(resolved.deletions.is_empty());
        assert_eq!(
            resolved
                .files
                .iter()
                .find(|file| file.path == "wiki/conflict.md")
                .map(|file| file.content.as_str()),
            Some("merged")
        );
    }
}
