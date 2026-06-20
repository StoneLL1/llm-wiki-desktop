use crate::errors::BackendError;
use crate::models::export::{ExportRecord, ExportRoute, ExportStatus, ExportType};
use crate::models::paths::ProjectContext;
use crate::services::file_store::FileStore;
use crate::services::SearchService;
use crate::utils::time_utils::now_rfc3339;

const EXPORTS_HTML_DIR: &str = "exports/html";
const EXPORTS_INDEX_PATH: &str = ".app/exports.json";
const PAGE_EXCERPT_CHARS: usize = 600;

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
    /// Assemble the prompt for the matching `skills/html-*` Skill. Single-page
    /// jobs embed the source page body; project jobs embed purpose + page
    /// summaries. No secret or API key is ever placed in the prompt.
    pub fn build_export_prompt(
        &self,
        context: &ProjectContext,
        export_type: ExportType,
        source_path: Option<&str>,
        search_service: &SearchService,
    ) -> Result<String, BackendError> {
        let skill = export_type.skill_folder();
        let purpose = self.file_store.read_markdown(context, "purpose.md").ok();

        let mut prompt = String::new();
        prompt.push_str(&format!(
            "You are generating a single, self-contained HTML document for a local Markdown wiki, \
             following the `{skill}` skill. Emit ONLY a complete standalone HTML document: a single \
             `<!doctype html>` page with all CSS inlined in a `<style>` block. Do NOT load external \
             stylesheets, fonts, images, or scripts (inline SVG only). Do NOT modify any project \
             files. You may wrap the whole document in a fenced ```html block; everything else will \
             be discarded.\n",
        ));

        if let Some(purpose) = &purpose {
            prompt.push_str("\n--- Wiki purpose ---\n");
            prompt.push_str(purpose.trim());
            prompt.push('\n');
        }

        match export_type {
            ExportType::BeautifulRead | ExportType::KnowledgeCard => {
                let path = source_path.ok_or_else(|| {
                    BackendError::new(
                        "EXPORT_SOURCE_REQUIRED",
                        "This export type requires a source page path.",
                        true,
                        true,
                    )
                })?;
                let page = search_service.read_page(context, path)?;
                prompt.push_str(&format!(
                    "\n--- Source page: {} ---\ntitle: {}\ntype: {:?}\ntags: {}\n\n",
                    path,
                    page.meta.title,
                    page.meta.page_type,
                    page.meta.tags.join(", ")
                ));
                prompt.push_str(page.body_markdown.trim());
                prompt.push('\n');
                if export_type == ExportType::BeautifulRead {
                    prompt.push_str(
                        "\nProduce a long-form, readable article layout (serif body, generous \
                         measure, clear heading hierarchy, styled blockquotes and code).",
                    );
                } else {
                    prompt.push_str(
                        "\nProduce a compact knowledge card (title, type, tags, 3-6 key facts as \
                         bullets, a one-line source attribution).",
                    );
                }
            }
            ExportType::ConceptMap => {
                if let Some(path) = source_path {
                    let page = search_service.read_page(context, path)?;
                    prompt.push_str(&format!(
                        "\n--- Centre page: {} ---\ntitle: {}\nlinks: {}\n\n",
                        path,
                        page.meta.title,
                        page.meta.wikilinks.join(", ")
                    ));
                    prompt.push_str(
                        "Render an inline-SVG concept map centred on this page with its wikilinks \
                         as 1-hop neighbour nodes. Static (no JavaScript).",
                    );
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
        Ok(prompt)
    }

    fn append_page_summaries(
        &self,
        context: &ProjectContext,
        search_service: &SearchService,
        prompt: &mut String,
    ) -> Result<(), BackendError> {
        let tree = search_service.scan_wiki(context)?;
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
            if let Ok(content) = search_service.read_page(context, &page.path) {
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
        if !output_relative.starts_with("exports/html/") || output_relative.contains("..") {
            return Err(BackendError::new(
                "EXPORT_PATH_INVALID",
                "Exports may only be written under exports/html/.",
                true,
                true,
            ));
        }
        let resolved = context.resolve_project_path(output_relative)?;
        self.file_store.ensure_dir(context, EXPORTS_HTML_DIR)?;
        self.file_store.write_text_absolute(&resolved, html)
    }

    /// Append a record to `.app/exports.json` (created if missing).
    pub fn append_record(
        &self,
        context: &ProjectContext,
        record: ExportRecord,
    ) -> Result<(), BackendError> {
        let mut records = self.list_records(context)?;
        records.insert(0, record);
        self.file_store
            .write_json_atomic(context, EXPORTS_INDEX_PATH, &records)
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
            task_id,
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::paths::ProjectContext;
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
            )
            .unwrap();
        assert!(prompt.contains("html-beautiful-read"));
        assert!(prompt.contains("An agent acts."));
        assert!(prompt.contains("Explain agents."));
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
            )
            .unwrap();
        assert!(prompt.contains("html-project-report"));
        assert!(prompt.contains("wiki/concepts/agent.md"));
        assert!(prompt.contains("wiki/concepts/react.md"));
        // log.md summaries are skipped.
        assert!(!prompt.contains("path: wiki/log.md"));
        std::fs::remove_dir_all(root).unwrap();
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
