use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::errors::BackendError;
use crate::models::compile::CompileConsumptionRecord;
use crate::models::export::{
    ExportContentOptions, ExportPreviewMetadata, ExportRecord, ExportRoute, ExportStatus,
    ExportType,
};
use crate::models::paths::ProjectContext;
use crate::services::file_store::FileStore;
use crate::services::import_v2::source_registry::SourceManifest;
use crate::services::SearchService;
use crate::utils::path_utils::normalize_project_path;
use crate::utils::time_utils::now_rfc3339;

const EXPORTS_HTML_DIR: &str = "exports/html";
const EXPORTS_INDEX_PATH: &str = ".app/exports.json";
const PAGE_EXCERPT_CHARS: usize = 600;
const MAX_EXPORT_HTML_BYTES: usize = 8 * 1024 * 1024;
static EXPORT_RECORD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn export_record_guard() -> Result<std::sync::MutexGuard<'static, ()>, BackendError> {
    EXPORT_RECORD_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| {
            BackendError::new(
                "EXPORT_RECORD_LOCK_FAILED",
                "The export history is temporarily unavailable.",
                true,
                false,
            )
        })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedExportArtifact {
    pub html: String,
    pub preview: ExportPreviewMetadata,
}

// Skill `template.html` styling references, baked in at compile time (mirrors
// `compile_service` embedding SKILL.md). Injecting these into the prompt makes
// the BYOK route — which has no skill workspace — follow the same styling the
// Agent CLI gets from its skill folder. Templates are styling-only.
const BEAUTIFUL_READ_TEMPLATE: &str =
    include_str!("../../templates/skills/html-beautiful-read/template.html");
const KNOWLEDGE_CARD_TEMPLATE: &str =
    include_str!("../../templates/skills/html-knowledge-card/template.html");
const CONCEPT_MAP_TEMPLATE: &str =
    include_str!("../../templates/skills/html-concept-map/template.html");
const PROJECT_REPORT_TEMPLATE: &str =
    include_str!("../../templates/skills/html-project-report/template.html");

fn template_for(export_type: ExportType) -> &'static str {
    match export_type {
        ExportType::BeautifulRead => BEAUTIFUL_READ_TEMPLATE,
        ExportType::KnowledgeCard => KNOWLEDGE_CARD_TEMPLATE,
        ExportType::ConceptMap => CONCEPT_MAP_TEMPLATE,
        ExportType::ProjectReport => PROJECT_REPORT_TEMPLATE,
    }
}

/// A short style-direction clause for the named template. The skill's
/// `template.html` is always the styling baseline; the name communicates the
/// desired aesthetic the model should lean into.
fn style_direction(template_name: &str) -> &'static str {
    match template_name {
        "modern-sans" => "modern sans-serif system font stack for body and headings",
        "editorial-magazine" => {
            "magazine feel: prominent display headings, generous measure, optional drop cap"
        }
        _ => "default serif treatment",
    }
}

/// Skill-driven HTML export orchestration: prompt assembly, output-path
/// derivation, HTML extraction, and record persistence. The model/Agent run
/// itself lives in `export_commands` (cancellable background task), mirroring
/// how `LintService` keeps the model call out of the service.
///
/// Hard boundaries honored here:
/// - Outputs only ever land under `exports/html/`.
/// - Templates/Skills only style output; this service never writes `schema.md`,
///   `wiki/`, or lint rules.
/// - No secret/API key is ever placed in a prompt or record.
#[derive(Default)]
pub struct ExportService {
    file_store: FileStore,
}

impl ExportService {
    pub fn restricted_source_count(
        &self,
        context: &ProjectContext,
        export_type: ExportType,
        source_path: Option<&str>,
    ) -> Result<usize, BackendError> {
        Ok(self
            .restricted_source_markers(context, export_type, source_path)?
            .len())
    }

    fn restricted_source_markers(
        &self,
        context: &ProjectContext,
        export_type: ExportType,
        source_path: Option<&str>,
    ) -> Result<Vec<String>, BackendError> {
        let directory = context.resolve_project_path(".app/sources")?;
        if !directory.exists() {
            return Ok(Vec::new());
        }
        let requested_path = source_path.map(normalize_project_path);
        let include_project = export_type == ExportType::ProjectReport;
        let contributing_source_ids = if include_project {
            HashSet::new()
        } else if let Some(requested) = requested_path.as_deref() {
            compile_sources_for_path(context, requested)?
        } else {
            HashSet::new()
        };
        let mut markers = Vec::new();
        for entry in std::fs::read_dir(&directory).map_err(|error| {
            BackendError::new(
                "EXPORT_RESTRICTED_STATUS_FAILED",
                error.to_string(),
                true,
                false,
            )
        })? {
            let path = entry
                .map_err(|error| {
                    BackendError::new(
                        "EXPORT_RESTRICTED_STATUS_FAILED",
                        error.to_string(),
                        true,
                        false,
                    )
                })?
                .path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let raw = std::fs::read_to_string(&path).map_err(|error| {
                BackendError::new(
                    "EXPORT_RESTRICTED_STATUS_FAILED",
                    error.to_string(),
                    true,
                    false,
                )
            })?;
            let manifest: SourceManifest = serde_json::from_str(&raw).map_err(|error| {
                BackendError::new(
                    "EXPORT_RESTRICTED_STATUS_FAILED",
                    error.to_string(),
                    true,
                    false,
                )
            })?;
            let selected = include_project
                || requested_path.as_deref().is_some_and(|requested| {
                    normalize_project_path(&manifest.wiki_path) == requested
                })
                || contributing_source_ids.contains(&manifest.source_id);
            if selected && manifest.restricted_content {
                markers.push(format!(
                    "{}:{}:{}",
                    manifest.source_id,
                    manifest.current_version_id,
                    self.file_store.content_hash(raw.as_bytes())
                ));
            }
        }
        markers.sort();
        markers.dedup();
        Ok(markers)
    }

    /// Assemble the prompt for the matching `skills/html-*` Skill. Single-page
    /// jobs embed the source page body; project jobs embed purpose + page
    /// summaries. No secret or API key is ever placed in the prompt.
    ///
    /// `template` (when set) injects the skill's `template.html` styling
    /// baseline so the BYOK route — which lacks the Agent's skill workspace —
    /// still follows the template. `options` adjusts content clauses only.
    pub fn build_export_prompt(
        &self,
        context: &ProjectContext,
        export_type: ExportType,
        source_path: Option<&str>,
        search_service: &SearchService,
        language: &str,
        template: Option<&str>,
        options: &ExportContentOptions,
    ) -> Result<String, BackendError> {
        let page_paths = if export_type == ExportType::ProjectReport {
            Vec::new()
        } else {
            source_path.map(str::to_owned).into_iter().collect()
        };
        self.build_export_prompt_for_pages(
            context,
            export_type,
            &page_paths,
            search_service,
            language,
            template,
            options,
        )
    }

