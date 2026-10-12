//! Bind citations once, before generation. Selected inputs and historical evidence
//! have different ownership: only selected inputs can acquire consumption records.
use super::*;
use crate::models::compile::CompileSourceDependency;
use std::collections::BTreeMap;

pub(super) struct SourceContext {
    catalog: BTreeMap<String, Option<SourceVersionRef>>,
    selected: HashSet<String>,
    initial_references: HashSet<String>,
    legacy_catalog_error: Option<BackendError>,
}

impl SourceContext {
    pub(super) fn new(
        context: &ProjectContext,
        workspace: &Path,
        selected: &[ResolvedCompileSource],
    ) -> Result<Self, BackendError> {
        let (legacy_paths, legacy_catalog_error) = match CompileLegacyAdapter::source_paths(context)
        {
            Ok(paths) => (paths, None),
            Err(error)
                if selected
                    .iter()
                    .all(|source| source.registry == CompileSourceRegistry::V2) =>
            {
                (BTreeMap::new(), Some(error))
            }
            Err(error) => return Err(error),
        };
        let mut catalog = legacy_paths
            .into_keys()
            .map(|path| (path, None))
            .collect::<BTreeMap<_, _>>();
        if context.app_dir.join("source-index-v2.json").is_file() {
            let index = SourceRegistry::read_index(context, &FileStore)?;
            let ids = index
                .by_content_hash
                .values()
                .chain(index.by_locator.values())
                .map(|pointer| &pointer.source_id)
                .collect::<HashSet<_>>();
            for id in ids {
                // Unrelated damaged Sources must not block a selected-only update.
                // Any referenced entry still undergoes full version/hash validation below.
                let manifest_path = context.layout.source_paths()?.manifest(id)?;
                let Ok(manifest) =
                    SourceRegistry::read_manifest(context, &FileStore, &manifest_path)
                else {
                    continue;
                };
                let Some(version) = manifest
                    .versions
                    .iter()
                    .find(|v| v.version_id == manifest.current_version_id)
                else {
                    continue;
                };
                catalog.insert(
                    manifest.wiki_path,
                    Some(SourceVersionRef {
                        source_id: manifest.source_id,
                        version_id: version.version_id.clone(),
                        content_hash: version.content_hash.clone(),
                    }),
                );
            }
        }
        for source in selected {
            catalog.insert(
                source.workspace_path.clone(),
                Some(source.reference.clone()),
            );
        }
        let mut result = Self {
            catalog,
            selected: selected
                .iter()
                .map(|source| source.workspace_path.clone())
                .collect(),
            initial_references: HashSet::new(),
            legacy_catalog_error,
        };
        // Freeze this before any model execution or plan-dependent workspace changes.
        for absolute in FileStore.list_markdown_files(&workspace.join("wiki"))? {
            let relative = absolute
                .strip_prefix(workspace)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if is_compile_protected_path(&relative) {
                continue;
            }
            let content = std::fs::read_to_string(&absolute)
                .map_err(|error| io_error("COMPILE_INPUT_READ_FAILED", error, &absolute))?;
            result
                .initial_references
                .extend(result.references_in(&content));
        }
        Ok(result)
    }

    fn paths(&self) -> HashSet<String> {
        self.catalog.keys().cloned().collect()
    }