    pub fn build_export_prompt_for_pages(
        &self,
        context: &ProjectContext,
        export_type: ExportType,
        page_paths: &[String],
        search_service: &SearchService,
        language: &str,
        template: Option<&str>,
        options: &ExportContentOptions,
    ) -> Result<String, BackendError> {
        let skill = export_type.skill_folder();
        let purpose = self.file_store.read_markdown(context, "purpose.md").ok();
        // `language` is read by the command layer from SettingsService so this
        // service stays host-state-free and testable. Generated HTML prose
        // (article text, card bullets, report sections) follows it; the
        // structural HTML contract (single doctype, inlined CSS, no external
        // resources) is language-independent and stays as written below.

        // Embedding images as base64 is the only content option that relaxes
        // the self-contained contract; otherwise the strict "inline SVG only"
        // clause stands.
        let resource_clause = if options.embed_images {
            "Do NOT load external stylesheets, fonts, or scripts. You may embed images as base64 \
             data URIs where helpful, and use inline SVG otherwise; never reference external \
             image URLs."
        } else {
            "Do NOT load external stylesheets, fonts, images, or scripts (inline SVG only)."
        };

        let mut prompt = String::new();
        prompt.push_str(&format!(
            "You are generating a single, self-contained HTML document for a local Markdown wiki, \
             following the `{skill}` skill. Emit ONLY a complete standalone HTML document: a single \
             `<!doctype html>` page with all CSS inlined in a `<style>` block. {resource_clause} \
             Do NOT modify any project files. You may wrap the whole document in a fenced ```html \
             block; everything else will be discarded.\n",
        ));
        prompt.push_str(&crate::utils::i18n::language_instruction(language));
        prompt.push_str(" Write the visible document text in that language.\n");

        if let Some(purpose) = &purpose {
            prompt.push_str("\n--- Wiki purpose ---\n");
            prompt.push_str(purpose.trim());
            prompt.push('\n');
        }

        match export_type {
            ExportType::BeautifulRead => {
                let path = page_paths.first().ok_or_else(|| {
                    BackendError::new(
                        "EXPORT_SOURCE_REQUIRED",
                        "This export type requires a source page path.",
                        true,
                        true,
                    )
                })?;
                let page = search_service.read_page(context, path, &HashSet::new())?;
                prompt.push_str(&format!(
                    "\n--- Source page: {} ---\ntitle: {}\ntype: {:?}\ntags: {}\n\n",
                    path,
                    page.meta.title,
                    page.meta.page_type,
                    page.meta.tags.join(", ")
                ));
                prompt.push_str(page.body_markdown.trim());
                prompt.push('\n');
                prompt.push_str(
                    "\nProduce a long-form, readable article layout (serif body, generous \
                     measure, clear heading hierarchy, styled blockquotes and code).",
                );
                if options.include_frontmatter {
                    prompt.push_str(
                        " Render the page's frontmatter (title, type, tags) as a small metadata \
                         header near the top of the document.",
                    );
                }
            }
            ExportType::KnowledgeCard => {
                if page_paths.is_empty() {
                    return Err(BackendError::new(
                        "EXPORT_SOURCE_REQUIRED",
                        "Knowledge cards require at least one Wiki page.",
                        true,
                        true,
                    ));
                }
                for path in page_paths {
                    let page = search_service.read_page(context, path, &HashSet::new())?;
                    prompt.push_str(&format!(
                        "\n--- Source page: {} ---\ntitle: {}\ntype: {:?}\ntags: {}\n\n",
                        path,
                        page.meta.title,
                        page.meta.page_type,
                        page.meta.tags.join(", ")
                    ));
                    prompt.push_str(page.body_markdown.trim());
                    prompt.push('\n');
                }
                prompt.push_str(
                    "\nProduce one compact knowledge card per selected page (title, type, tags, \
                     3-6 key facts as bullets, and a one-line source attribution). Keep all cards \
                     in one self-contained document.",
                );
                if options.include_frontmatter {
                    prompt.push_str(
                        " Render each page's title, type, and tags as a small metadata header.",
                    );
                }
            }
            ExportType::ConceptMap => {
                if let Some(path) = page_paths.first() {
                    let page = search_service.read_page(context, path, &HashSet::new())?;
                    prompt.push_str(&format!(
                        "\n--- Centre page: {} ---\ntitle: {}\nlinks: {}\n\n",
                        path,
                        page.meta.title,
                        page.meta.wikilinks.join(", ")
                    ));
                    prompt.push_str(
                        "Render an inline-SVG concept map centred on this page with its wikilinks \
                         and the other selected pages as related nodes. Static (no JavaScript).",
                    );
                    for related in page_paths.iter().skip(1) {
                        let related_page =
                            search_service.read_page(context, related, &HashSet::new())?;
                        prompt.push_str(&format!(
                            "\nrelated: {} | title: {} | links: {}",
                            related,
                            related_page.meta.title,
                            related_page.meta.wikilinks.join(", ")
                        ));
                    }
                } else {
                    self.append_page_summaries(context, search_service, &mut prompt)?;
                    prompt.push_str(
                        "\nRender an inline-SVG concept map of the whole wiki using the page list \
                         and the cross-references in their links. Static (no JavaScript).",
                    );
                }
            }
            ExportType::ProjectReport => {
                self.append_page_summaries(context, search_service, &mut prompt)?;
                prompt.push_str(
                    "\nProduce a whole-wiki report: a purpose summary, an index of pages grouped \
                     by type, a short highlights section, and any structural notes you can infer \
                     from the page list.",
                );
            }
        }

        if let Some(name) = template {
            prompt.push_str(&format!(
                "\n--- Template: {} ({}) ---\n",
                name,
                style_direction(name)
            ));
            prompt.push_str(
                "Use the following HTML template as the styling baseline: keep its CSS structure \
                 and visual language, fill the {{...}} regions with generated content, and extend \
                 rather than contradict it.\n",
            );
            prompt.push_str(template_for(export_type).trim());
            prompt.push('\n');
        }

        Ok(prompt)
    }

    pub fn validate_workflow_scope(
        &self,
        export_type: ExportType,
        page_paths: &[String],
    ) -> Result<(), BackendError> {
        let valid = match export_type {
            ExportType::BeautifulRead => page_paths.len() == 1,
            ExportType::KnowledgeCard | ExportType::ConceptMap => !page_paths.is_empty(),
            ExportType::ProjectReport => page_paths.is_empty(),
        };
        if valid {
            Ok(())
        } else {
            Err(BackendError::new(
                "WORKFLOW_ARTIFACT_SCOPE_INVALID",
                "The selected artifact type requires a different Wiki page scope.",
                true,
                true,
            ))
        }
    }

    pub fn restricted_content_revision_for_pages(
        &self,
        context: &ProjectContext,
        export_type: ExportType,
        page_paths: &[String],
    ) -> Result<Option<String>, BackendError> {
        let mut markers = if export_type == ExportType::ProjectReport {
            self.restricted_source_markers(context, export_type, None)?
        } else {
            let mut markers = Vec::new();
            for path in page_paths {
                markers.extend(self.restricted_source_markers(context, export_type, Some(path))?);
            }
            markers
        };
        markers.sort();
        markers.dedup();
        if markers.is_empty() {
            return Ok(None);
        }
        let material =
            serde_json::to_vec(&(export_type, page_paths, markers)).map_err(|error| {
                BackendError::new(
                    "EXPORT_RESTRICTED_STATUS_FAILED",
                    error.to_string(),
                    true,
                    false,
                )
            })?;
        Ok(Some(self.file_store.content_hash(&material)))
    }

    pub fn validate_workflow_output_path(
        &self,
        context: &ProjectContext,
        output_relative: &str,
    ) -> Result<String, BackendError> {
        let normalized = normalize_project_path(output_relative);
        let export_root = self.workflow_export_root_relative(context)?;
        let export_prefix = format!("{export_root}/");
        if normalized != output_relative.replace('\\', "/")
            || !normalized.starts_with(&export_prefix)
            || normalized.contains("..")
            || !normalized.to_ascii_lowercase().ends_with(".html")
        {
            return Err(BackendError::new(
                "WORKFLOW_OUTPUT_PATH_INVALID",
                "Generated content must use a project-relative HTML path under the project export root.",
                true,
                true,
            ));
        }
        let absolute = context.resolve_project_path(&normalized)?;
        let absolute_export_root = context.resolve_project_path(&export_root)?;
        reject_case_collision(&absolute_export_root, &absolute)?;
        Ok(normalized)
    }

    pub fn workflow_export_root_relative(
        &self,
        context: &ProjectContext,
    ) -> Result<String, BackendError> {
        context.to_project_relative(&context.exports_dir.join("html"))
    }

    pub fn workflow_baseline_entries(
        &self,
        context: &ProjectContext,
        markdown_paths: &[String],
        output_relative: &str,
    ) -> Result<Vec<String>, BackendError> {
        let output_relative = self.validate_workflow_output_path(context, output_relative)?;
        let mut entries = self.workflow_resource_entries(context, markdown_paths)?;
        let target_hash = self
            .file_store
            .file_hash_if_exists(context, &output_relative)?
            .unwrap_or_else(|| "missing".into());
        entries.push(format!("target:{output_relative}:{target_hash}"));
        Ok(entries)
    }

    pub fn workflow_resource_entries(
        &self,
        context: &ProjectContext,
        markdown_paths: &[String],
    ) -> Result<Vec<String>, BackendError> {
        let mut resources = BTreeSet::new();
        for page_path in markdown_paths {
            let Ok(markdown) = self.file_store.read_markdown(context, page_path) else {
                continue;
            };
            for reference in markdown_resource_references(&markdown) {
                if let Some(relative) = resolve_markdown_resource(page_path, &reference)? {
                    resources.insert(relative);
                }
            }
        }
        let mut entries = Vec::new();
        for resource in resources {
            let hash = self
                .file_store
                .file_hash_if_exists(context, &resource)?
                .unwrap_or_else(|| "missing".into());
            entries.push(format!("resource:{resource}:{hash}"));
        }
        Ok(entries)
    }

    pub fn validate_html_artifact(
        &self,
        raw: &str,
    ) -> Result<ValidatedExportArtifact, BackendError> {
        let html = Self::extract_html(raw);
        if html.len() > MAX_EXPORT_HTML_BYTES {
            return Err(BackendError::new(
                "EXPORT_OUTPUT_TOO_LARGE",
                "The generated HTML exceeds the bounded artifact size.",
                true,
                false,
            ));
        }
        let lower = html.trim().to_ascii_lowercase();
        if !lower.starts_with("<!doctype html")
            || !lower.contains("<html")
            || !lower.contains("</html>")
            || lower.contains("<script")
        {
            return Err(BackendError::new(
                "EXPORT_OUTPUT_INVALID",
                "The generated artifact is not a complete script-free HTML document.",
                true,
                false,
            ));
        }
        if let Some(reference) = first_unsafe_html_reference(&html) {
            return Err(BackendError::new(
                "EXPORT_RESOURCE_INVALID",
                "The generated artifact contains an external or unsafe resource reference.",
                true,
                false,
            )
            .with_details(serde_json::json!({ "referenceKind": reference })));
        }
        let html = inject_export_csp(&html)?;
        if html.len() > MAX_EXPORT_HTML_BYTES {
            return Err(BackendError::new(
                "EXPORT_OUTPUT_TOO_LARGE",
                "The validated HTML exceeds the bounded artifact size.",
                true,
                false,
            ));
        }
        let bytes = html.as_bytes();
        let preview = ExportPreviewMetadata {
            content_type: "text/html".into(),
            byte_size: bytes.len() as u64,
            content_hash: self.file_store.content_hash(bytes),
            validation_passed: true,
        };
        Ok(ValidatedExportArtifact { html, preview })
    }

    pub fn write_html_checked(
        &self,
        context: &ProjectContext,
        output_relative: &str,
        html: &str,
        mode: crate::services::WriteMode,
    ) -> Result<(), BackendError> {
        let normalized = self.validate_workflow_output_path(context, output_relative)?;
        let export_root = self.workflow_export_root_relative(context)?;
        self.file_store.ensure_dir(context, &export_root)?;
        match mode {
            crate::services::WriteMode::CreateNew => self
                .file_store
                .write_markdown_create_new_atomic(context, &normalized, html),
            overwrite @ crate::services::WriteMode::OverwriteIfHashMatches(_) => self
                .file_store
                .write_markdown_checked(context, &normalized, html, overwrite),
        }
    }

    fn append_page_summaries(
        &self,
        context: &ProjectContext,
        search_service: &SearchService,
        prompt: &mut String,
    ) -> Result<(), BackendError> {
        let tree = search_service.scan_wiki(context, &HashSet::new())?;
        prompt.push_str("\n--- Pages ---\n");
        for page in &tree.pages {
            if page.path == "wiki/log.md" {
                continue;
            }
            prompt.push_str(&format!(
                "\n### {} ({:?})\npath: {}\ntags: {}\nlinks: {}\n",
                page.title,
                page.page_type,
                page.path,
                page.tags.join(", "),
                page.wikilinks.join(", ")
            ));
            if let Ok(content) = search_service.read_page(context, &page.path, &HashSet::new()) {
                let excerpt = truncate_chars(&content.body_markdown, PAGE_EXCERPT_CHARS);
                if !excerpt.is_empty() {
                    prompt.push_str(excerpt.trim());
                    prompt.push('\n');
                }
            }
        }
        Ok(())
    }

    /// Build the project-relative output path `exports/html/<slug>-<stamp>.html`.
    /// The filename is derived entirely from the export type / source — the UI
    /// never supplies it. Defense-in-depth: the result is asserted to stay
    /// inside `exports/html/` with no traversal.
    pub fn build_output_relative_path(
        &self,
        export_type: ExportType,
        source_path: Option<&str>,
    ) -> Result<String, BackendError> {
        // Project-wide jobs ignore any stray source path — a project report
        // named after a single page would be misleading in the history list.
        let slug = if export_type == ExportType::ProjectReport {
            export_type
                .skill_folder()
                .trim_start_matches("html-")
                .to_string()
        } else {
            source_path.map(slug_from_source).unwrap_or_else(|| {
                export_type
                    .skill_folder()
                    .trim_start_matches("html-")
                    .to_string()
            })
        };
        let stamp = compact_timestamp();
        let path = format!("{EXPORTS_HTML_DIR}/{slug}-{stamp}.html");
        if !path.starts_with("exports/html/") || path.contains("..") {
            return Err(BackendError::new(
                "EXPORT_PATH_INVALID",
                "Resolved export path escaped exports/html/.",
                true,
                true,
            ));
        }
        Ok(path)
    }

    /// Resolve an already-created HTML export for read/open operations.
    /// User-provided paths must remain project-relative, under `exports/html/`,
    /// point at `.html`, and already exist as a file.
    pub fn resolve_existing_html_export(
        &self,
        context: &ProjectContext,
        output_relative: &str,
    ) -> Result<PathBuf, BackendError> {
        let normalized = normalize_project_path(output_relative);
        let export_root = self.workflow_export_root_relative(context)?;
        if !normalized.starts_with(&format!("{export_root}/"))
            || normalized.contains("..")
            || !normalized.to_ascii_lowercase().ends_with(".html")
        {
            return Err(BackendError::new(
                "EXPORT_PATH_INVALID",
                "Exports may only be read from exports/html/*.html.",
                true,
                true,
            ));
        }

        let absolute = context.resolve_project_path(&normalized)?;
        if !absolute.is_file() {
            return Err(BackendError::new(
                "EXPORT_FILE_NOT_FOUND",
                "Export HTML file does not exist.",
                true,
                true,
            ));
        }

        Ok(absolute)
    }

    /// Strip a single ```html / ``` fence if the model wrapped the document,
    /// trim, and drop any trailing prose the model appended after the closing
    /// `</html>` (the skill says output the document only, but models sometimes
    /// add a one-line explanation). Bare HTML (no fence) is returned with the
    /// same trailing-prose trimming.
    pub fn extract_html(raw: &str) -> String {
        let trimmed = raw.trim();
        let body = if let Some(rest) = trimmed.strip_prefix("```html") {
            let inner = rest.trim_start_matches('\n');
            if let Some(without_close) = inner.strip_suffix("```") {
                without_close.trim().to_string()
            } else {
                // Opening fence with no matching close (model added prose after):
                // take up to the closing </html> if present.
                inner.trim().to_string()
            }
        } else if trimmed.starts_with("```") && trimmed.ends_with("```") && trimmed.len() > 6 {
            trimmed[3..trimmed.len() - 3].trim().to_string()
        } else {
            trimmed.to_string()
        };
        trim_trailing_prose(&body)
    }

    /// Write the HTML to the project-resolved output path. The path must already
    /// be a safe relative path under `exports/html/`; we additionally resolve it
    /// through `ProjectContext` (path-safety gate) before writing atomically.
    pub fn write_html(
        &self,
        context: &ProjectContext,
        output_relative: &str,
        html: &str,
    ) -> Result<(), BackendError> {
        self.write_html_checked(
            context,
            output_relative,
            html,
            crate::services::WriteMode::CreateNew,
        )
        .map_err(|mut error| {
            if error.code == "WORKFLOW_OUTPUT_PATH_INVALID" {
                error.code = "EXPORT_PATH_INVALID".to_owned();
            }
            error
        })
    }

    /// Append a record to `.app/exports.json` (created if missing).
    pub fn append_record(
        &self,
        context: &ProjectContext,
        record: ExportRecord,
    ) -> Result<(), BackendError> {
        let _guard = export_record_guard()?;
        let mut records = self.list_records(context)?;
        records.insert(0, record);
        self.file_store
            .write_json_atomic(context, EXPORTS_INDEX_PATH, &records)
    }

    pub fn remove_record_if_matches(
        &self,
        context: &ProjectContext,
        record_id: &str,
        output_path: &str,
        content_hash: &str,
    ) -> Result<bool, BackendError> {
        let _guard = export_record_guard()?;
        let mut records = self.list_records(context)?;
        let before = records.len();
        records.retain(|record| {
            !(record.id == record_id
                && record.output_path == output_path
                && record
                    .preview
                    .as_ref()
                    .is_some_and(|preview| preview.content_hash == content_hash))
        });
        if records.len() == before {
            return Ok(false);
        }
        self.file_store
            .write_json_atomic(context, EXPORTS_INDEX_PATH, &records)?;
        Ok(true)
    }

    /// Read all export records, newest first. Missing file ⇒ empty list.
    pub fn list_records(
        &self,
        context: &ProjectContext,
    ) -> Result<Vec<ExportRecord>, BackendError> {
        if !self.file_store.exists(context, EXPORTS_INDEX_PATH) {
            return Ok(Vec::new());
        }
        self.file_store.read_json(context, EXPORTS_INDEX_PATH)
    }

    pub fn list_records_with_bookmarks(
        &self,
        context: &ProjectContext,
        bookmark_ids: &HashSet<String>,
    ) -> Result<Vec<ExportRecord>, BackendError> {
        let mut records = self.list_records(context)?;
        for record in &mut records {
            record.bookmarked = bookmark_ids.contains(&record.id);
        }
        Ok(records)
    }