    fn references_in(&self, content: &str) -> HashSet<String> {
        let paths = self.paths();
        split_frontmatter(content)
            .frontmatter
            .and_then(|raw| crate::utils::markdown_utils::parse_frontmatter_checked(&raw).ok())
            .map(|frontmatter| {
                frontmatter
                    .get_list("sources")
                    .iter()
                    .filter_map(|raw| canonical_source_ref(raw, &paths))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn bind_plan(
        &self,
        context: &ProjectContext,
        workspace: &Path,
        plan: &mut CompilePlan,
        baseline: &HashMap<String, String>,
    ) -> Result<Vec<CompileSourceDependency>, BackendError> {
        let catalog_paths = self.paths();
        let mut needed = HashSet::new();
        for item in &mut plan.items {
            if is_compile_protected_path(&item.target_path) {
                return Err(protected_path_error(&item.target_path));
            }
            if !is_safe_wiki_markdown(&item.target_path) {
                return Err(plan_error_with_path(
                    "Unsafe CompilePlan target.",
                    &item.target_path,
                ));
            }
            let inherited = if let Some(expected) = baseline.get(&item.target_path) {
                let old = FileStore.read_markdown(context, &item.target_path)?;
                if hash_bytes(old.as_bytes()) != *expected {
                    return Err(BackendError::new(
                        "WORKFLOW_INPUT_BASELINE_CHANGED",
                        "A planned Wiki target changed before generation.",
                        true,
                        true,
                    ));
                }
                self.references_in(&old)
            } else {
                HashSet::new()
            };
            for raw in &mut item.source_ids {
                let path = canonical_source_ref(raw, &catalog_paths).ok_or_else(|| {
                    if let Some(error) = &self.legacy_catalog_error { return error.clone(); }
                    let mut error = source_ref_error(&item.target_path, raw);
                    error.message = "Source reference is missing or ambiguous in the registered Source catalog; use its full project-relative path.".into();
                    error
                })?;
                if !self.selected.contains(&path)
                    && !self.initial_references.contains(&path)
                    && !inherited.contains(&path)
                {
                    return Err(BackendError::new("COMPILE_SOURCE_SCOPE_INVALID", "The plan cites an unselected Source with no existing Wiki evidence. Select it explicitly before compiling.", true, true)
                        .with_details(serde_json::json!({"path": item.target_path, "source": raw})));
                }
                if !self.selected.contains(&path) {
                    needed.insert(path.clone());
                }
                *raw = path;
            }
            item.source_ids.sort();
            item.source_ids.dedup();
        }
        // Resolve decisions against selected Sources only, even if a historical
        // dependency has the same display filename.
        for decision in &mut plan.source_decisions {
            decision.source_id = canonical_source_ref(&decision.source_id, &self.selected)
                .ok_or_else(|| source_ref_error("sourceDecisions", &decision.source_id))?;
        }
        let mut paths = needed.into_iter().collect::<Vec<_>>();
        paths.sort();
        let mut dependencies = Vec::new();
        for path in paths {
            let reference = match self.catalog.get(&path).unwrap() {
                Some(reference) => reference.clone(),
                None => CompileLegacyAdapter::source_at_path(context, &path)?.reference,
            };
            let source =
                CompileService::resolve_source_versions(context, std::slice::from_ref(&reference))?
                    .into_iter()
                    .next()
                    .unwrap();
            let bytes = FileStore
                .read_markdown(context, &source.project_path)?
                .into_bytes();
            let file_hash = hash_bytes(&bytes);
            let target = workspace.join(&source.workspace_path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| io_error("COMPILE_WORKSPACE_FAILED", error, parent))?;
            }
            std::fs::write(&target, &bytes)
                .map_err(|error| io_error("COMPILE_WORKSPACE_FAILED", error, &target))?;
            dependencies.push(CompileSourceDependency {
                source_version: source.reference,
                project_path: source.project_path,
                workspace_path: source.workspace_path,
                file_hash,
            });
        }
        Ok(dependencies)
    }
}

/// The only revalidation path for a generated candidate. No live Wiki page can
/// expand a persisted plan's citation authority during review or recovery.
pub(super) fn validate_bound_sources(
    context: &ProjectContext,
    selected: &[ResolvedCompileSource],
    dependencies: &[CompileSourceDependency],
) -> Result<HashSet<String>, BackendError> {
    let mut known = CompileService::known_source_refs_for_sources(selected);
    let mut seen = selected
        .iter()
        .map(|source| source.workspace_path.as_str())
        .collect::<HashSet<_>>();
    for dependency in dependencies {
        if !seen.insert(dependency.workspace_path.as_str()) {
            return Err(invalid_source_version());
        }
        let sources = CompileService::resolve_source_versions(
            context,
            std::slice::from_ref(&dependency.source_version),
        )?;
        let source = &sources[0];
        if source.project_path != dependency.project_path
            || source.workspace_path != dependency.workspace_path
            || FileStore.file_hash(context, &source.project_path)? != dependency.file_hash
        {
            return Err(invalid_source_version());
        }
        known.insert(dependency.workspace_path.clone());
    }
    Ok(known)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (
        tempfile::TempDir,
        ProjectContext,
        Vec<ResolvedCompileSource>,
    ) {
        let root = tempfile::tempdir().unwrap();
        for dir in [".app", "wiki/sources/local", "wiki/concepts"] {
            std::fs::create_dir_all(root.path().join(dir)).unwrap();
        }
        for (path, body) in [
            (".app/source-index.json", r#"{"sources":{}}"#),
            ("purpose.md", "# Purpose"), ("schema.md", "# Schema"),
            ("wiki/index.md", "# Index\n[[concepts/prior]]"), ("wiki/overview.md", "# Overview"), ("wiki/log.md", "# Log"),
            ("wiki/sources/local/new.md", "# New Source"),
            ("wiki/sources/local/article_39.md", "# Historical original evidence"),
            ("wiki/concepts/prior.md", "---\ntype: concept\nsources: [article_39.md]\n---\n# Prior\n> Sources: [[sources/local/article_39]]"),
        ] { std::fs::write(root.path().join(path), body).unwrap(); }
        let context = ProjectContext::new("source-binding", root.path().to_path_buf());
        let versions = CompileService::list_source_versions(&context).unwrap();
        let selected = CompileService::resolve_source_versions(&context, &versions)
            .unwrap()
            .into_iter()
            .filter(|source| source.workspace_path.ends_with("/new.md"))
            .collect();
        (root, context, selected)
    }

    fn item(target: &str, sources: &[&str]) -> CompilePlanItem {
        CompilePlanItem {
            action: CompileAction::Create,
            target_path: target.into(),
            page_type: CompilePageType::Concept,
            source_ids: sources.iter().map(|s| (*s).into()).collect(),
            affected_existing_pages: vec![],
            reason: "Synthesize evidence".into(),
            risk_flags: vec![],
        }
    }
    fn plan(items: Vec<CompilePlanItem>) -> CompilePlan {
        CompilePlan {
            summary: "Update".into(),
            items,
            global_risk_flags: vec![],
            source_decisions: vec![],
        }
    }

    #[test]
    fn nested_historical_evidence_is_bound_and_copied_without_becoming_selected() {
        let (_root, context, selected) = fixture();
        let workspace =
            CompileService::create_workspace_for_sources(&context, "bind", &selected).unwrap();
        let scope = SourceContext::new(&context, &workspace, &selected).unwrap();
        let mut plan = plan(vec![item(
            "wiki/concepts/unattended-agent-operation.md",
            &["new.md", "article_39.md"],
        )]);
        let dependencies = prepare_bound_plan(
            &context,
            &workspace,
            &mut plan,
            &CompileService::snapshot_wiki(&context).unwrap(),
            &selected,
            &scope,
            CompileGenerationPolicy::WorkflowReviewableDeletes,
        )
        .unwrap();
        assert_eq!(dependencies.len(), 1);
        assert_eq!(
            dependencies[0].workspace_path,
            "wiki/sources/local/article_39.md"
        );
        assert_eq!(
            plan.items[0].source_ids,
            vec![
                "wiki/sources/local/article_39.md",
                "wiki/sources/local/new.md"
            ]
        );
        assert_eq!(
            std::fs::read_to_string(workspace.join(&dependencies[0].workspace_path)).unwrap(),
            "# Historical original evidence"
        );
        assert_eq!(selected.len(), 1);
        validate_bound_sources(&context, &selected, &dependencies).unwrap();
        std::fs::write(
            context.root.join(&dependencies[0].project_path),
            "# External edit",
        )
        .unwrap();
        assert!(validate_bound_sources(&context, &selected, &dependencies).is_err());
        std::fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn a_plan_target_cannot_expand_another_items_historical_authority() {
        let (_root, context, selected) = fixture();
        std::fs::write(
            context.root.join("wiki/sources/local/unseen.md"),
            "# Unselected evidence",
        )
        .unwrap();
        std::fs::write(
            context.root.join("wiki/concepts/hidden.md"),
            "---\ntype: concept\nsources: [unseen.md]\n---\n# Hidden",
        )
        .unwrap();
        let workspace =
            CompileService::create_workspace_for_sources(&context, "scope", &selected).unwrap();
        assert!(!workspace.join("wiki/concepts/hidden.md").exists());
        let scope = SourceContext::new(&context, &workspace, &selected).unwrap();
        let baseline = CompileService::snapshot_wiki(&context).unwrap();
        let old_item = item("wiki/concepts/hidden.md", &["new.md", "unseen.md"]);
        let mut own = plan(vec![old_item.clone()]);
        scope
            .bind_plan(&context, &workspace, &mut own, &baseline)
            .unwrap();
        let mut leaked = plan(vec![
            old_item,
            item("wiki/concepts/new-topic.md", &["new.md", "unseen.md"]),
        ]);
        let error = scope
            .bind_plan(&context, &workspace, &mut leaked, &baseline)
            .unwrap_err();
        assert_eq!(error.code, "COMPILE_SOURCE_SCOPE_INVALID");
        std::fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn a_derived_page_cannot_become_a_source_through_old_frontmatter() {
        let (_root, context, selected) = fixture();
        std::fs::write(
            context.root.join("wiki/concepts/prior.md"),
            "---\ntype: concept\nsources: [wiki/concepts/fake.md]\n---\n# Prior",
        )
        .unwrap();
        std::fs::write(
            context.root.join("wiki/concepts/fake.md"),
            "# Derived, not a Source",
        )
        .unwrap();
        let workspace =
            CompileService::create_workspace_for_sources(&context, "fake", &selected).unwrap();
        let scope = SourceContext::new(&context, &workspace, &selected).unwrap();
        let mut plan = plan(vec![item(
            "wiki/concepts/prior.md",
            &["new.md", "wiki/concepts/fake.md"],
        )]);
        assert_eq!(
            scope
                .bind_plan(
                    &context,
                    &workspace,
                    &mut plan,
                    &CompileService::snapshot_wiki(&context).unwrap()
                )
                .unwrap_err()
                .code,
            "COMPILE_SOURCE_REF_INVALID"
        );
        std::fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn aliases_are_unicode_normalized_but_never_guess_case_or_duplicate_names() {
        let known = HashSet::from([
            "wiki/sources/local/资料 café.md".to_string(),
            "raw/extracted/a/同名.md".into(),
            "wiki/sources/b/同名.md".into(),
            "wiki/sources/Case.md".into(),
        ]);
        assert_eq!(
            canonical_source_ref("资料 cafe\u{301}.md", &known).as_deref(),
            Some("wiki/sources/local/资料 café.md")
        );
        assert_eq!(
            canonical_source_ref(r"wiki\sources\local\资料 café.md", &known).as_deref(),
            Some("wiki/sources/local/资料 café.md")
        );
        assert!(canonical_source_ref("同名.md", &known).is_none());
        assert_eq!(
            canonical_source_ref("raw/extracted/a/同名.md", &known).as_deref(),
            Some("raw/extracted/a/同名.md")
        );
        assert!(canonical_source_ref("case.md", &known).is_none());
        assert!(canonical_source_ref("../Case.md", &known).is_none());
    }
}