    /// Convenience for the command layer to build a record after a run.
    pub fn new_record(
        export_type: ExportType,
        title: String,
        source_path: Option<String>,
        output_path: String,
        route: ExportRoute,
        task_id: Option<String>,
    ) -> ExportRecord {
        ExportRecord {
            id: format!("export-{}", uuid::Uuid::new_v4()),
            export_type,
            title,
            source_path,
            output_path,
            created_at: now_rfc3339(),
            route,
            status: ExportStatus::Succeeded,
            bookmarked: false,
            task_id,
            preview: None,
        }
    }

    pub fn new_validated_record(
        export_type: ExportType,
        title: String,
        source_path: Option<String>,
        output_path: String,
        route: ExportRoute,
        task_id: Option<String>,
        preview: ExportPreviewMetadata,
    ) -> ExportRecord {
        let mut record =
            Self::new_record(export_type, title, source_path, output_path, route, task_id);
        record.preview = Some(preview);
        record
    }

    /// Build a Failed record so a botched export still appears in the history
    /// list with a retry entry. `output_path` is the would-be path (no file is
    /// written on failure); `route` is the intended route derived from the
    /// preference, since the resolved route may not be known at the point of
    /// failure.
    pub fn new_failed_record(
        export_type: ExportType,
        title: String,
        source_path: Option<String>,
        output_path: String,
        route: ExportRoute,
        task_id: Option<String>,
    ) -> ExportRecord {
        ExportRecord {
            id: format!("export-{}", uuid::Uuid::new_v4()),
            export_type,
            title,
            source_path,
            output_path,
            created_at: now_rfc3339(),
            route,
            status: ExportStatus::Failed,
            bookmarked: false,
            task_id,
            preview: None,
        }
    }
}

fn compile_sources_for_path(
    context: &ProjectContext,
    requested_path: &str,
) -> Result<HashSet<String>, BackendError> {
    let directory = context.resolve_project_path(".app/compile")?;
    let mut source_ids = HashSet::new();
    if !directory.exists() {
        return Ok(source_ids);
    }
    for entry in std::fs::read_dir(directory).map_err(export_restricted_error)? {
        let path = entry.map_err(export_restricted_error)?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let record: CompileConsumptionRecord =
            serde_json::from_slice(&std::fs::read(path).map_err(export_restricted_error)?)
                .map_err(export_restricted_error)?;
        if record
            .affected_paths
            .iter()
            .any(|path| normalize_project_path(path) == requested_path)
        {
            source_ids.extend(
                record
                    .source_versions
                    .into_iter()
                    .map(|source| source.source_id),
            );
        }
    }
    Ok(source_ids)
}

fn export_restricted_error(error: impl std::fmt::Display) -> BackendError {
    BackendError::new(
        "EXPORT_RESTRICTED_STATUS_FAILED",
        error.to_string(),
        true,
        false,
    )
}

fn slug_from_source(source: &str) -> String {
    let normalized = source.replace('\\', "/");
    let file_name = normalized
        .rsplit('/')
        .next()
        .unwrap_or(source)
        .trim_end_matches(".md");
    let slug: String = file_name
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let collapsed = slug
        .trim_matches('-')
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if collapsed.is_empty() {
        "export".to_string()
    } else {
        collapsed
    }
}

/// Compact Windows-safe timestamp (no colons) for the output filename.
fn compact_timestamp() -> String {
    chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string()
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let taken: String = trimmed.chars().take(max_chars).collect();
    format!("{}…", taken.trim_end())
}

/// Drop any trailing prose the model appended after the closing `</html>`.
/// The skill says output the document only, but models sometimes add a
/// one-line "Explanation: …". Finds the last `</html>` (case-insensitive) and
/// keeps everything up to and including it; if none is found the input is
/// returned unchanged so we never silently truncate legitimate content.
fn trim_trailing_prose(body: &str) -> String {
    let lower = body.to_ascii_lowercase();
    match lower.rfind("</html>") {
        Some(end) => body[..end + "</html>".len()].to_string(),
        None => body.to_string(),
    }
}

fn reject_case_collision(export_root: &Path, target: &Path) -> Result<(), BackendError> {
    let relative = target.strip_prefix(export_root).map_err(|_| {
        BackendError::new(
            "EXPORT_PATH_INVALID",
            "Export target escaped the validated export root.",
            false,
            true,
        )
    })?;
    let mut parent = export_root.to_path_buf();
    for component in relative.components() {
        let expected = component.as_os_str().to_string_lossy();
        if !parent.is_dir() {
            parent.push(component.as_os_str());
            continue;
        }
        let mut exact = None;
        for entry in std::fs::read_dir(&parent).map_err(|error| {
            BackendError::new("EXPORT_PATH_INVALID", error.to_string(), true, false)
        })? {
            let entry = entry.map_err(|error| {
                BackendError::new("EXPORT_PATH_INVALID", error.to_string(), true, false)
            })?;
            let actual = entry.file_name().to_string_lossy().into_owned();
            if actual == expected {
                exact = Some(entry.path());
                break;
            }
            if actual.eq_ignore_ascii_case(&expected) {
                return Err(BackendError::new(
                    "EXPORT_PATH_CASE_COLLISION",
                    "An export path component differs only by ASCII case.",
                    true,
                    true,
                ));
            }
        }
        parent = exact.unwrap_or_else(|| parent.join(component.as_os_str()));
    }
    if relative.as_os_str().is_empty() {
        return Err(BackendError::new(
            "EXPORT_PATH_INVALID",
            "Export target must name a file below the export root.",
            false,
            true,
        ));
    }
    Ok(())
}

fn markdown_resource_references(markdown: &str) -> Vec<String> {
    let mut references = Vec::new();
    let mut remaining = markdown;
    while let Some(start) = remaining.find("](") {
        let value = &remaining[start + 2..];
        let Some(end) = value.find(')') else {
            break;
        };
        references.push(value[..end].trim().to_string());
        remaining = &value[end + 1..];
    }
    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            if let Some((_, target)) = trimmed.split_once("]: ") {
                references.push(target.trim().to_string());
            } else if let Some((_, target)) = trimmed.split_once("]:") {
                references.push(target.trim().to_string());
            }
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            if matches!(
                key.trim().to_ascii_lowercase().as_str(),
                "cover" | "image" | "thumbnail" | "banner" | "asset"
            ) {
                references.push(value.trim().trim_matches(['\'', '"']).to_string());
            }
        }
    }
    for attribute in ["src", "href", "poster"] {
        references.extend(quoted_attribute_values(markdown, attribute));
    }
    references.sort();
    references.dedup();
    references
}

fn quoted_attribute_values(input: &str, attribute: &str) -> Vec<String> {
    let lower = input.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let name = attribute.as_bytes();
    let mut values = Vec::new();
    let mut index = 0usize;
    while index + name.len() < bytes.len() {
        if &bytes[index..index + name.len()] != name
            || (index > 0
                && !bytes[index - 1].is_ascii_whitespace()
                && !matches!(bytes[index - 1], b'<' | b'/'))
        {
            index += 1;
            continue;
        }
        let mut cursor = index + name.len();
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&b'=') {
            index += name.len();
            continue;
        }
        cursor += 1;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let Some(quote) = bytes
            .get(cursor)
            .copied()
            .filter(|value| matches!(value, b'\'' | b'"'))
        else {
            index = cursor;
            continue;
        };
        cursor += 1;
        let Some(end) = bytes[cursor..].iter().position(|value| *value == quote) else {
            break;
        };
        values.push(input[cursor..cursor + end].to_string());
        index = cursor + end + 1;
    }
    values
}

fn resolve_markdown_resource(
    page_path: &str,
    reference: &str,
) -> Result<Option<String>, BackendError> {
    let trimmed = reference.trim();
    let target = if trimmed.starts_with('<') {
        trimmed.trim_matches('<').trim_matches('>')
    } else {
        trimmed.split_ascii_whitespace().next().unwrap_or_default()
    };
    let value = target
        .split(['#', '?'])
        .next()
        .unwrap_or_default()
        .replace('\\', "/");
    let lower = value.to_ascii_lowercase();
    if value.is_empty()
        || value.starts_with('#')
        || lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with("data:")
        || lower.starts_with("asset://")
    {
        return Ok(None);
    }
    if value.starts_with('/') || value.contains(':') {
        return Err(BackendError::new(
            "EXPORT_RESOURCE_PATH_INVALID",
            "A selected Wiki page contains an unsafe local resource path.",
            true,
            true,
        ));
    }
    let root_relative = ["wiki/", "raw/", "exports/", "skills/", ".app/"]
        .iter()
        .any(|prefix| value.starts_with(prefix));
    let mut parts = if root_relative {
        Vec::new()
    } else {
        page_path
            .rsplit_once('/')
            .map(|(parent, _)| parent.split('/').map(str::to_owned).collect())
            .unwrap_or_default()
    };
    for segment in value.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(BackendError::new(
                        "EXPORT_RESOURCE_PATH_INVALID",
                        "A selected Wiki resource resolves outside the project.",
                        true,
                        true,
                    ));
                }
            }
            normal => parts.push(normal.to_string()),
        }
    }
    (!parts.is_empty())
        .then(|| parts.join("/"))
        .ok_or_else(|| {
            BackendError::new(
                "EXPORT_RESOURCE_PATH_INVALID",
                "A selected Wiki resource path is empty.",
                true,
                true,
            )
        })
        .map(Some)
}

fn first_unsafe_html_reference(html: &str) -> Option<&'static str> {
    let lower = html.to_ascii_lowercase();
    let attributes = parse_html_attributes(&lower);
    for (needle, kind) in [
        ("<iframe", "iframe"),
        ("<object", "object"),
        ("<embed", "embed"),
        ("<link", "link"),
        ("<base", "base"),
        ("@import", "css-import"),
    ] {
        if lower.contains(needle) {
            return Some(kind);
        }
    }
    if attributes.iter().any(|(name, value)| {
        name == "http-equiv"
            && value
                .as_deref()
                .is_some_and(|value| value.trim() == "refresh")
    }) {
        return Some("meta-refresh");
    }
    let mut css = lower.as_str();
    while let Some(index) = css.find("url(") {
        let value = css[index + 4..].trim_start();
        let Some(end) = value.find(')') else {
            return Some("css-url");
        };
        let reference = value[..end].trim().trim_matches(['\'', '"']);
        if !reference.starts_with("data:image/") && !reference.starts_with('#') {
            return Some("css-url");
        }
        css = &value[end + 1..];
    }
    if attributes
        .iter()
        .any(|(name, _)| name.len() > 2 && name.starts_with("on"))
    {
        return Some("event-handler");
    }
    for (attribute, kind) in [
        ("src", "src"),
        ("href", "href"),
        ("poster", "poster"),
        ("background", "background"),
        ("srcset", "srcset"),
        ("action", "action"),
        ("formaction", "formaction"),
    ] {
        for (_, value) in attributes.iter().filter(|(name, _)| name == attribute) {
            let Some(reference) = value.as_deref().map(str::trim) else {
                return Some(kind);
            };
            let safe = match kind {
                "src" | "poster" | "background" => reference.starts_with("data:image/"),
                "href" => reference.starts_with('#'),
                _ => false,
            };
            if !safe {
                return Some(kind);
            }
        }
    }
    None
}

fn parse_html_attributes(html: &str) -> Vec<(String, Option<String>)> {
    let bytes = html.as_bytes();
    let mut attributes = Vec::new();
    let mut cursor = 0usize;
    while let Some(relative_start) = html[cursor..].find('<') {
        let tag_start = cursor + relative_start + 1;
        let mut tag_end = tag_start;
        let mut quote = None;
        while tag_end < bytes.len() {
            let byte = bytes[tag_end];
            if matches!(byte, b'\'' | b'"') {
                if quote == Some(byte) {
                    quote = None;
                } else if quote.is_none() {
                    quote = Some(byte);
                }
            } else if byte == b'>' && quote.is_none() {
                break;
            }
            tag_end += 1;
        }
        if tag_end == bytes.len() {
            break;
        }
        let mut index = tag_start;
        while index < tag_end && !bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        while index < tag_end {
            while index < tag_end && (bytes[index].is_ascii_whitespace() || bytes[index] == b'/') {
                index += 1;
            }
            let name_start = index;
            while index < tag_end
                && !bytes[index].is_ascii_whitespace()
                && !matches!(bytes[index], b'=' | b'/' | b'>')
            {
                index += 1;
            }
            if name_start == index {
                index += 1;
                continue;
            }
            let name = html[name_start..index].to_string();
            while index < tag_end && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            if bytes.get(index) != Some(&b'=') {
                attributes.push((name, None));
                continue;
            }
            index += 1;
            while index < tag_end && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            let value = if let Some(delimiter @ (b'\'' | b'"')) = bytes.get(index).copied() {
                index += 1;
                let value_start = index;
                while index < tag_end && bytes[index] != delimiter {
                    index += 1;
                }
                let value = html[value_start..index].to_string();
                if index < tag_end {
                    index += 1;
                }
                value
            } else {
                let value_start = index;
                while index < tag_end && !bytes[index].is_ascii_whitespace() && bytes[index] != b'>'
                {
                    index += 1;
                }
                html[value_start..index].to_string()
            };
            attributes.push((name, Some(value)));
        }
        cursor = tag_end + 1;
    }
    attributes
}

fn inject_export_csp(html: &str) -> Result<String, BackendError> {
    const CSP: &str = "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src data:; style-src 'unsafe-inline'; font-src data:; base-uri 'none'; form-action 'none'\">";
    let lower = html.to_ascii_lowercase();
    if lower.contains("content-security-policy") {
        if html.contains(CSP) && lower.matches("content-security-policy").count() == 1 {
            return Ok(html.to_string());
        }
        return Err(BackendError::new(
            "EXPORT_RESOURCE_INVALID",
            "The generated HTML contains an untrusted Content Security Policy override.",
            true,
            false,
        ));
    }
    if let Some(head) = lower.find("<head") {
        let end = lower[head..]
            .find('>')
            .map(|offset| head + offset + 1)
            .ok_or_else(|| {
                BackendError::new(
                    "EXPORT_OUTPUT_INVALID",
                    "The generated HTML contains an incomplete head element.",
                    true,
                    false,
                )
            })?;
        let mut secured = String::with_capacity(html.len() + CSP.len());
        secured.push_str(&html[..end]);
        secured.push_str(CSP);
        secured.push_str(&html[end..]);
        return Ok(secured);
    }
    let html_start = lower.find("<html").ok_or_else(|| {
        BackendError::new(
            "EXPORT_OUTPUT_INVALID",
            "The generated HTML has no root element.",
            true,
            false,
        )
    })?;
    let end = lower[html_start..]
        .find('>')
        .map(|offset| html_start + offset + 1)
        .ok_or_else(|| {
            BackendError::new(
                "EXPORT_OUTPUT_INVALID",
                "The generated HTML contains an incomplete root element.",
                true,
                false,
            )
        })?;
    let mut secured = String::with_capacity(html.len() + CSP.len() + 13);
    secured.push_str(&html[..end]);
    secured.push_str("<head>");
    secured.push_str(CSP);
    secured.push_str("</head>");
    secured.push_str(&html[end..]);
    Ok(secured)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::paths::ProjectContext;
    use crate::services::{BookmarkService, WriteMode};
    use std::collections::HashSet;
    use std::path::PathBuf;

    fn tmp_context(suffix: &str) -> (ProjectContext, PathBuf) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("llm-wiki-export-{stamp}-{suffix}"));
        std::fs::create_dir_all(&root).unwrap();
        (ProjectContext::new("project-1", root.clone()), root)
    }

    fn write_file(context: &ProjectContext, rel: &str, body: &str) {
        let path = context.resolve_project_path(rel).unwrap();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, body).unwrap();
    }

    #[test]
    fn restricted_source_status_covers_single_and_project_exports() {
        let (context, root) = tmp_context("restricted-status");
        write_file(
            &context,
            ".app/sources/restricted.json",
            r#"{
                "schemaVersion": 3,
                "sourceId": "restricted",
                "sourceKind": "web_page",
                "currentVersionId": "version-1",
                "wikiPath": "wiki/sources/restricted.md",
                "aliases": [],
                "origins": ["https://example.com/restricted"],
                "title": "Restricted",
                "importedAt": "2026-07-27T00:00:00Z",
                "versions": [],
                "compiledConsumptions": [],
                "restrictedContent": true,
                "restrictedIdentitySummary": "Reader — @reader",
                "timeline": []
            }"#,
        );
        write_file(
            &context,
            ".app/sources/public.json",
            r#"{
                "schemaVersion": 3,
                "sourceId": "public",
                "sourceKind": "web_page",
                "currentVersionId": "version-1",
                "wikiPath": "wiki/sources/public.md",
                "aliases": [],
                "origins": ["https://example.com/public"],
                "title": "Public",
                "importedAt": "2026-07-27T00:00:00Z",
                "versions": [],
                "compiledConsumptions": [],
                "restrictedContent": false,
                "timeline": []
            }"#,
        );

        let service = ExportService::default();
        assert_eq!(
            service
                .restricted_source_count(
                    &context,
                    ExportType::BeautifulRead,
                    Some("wiki/sources/restricted.md"),
                )
                .unwrap(),
            1
        );
        assert_eq!(
            service
                .restricted_source_count(
                    &context,
                    ExportType::BeautifulRead,
                    Some("wiki/sources/public.md"),
                )
                .unwrap(),
            0
        );
        assert_eq!(
            service
                .restricted_source_count(&context, ExportType::ProjectReport, None)
                .unwrap(),
            1
        );
        write_file(
            &context,
            ".app/compile/task-derived.json",
            r#"{
                "schemaVersion": 1,
                "compileTaskId": "task-derived",
                "route": "agent",
                "consumedAt": "2026-07-27T00:10:00Z",
                "sourceVersions": [{
                    "sourceId": "restricted",
                    "versionId": "version-1",
                    "contentHash": "hash-1"
                }],
                "affectedPaths": ["wiki/concepts/derived.md"],
                "checkpoint": null
            }"#,
        );
        assert_eq!(
            service
                .restricted_source_count(
                    &context,
                    ExportType::BeautifulRead,
                    Some("wiki/concepts/derived.md"),
                )
                .unwrap(),
            1
        );
        let first_revision = service
            .restricted_content_revision_for_pages(
                &context,
                ExportType::BeautifulRead,
                &["wiki/sources/restricted.md".into()],
            )
            .unwrap()
            .unwrap();
        let restricted_path = context
            .resolve_project_path(".app/sources/restricted.json")
            .unwrap();
        let replaced = std::fs::read_to_string(&restricted_path)
            .unwrap()
            .replace(
                "\"sourceId\": \"restricted\"",
                "\"sourceId\": \"replacement\"",
            )
            .replace(
                "\"currentVersionId\": \"version-1\"",
                "\"currentVersionId\": \"version-2\"",
            );
        std::fs::write(&restricted_path, replaced).unwrap();
        let second_revision = service
            .restricted_content_revision_for_pages(
                &context,
                ExportType::BeautifulRead,
                &["wiki/sources/restricted.md".into()],
            )
            .unwrap()
            .unwrap();
        assert_ne!(first_revision, second_revision);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn output_path_is_scoped_under_exports_html() {
        let service = ExportService::default();
        let path = service
            .build_output_relative_path(ExportType::BeautifulRead, Some("wiki/concepts/Agent.md"))
            .unwrap();
        assert!(path.starts_with("exports/html/"), "got {path}");
        assert!(path.ends_with(".html"));
        assert!(path.contains("agent"));
        assert!(!path.contains(".."));
    }

    #[test]
    fn output_path_for_project_type_has_no_source_slug() {
        let service = ExportService::default();
        let path = service
            .build_output_relative_path(ExportType::ProjectReport, None)
            .unwrap();
        assert!(path.starts_with("exports/html/project-report-"));
        assert!(path.ends_with(".html"));
    }

    #[test]
    fn output_path_cjk_and_spaces_collapse_to_slug() {
        let service = ExportService::default();
        let path = service
            .build_output_relative_path(ExportType::KnowledgeCard, Some("wiki/概念/My Page!.md"))
            .unwrap();
        let file = path.rsplit('/').next().unwrap();
        assert!(file.starts_with("my-page"));
        assert!(file.ends_with(".html"));
        assert!(!file.contains(' '));
    }

    #[test]
    fn slug_falls_back_for_empty_names() {
        let service = ExportService::default();
        let path = service
            .build_output_relative_path(ExportType::KnowledgeCard, Some("wiki/!!!.md"))
            .unwrap();
        assert!(path.starts_with("exports/html/export-"));
    }

    #[test]
    fn extract_html_strips_fenced_block() {
        let raw = "```html\n<!doctype html><html></html>\n```";
        assert_eq!(
            ExportService::extract_html(raw),
            "<!doctype html><html></html>"
        );
        // Bare fence without language tag.
        assert_eq!(
            ExportService::extract_html("```\n<!doctype html>\n```"),
            "<!doctype html>"
        );
        // No fence → unchanged (trimmed).
        assert_eq!(
            ExportService::extract_html("  <!doctype html><p>hi</p>  "),
            "<!doctype html><p>hi</p>"
        );
    }

    #[test]
    fn extract_html_leaves_real_content_alone() {
        let raw = "<!doctype html>\n<html><body><pre>```\ncode\n```</pre></body></html>";
        // Has a ``` inside but is not itself a wrapped fence → returned trimmed.
        assert_eq!(ExportService::extract_html(raw), raw.trim());
    }

    #[test]
    fn extract_html_drops_trailing_prose_after_closing_tag() {
        // Model emitted the doc (no closing fence) then a one-line explanation.
        let raw = "```html\n<!doctype html><html></html>\n\nExplanation: I generated this.";
        assert_eq!(
            ExportService::extract_html(raw),
            "<!doctype html><html></html>"
        );
        // Case-insensitive closing tag.
        let raw2 = "<html><body>x</body></HTML>\n\nHope this helps!";
        assert_eq!(
            ExportService::extract_html(raw2),
            "<html><body>x</body></HTML>"
        );
    }

    #[test]
    fn build_output_relative_path_ignores_source_for_project_report() {
        // ProjectReport is project-wide; the slug should fall back to the skill
        // folder name, not the source path the UI might still send.
        let service = ExportService::default();
        let path = service
            .build_output_relative_path(ExportType::ProjectReport, Some("wiki/concepts/agent.md"))
            .expect("project report path");
        assert!(
            path.starts_with("exports/html/project-report-"),
            "expected project-report slug, got {path}"
        );
        assert!(path.ends_with(".html"));
        assert!(!path.contains("agent"));
    }

    #[test]
    fn write_html_rejects_escape_attempts() {
        let (context, root) = tmp_context("escape");
        let service = ExportService::default();
        let err = service
            .write_html(&context, "exports/html/../../wiki/x.html", "<p/>")
            .expect_err("escape must be rejected");
        assert_eq!(err.code, "EXPORT_PATH_INVALID");
        assert!(!context.wiki_dir.join("x.html").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn write_html_creates_file_under_exports() {
        let (context, root) = tmp_context("write");
        let service = ExportService::default();
        service
            .write_html(
                &context,
                "exports/html/agent-1.html",
                "<!doctype html><p>hi</p>",
            )
            .unwrap();
        let on_disk = std::fs::read_to_string(
            context
                .resolve_project_path("exports/html/agent-1.html")
                .unwrap(),
        )
        .unwrap();
        assert!(on_disk.starts_with("<!doctype html>"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolve_existing_html_export_accepts_valid_cjk_path() {
        let (context, root) = tmp_context("resolve-cjk");
        let service = ExportService::default();
        write_file(&context, "exports/html/报告 index.html", "<!doctype html>");

        let resolved = service
            .resolve_existing_html_export(&context, "exports/html/报告 index.html")
            .unwrap();

        assert_eq!(
            resolved,
            context
                .resolve_project_path("exports/html/报告 index.html")
                .unwrap()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolve_existing_html_export_rejects_escape_and_non_html_paths() {
        let (context, root) = tmp_context("resolve-invalid");
        let service = ExportService::default();
        write_file(&context, "exports/html/ok.html", "<!doctype html>");

        let escape = service
            .resolve_existing_html_export(&context, "exports/html/../../wiki/x.html")
            .expect_err("path traversal must be rejected");
        let wrong_dir = service
            .resolve_existing_html_export(&context, "wiki/x.html")
            .expect_err("non-export path must be rejected");
        let wrong_ext = service
            .resolve_existing_html_export(&context, "exports/html/x.txt")
            .expect_err("non-html path must be rejected");

        assert_eq!(escape.code, "EXPORT_PATH_INVALID");
        assert_eq!(wrong_dir.code, "EXPORT_PATH_INVALID");
        assert_eq!(wrong_ext.code, "EXPORT_PATH_INVALID");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolve_existing_html_export_requires_existing_file() {
        let (context, root) = tmp_context("resolve-missing");
        let service = ExportService::default();

        let err = service
            .resolve_existing_html_export(&context, "exports/html/missing.html")
            .expect_err("missing export must be rejected");

        assert_eq!(err.code, "EXPORT_FILE_NOT_FOUND");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn record_round_trip_appends_and_lists() {
        let (context, root) = tmp_context("records");
        let service = ExportService::default();
        assert!(service.list_records(&context).unwrap().is_empty());

        let first = ExportService::new_record(
            ExportType::BeautifulRead,
            "Agent".into(),
            Some("wiki/concepts/agent.md".into()),
            "exports/html/agent-1.html".into(),
            ExportRoute::Byok,
            Some("task-1".into()),
        );
        let second = ExportService::new_record(
            ExportType::ProjectReport,
            "Report".into(),
            None,
            "exports/html/project-report-2.html".into(),
            ExportRoute::Agent,
            Some("task-2".into()),
        );
        service.append_record(&context, first.clone()).unwrap();
        service.append_record(&context, second.clone()).unwrap();

        let listed = service.list_records(&context).unwrap();
        assert_eq!(listed.len(), 2);
        // Newest first.
        assert_eq!(listed[0].id, second.id);
        assert_eq!(listed[1].id, first.id);
        assert_eq!(
            listed[1].source_path.as_deref(),
            Some("wiki/concepts/agent.md")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn list_records_with_bookmarks_marks_matching_record_id() {
        let (context, root) = tmp_context("record-bookmarks");
        let service = ExportService::default();
        let first = ExportService::new_record(
            ExportType::BeautifulRead,
            "Agent".into(),
            Some("wiki/concepts/agent.md".into()),
            "exports/html/agent-1.html".into(),
            ExportRoute::Byok,
            Some("task-1".into()),
        );
        let second = ExportService::new_record(
            ExportType::ProjectReport,
            "Report".into(),
            None,
            "exports/html/project-report-2.html".into(),
            ExportRoute::Agent,
            Some("task-2".into()),
        );
        let second_id = second.id.clone();
        service.append_record(&context, first).unwrap();
        service.append_record(&context, second).unwrap();
        let bookmark_ids = HashSet::from([second_id.clone()]);

        let listed = service
            .list_records_with_bookmarks(&context, &bookmark_ids)
            .unwrap();

        assert!(
            listed
                .iter()
                .find(|record| record.id == second_id)
                .unwrap()
                .bookmarked
        );
        assert!(listed.iter().any(|record| !record.bookmarked));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn toggle_export_bookmark_keeps_missing_output_as_bookmarkable_history() {
        let (context, root) = tmp_context("bookmark-missing-output");
        let record = ExportService::new_record(
            ExportType::BeautifulRead,
            "Agent".into(),
            Some("wiki/concepts/agent.md".into()),
            "exports/html/missing.html".into(),
            ExportRoute::Byok,
            Some("task-1".into()),
        );

        let result = BookmarkService::default()
            .toggle_export_html(&context, &record)
            .unwrap();
        let bookmark_ids = BookmarkService::default()
            .export_record_ids(&context)
            .unwrap();

        assert!(result.bookmarked);
        assert!(bookmark_ids.contains(&record.id));
        assert!(!context
            .resolve_project_path(&record.output_path)
            .unwrap()
            .exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_export_records_are_not_bookmarkable() {
        let (context, root) = tmp_context("bookmark-failed");
        let record = ExportService::new_failed_record(
            ExportType::BeautifulRead,
            "Agent".into(),
            Some("wiki/concepts/agent.md".into()),
            "exports/html/agent-failed.html".into(),
            ExportRoute::Byok,
            Some("task-1".into()),
        );

        let err = BookmarkService::default()
            .toggle_export_html(&context, &record)
            .expect_err("failed exports cannot be bookmarked");

        assert_eq!(err.code, "EXPORT_BOOKMARK_UNAVAILABLE");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn build_prompt_includes_source_body_for_single_page() {
        let (context, root) = tmp_context("prompt-single");
        write_file(
            &context,
            "wiki/concepts/agent.md",
            "---\ntitle: Agent\ntype: concept\ntags: [ai]\n---\n\n# Agent\n\nAn agent acts.",
        );
        write_file(&context, "wiki/index.md", "# Index\n");
        write_file(&context, "wiki/log.md", "# Log\n");
        write_file(&context, "purpose.md", "# Purpose\n\nExplain agents.");

        let prompt = ExportService::default()
            .build_export_prompt(
                &context,
                ExportType::BeautifulRead,
                Some("wiki/concepts/agent.md"),
                &SearchService::default(),
                "en",
                None,
                &ExportContentOptions::default(),
            )
            .unwrap();
        assert!(prompt.contains("html-beautiful-read"));
        assert!(prompt.contains("An agent acts."));
        assert!(prompt.contains("Explain agents."));
        assert!(prompt.contains("Respond in English."));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn build_prompt_requires_source_for_single_page_type() {
        let (context, root) = tmp_context("prompt-nosrc");
        write_file(&context, "wiki/index.md", "# Index\n");
        write_file(&context, "wiki/log.md", "# Log\n");
        let err = ExportService::default()
            .build_export_prompt(
                &context,
                ExportType::KnowledgeCard,
                None,
                &SearchService::default(),
                "en",
                None,
                &ExportContentOptions::default(),
            )
            .expect_err("source required");
        assert_eq!(err.code, "EXPORT_SOURCE_REQUIRED");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn build_prompt_lists_pages_for_project_report() {
        let (context, root) = tmp_context("prompt-report");
        write_file(
            &context,
            "wiki/concepts/agent.md",
            "---\ntitle: Agent\ntype: concept\n---\n\n# Agent\n\n[[react]]",
        );
        write_file(
            &context,
            "wiki/concepts/react.md",
            "---\ntitle: ReAct\ntype: concept\n---\n\n# ReAct\n\n[[agent]]",
        );
        write_file(&context, "wiki/index.md", "# Index\n");
        write_file(&context, "wiki/log.md", "# Log\n");

        let prompt = ExportService::default()
            .build_export_prompt(
                &context,
                ExportType::ProjectReport,
                None,
                &SearchService::default(),
                "en",
                None,
                &ExportContentOptions::default(),
            )
            .unwrap();
        assert!(prompt.contains("html-project-report"));
        assert!(prompt.contains("wiki/concepts/agent.md"));
        assert!(prompt.contains("wiki/concepts/react.md"));
        // log.md summaries are skipped.
        assert!(!prompt.contains("path: wiki/log.md"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn build_prompt_injects_user_language_preference() {
        let (context, root) = tmp_context("prompt-i18n");
        write_file(
            &context,
            "wiki/concepts/agent.md",
            "---\ntitle: Agent\ntype: concept\n---\n\n# Agent\n\nAn agent acts.",
        );
        write_file(&context, "wiki/index.md", "# Index\n");
        write_file(&context, "wiki/log.md", "# Log\n");
        write_file(&context, "purpose.md", "# Purpose\n\nExplain agents.");

        let service = ExportService::default();
        // zh-CN produces a Simplified Chinese instruction.
        let zh_prompt = service
            .build_export_prompt(
                &context,
                ExportType::BeautifulRead,
                Some("wiki/concepts/agent.md"),
                &SearchService::default(),
                "zh-CN",
                None,
                &ExportContentOptions::default(),
            )
            .unwrap();
        assert!(zh_prompt.contains("Respond in Simplified Chinese."));
        assert!(zh_prompt.contains("Write the visible document text"));
        // Structural HTML contract is language-independent and still present.
        assert!(zh_prompt.contains("html-beautiful-read"));
        // English produces an English instruction.
        let en_prompt = service
            .build_export_prompt(
                &context,
                ExportType::BeautifulRead,
                Some("wiki/concepts/agent.md"),
                &SearchService::default(),
                "en",
                None,
                &ExportContentOptions::default(),
            )
            .unwrap();
        assert!(en_prompt.contains("Respond in English."));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn build_prompt_injects_template_when_requested() {
        let (context, root) = tmp_context("prompt-tmpl");
        write_file(
            &context,
            "wiki/concepts/agent.md",
            "---\ntitle: Agent\ntype: concept\n---\n\n# Agent\n\nAn agent acts.",
        );
        write_file(&context, "wiki/index.md", "# Index\n");
        write_file(&context, "wiki/log.md", "# Log\n");

        let service = ExportService::default();
        // No template ⇒ the styling baseline is not in the prompt.
        let no_template = service
            .build_export_prompt(
                &context,
                ExportType::BeautifulRead,
                Some("wiki/concepts/agent.md"),
                &SearchService::default(),
                "en",
                None,
                &ExportContentOptions::default(),
            )
            .unwrap();
        assert!(!no_template.contains("--- Template:"));
        assert!(!no_template.contains("Source Serif Pro"));

        // Template selected ⇒ its CSS (baked-in template.html) is injected.
        let with_template = service
            .build_export_prompt(
                &context,
                ExportType::BeautifulRead,
                Some("wiki/concepts/agent.md"),
                &SearchService::default(),
                "en",
                Some("modern-sans"),
                &ExportContentOptions::default(),
            )
            .unwrap();
        assert!(with_template.contains("--- Template: modern-sans"));
        assert!(with_template.contains("modern sans-serif system font stack"));
        // The skill's template.html styling baseline is present.
        assert!(with_template.contains("Source Serif Pro"));
        assert!(with_template.contains("{{body}}"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn build_prompt_respects_content_options() {
        let (context, root) = tmp_context("prompt-opts");
        write_file(
            &context,
            "wiki/concepts/agent.md",
            "---\ntitle: Agent\ntype: concept\ntags: [ai]\n---\n\n# Agent\n\nAn agent acts.",
        );
        write_file(&context, "wiki/index.md", "# Index\n");
        write_file(&context, "wiki/log.md", "# Log\n");

        let service = ExportService::default();

        // Defaults: frontmatter on, images off.
        let defaults = service
            .build_export_prompt(
                &context,
                ExportType::BeautifulRead,
                Some("wiki/concepts/agent.md"),
                &SearchService::default(),
                "en",
                None,
                &ExportContentOptions::default(),
            )
            .unwrap();
        assert!(defaults.contains("inline SVG only"));
        assert!(!defaults.contains("base64 data URIs"));
        assert!(defaults.contains("frontmatter"));

        // Images on relaxes the clause; frontmatter off drops the instruction.
        let images_on = ExportContentOptions {
            include_frontmatter: false,
            embed_css: true,
            embed_images: true,
        };
        let prompt = service
            .build_export_prompt(
                &context,
                ExportType::BeautifulRead,
                Some("wiki/concepts/agent.md"),
                &SearchService::default(),
                "en",
                None,
                &images_on,
            )
            .unwrap();
        assert!(prompt.contains("base64 data URIs"));
        assert!(!prompt.contains("inline SVG only."));
        assert!(!prompt.contains("Render the page's frontmatter"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn new_failed_record_marks_failed_status() {
        let record = ExportService::new_failed_record(
            ExportType::ProjectReport,
            "Project report".into(),
            None,
            "exports/html/project-report-x.html".into(),
            ExportRoute::Agent,
            Some("task-1".into()),
        );
        assert_eq!(record.status, ExportStatus::Failed);
        assert_eq!(record.export_type, ExportType::ProjectReport);
        assert!(record.id.starts_with("export-"));
        assert_eq!(record.task_id.as_deref(), Some("task-1"));
    }

    #[test]
    fn workflow_artifact_scopes_are_strict_for_all_four_types() {
        let service = ExportService::default();
        let page = vec!["wiki/concepts/one.md".into()];
        assert!(service
            .validate_workflow_scope(ExportType::BeautifulRead, &page)
            .is_ok());
        assert!(service
            .validate_workflow_scope(ExportType::BeautifulRead, &[])
            .is_err());
        assert!(service
            .validate_workflow_scope(
                ExportType::BeautifulRead,
                &["wiki/a.md".into(), "wiki/b.md".into()],
            )
            .is_err());
        for export_type in [ExportType::KnowledgeCard, ExportType::ConceptMap] {
            assert!(service.validate_workflow_scope(export_type, &page).is_ok());
            assert!(service.validate_workflow_scope(export_type, &[]).is_err());
        }
        assert!(service
            .validate_workflow_scope(ExportType::ProjectReport, &[])
            .is_ok());
        assert!(service
            .validate_workflow_scope(ExportType::ProjectReport, &page)
            .is_err());
    }

    #[test]
    fn workflow_output_paths_reject_escape_absolute_and_case_collisions_but_allow_cjk() {
        let (context, root) = tmp_context("workflow-output-paths");
        let service = ExportService::default();
        write_file(&context, "exports/html/Report.HTML", "old");
        std::fs::create_dir_all(context.root.join("exports/html/Reports")).unwrap();

        for invalid in [
            "exports/html/../../wiki/out.html",
            "C:/outside/out.html",
            "/outside/out.html",
            "wiki/out.html",
            "exports/html/report.html",
            "exports/html/reports/new.html",
        ] {
            assert!(
                service
                    .validate_workflow_output_path(&context, invalid)
                    .is_err(),
                "{invalid} must be rejected"
            );
        }
        assert_eq!(
            service
                .validate_workflow_output_path(&context, "exports/html/项目 报告.html")
                .unwrap(),
            "exports/html/项目 报告.html"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn workflow_custom_export_layout_writes_records_and_resolves_preview() {
        let (mut context, root) = tmp_context("workflow-custom-export-layout");
        context.exports_dir = root.join("published");
        let service = ExportService::default();
        let output = "published/html/自定义.html";
        let artifact = service
            .validate_html_artifact("<!doctype html><html><body>Custom</body></html>")
            .unwrap();
        service
            .write_html_checked(&context, output, &artifact.html, WriteMode::CreateNew)
            .unwrap();
        let record = ExportService::new_validated_record(
            ExportType::BeautifulRead,
            "Custom".into(),
            Some("wiki/custom.md".into()),
            output.into(),
            ExportRoute::Byok,
            Some("task-custom".into()),
            artifact.preview,
        );
        service.append_record(&context, record).unwrap();
        assert_eq!(
            service.list_records(&context).unwrap()[0].output_path,
            output
        );
        assert_eq!(
            service
                .resolve_existing_html_export(&context, output)
                .unwrap(),
            root.join("published/html/自定义.html")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn workflow_output_path_rejects_internal_symlink_to_outside_root() {
        use std::os::unix::fs::symlink;

        let (context, root) = tmp_context("workflow-output-symlink");
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.join("exports")).unwrap();
        symlink(outside.path(), root.join("exports/html")).unwrap();
        assert!(ExportService::default()
            .validate_workflow_output_path(&context, "exports/html/escape.html")
            .is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn workflow_output_path_rejects_internal_junction_to_outside_root() {
        let (context, root) = tmp_context("workflow-output-junction");
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.join("exports")).unwrap();
        let command = format!(
            "mklink /J {} {}",
            root.join("exports").join("html").display(),
            outside.path().display()
        );
        let output = std::process::Command::new("cmd")
            .args(["/C", &command])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{command} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(ExportService::default()
            .validate_workflow_output_path(&context, "exports/html/escape.html")
            .is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn workflow_resources_are_hashed_and_unsafe_references_fail_closed() {
        let (context, root) = tmp_context("workflow-resources");
        let service = ExportService::default();
        write_file(
            &context,
            "wiki/concepts/page.md",
            "# Page\n\n![local](../assets/图.png)\n",
        );
        write_file(&context, "wiki/assets/图.png", "image-bytes");
        let entries = service
            .workflow_resource_entries(&context, &["wiki/concepts/page.md".into()])
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].starts_with("resource:wiki/assets/图.png:"));

        write_file(
            &context,
            "wiki/concepts/page.md",
            "# Page\n\n![unsafe](C:/outside.png)\n",
        );
        assert_eq!(
            service
                .workflow_resource_entries(&context, &["wiki/concepts/page.md".into()])
                .unwrap_err()
                .code,
            "EXPORT_RESOURCE_PATH_INVALID"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn workflow_html_validation_is_bounded_and_self_contained() {
        let service = ExportService::default();
        let valid = service
            .validate_html_artifact(
                "<!doctype html><html><body><a href=\"#topic\">Topic</a><img src=\"data:image/png;base64,AA==\"></body></html>",
            )
            .unwrap();
        assert!(valid.preview.validation_passed);
        assert_eq!(valid.preview.content_type, "text/html");
        assert!(valid.html.contains("Content-Security-Policy"));
        for invalid in [
            "<html><body>missing doctype</body></html>",
            "<!doctype html><html><script>alert(1)</script></html>",
            "<!doctype html><html><img src=\"https://example.com/x.png\"></html>",
            "<!doctype html><html><body onload = \"fetch('https://example.com')\"></body></html>",
            "<!doctype html><html><style>body{background:url(https://example.com/x.png)}</style></html>",
            "<!doctype html><html><style>body{background:url(</style></html>",
            "<!doctype html><html><iframe srcdoc=\"x\"></iframe></html>",
            "<!doctype html><html><meta http-equiv=\"refresh\" content=\"0;url=https://example.com\"></html>",
            "<!doctype html><html><meta http-equiv = \"refresh\" content = \"0;url=https://example.com\"></html>",
            "<!doctype html><html><a href = https://example.com>leave</a></html>",
        ] {
            assert!(service.validate_html_artifact(invalid).is_err());
        }
    }

    /// Each template.html must be styling-only — it must not carry schema/lint
    /// directives that could leak into agent behavior. Loads the templates from
    /// the checked-in skills folder.
    #[test]
    fn templates_carry_no_schema_or_lint_directives() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        for export_type in ExportType::ALL {
            let folder = export_type.skill_folder();
            let template_path = format!("{manifest_dir}/templates/skills/{folder}/template.html");
            let template = std::fs::read_to_string(&template_path)
                .unwrap_or_else(|err| panic!("missing template {template_path}: {err}"));
            let lower = template.to_ascii_lowercase();
            assert!(
                !lower.contains("schema.md")
                    && !lower.contains("wiki-lint")
                    && !lower.contains("lint rule"),
                "template {template_path} must not reference schema.md or lint rules"
            );
            // No external resource loads — previews must render offline in a sandbox.
            assert!(
                !lower.contains("<script") && !lower.contains("src=\"http"),
                "template {template_path} must be self-contained (no scripts/remote sources)"
            );
        }
    }
}
