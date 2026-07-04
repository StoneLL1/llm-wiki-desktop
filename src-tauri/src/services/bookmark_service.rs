use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::errors::BackendError;
use crate::models::bookmark::{
    BookmarkEntry, BookmarkFile, BookmarkResourceKind, ExportBookmarkResponse,
    BOOKMARK_FILE_VERSION,
};
use crate::models::export::{ExportRecord, ExportStatus};
use crate::models::paths::ProjectContext;
use crate::models::wiki::ToggleBookmarkResponse;
use crate::services::FileStore;
use crate::utils::path_utils::normalize_project_path;
use crate::utils::time_utils::now_rfc3339;

const BOOKMARKS_PATH: &str = ".app/bookmarks.json";

#[derive(Default)]
pub struct BookmarkService {
    file_store: FileStore,
}

impl BookmarkService {
    pub fn read_file(&self, context: &ProjectContext) -> Result<BookmarkFile, BackendError> {
        let path = context.app_dir.join("bookmarks.json");
        if !path.exists() {
            return Ok(BookmarkFile::default());
        }

        let raw = fs::read_to_string(&path).map_err(|err| {
            BackendError::new("BOOKMARK_READ_FAILED", err.to_string(), true, false)
                .with_details(serde_json::json!({ "path": BOOKMARKS_PATH }))
        })?;
        parse_bookmark_file(&raw)
    }

    pub fn list_entries(
        &self,
        context: &ProjectContext,
    ) -> Result<Vec<crate::models::bookmark::BookmarkEntry>, BackendError> {
        Ok(self.read_file(context)?.entries)
    }

    pub fn wiki_page_paths(
        &self,
        context: &ProjectContext,
    ) -> Result<HashSet<String>, BackendError> {
        Ok(self
            .read_file(context)?
            .entries
            .into_iter()
            .filter(|entry| entry.kind == BookmarkResourceKind::WikiPage)
            .map(|entry| entry.path)
            .collect())
    }

    pub fn export_record_ids(
        &self,
        context: &ProjectContext,
    ) -> Result<HashSet<String>, BackendError> {
        Ok(self
            .read_file(context)?
            .entries
            .into_iter()
            .filter(|entry| entry.kind == BookmarkResourceKind::ExportHtml)
            .filter_map(|entry| entry.export_record_id)
            .collect())
    }

    pub fn toggle_wiki_page(
        &self,
        context: &ProjectContext,
        relative_path: &str,
        title: &str,
    ) -> Result<ToggleBookmarkResponse, BackendError> {
        let relative_path = validate_wiki_page(context, relative_path)?;
        let mut file = self.read_file(context)?;
        let id = wiki_entry_id(&relative_path);
        let bookmarked = toggle_entry(
            &mut file,
            &id,
            BookmarkEntry {
                id: id.clone(),
                kind: BookmarkResourceKind::WikiPage,
                path: relative_path.clone(),
                title: title.to_string(),
                export_record_id: None,
                created_at: now_rfc3339(),
            },
        );
        self.persist(context, &file)?;
        Ok(ToggleBookmarkResponse {
            relative_path,
            bookmarked,
        })
    }

    pub fn toggle_export_html(
        &self,
        context: &ProjectContext,
        record: &ExportRecord,
    ) -> Result<ExportBookmarkResponse, BackendError> {
        if record.status == ExportStatus::Failed {
            return Err(BackendError::new(
                "EXPORT_BOOKMARK_UNAVAILABLE",
                "Failed exports cannot be bookmarked.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "exportRecordId": record.id })));
        }
        let output_path = validate_export_html_path(context, &record.output_path)?;
        let mut file = self.read_file(context)?;
        let id = export_entry_id(&record.id);
        let bookmarked = toggle_entry(
            &mut file,
            &id,
            BookmarkEntry {
                id: id.clone(),
                kind: BookmarkResourceKind::ExportHtml,
                path: output_path,
                title: record.title.clone(),
                export_record_id: Some(record.id.clone()),
                created_at: now_rfc3339(),
            },
        );
        self.persist(context, &file)?;
        Ok(ExportBookmarkResponse {
            export_record_id: record.id.clone(),
            bookmarked,
        })
    }

    fn persist(&self, context: &ProjectContext, file: &BookmarkFile) -> Result<(), BackendError> {
        self.file_store
            .write_json_atomic(context, BOOKMARKS_PATH, file)
    }
}

fn parse_bookmark_file(raw: &str) -> Result<BookmarkFile, BackendError> {
    let value: serde_json::Value = serde_json::from_str(raw).map_err(bookmark_parse_error)?;
    let parsed = match value {
        serde_json::Value::Array(items) => legacy_entries_from_array(&items),
        serde_json::Value::Object(map) if map.contains_key("entries") => {
            let file: BookmarkFile = serde_json::from_value(serde_json::Value::Object(map))
                .map_err(bookmark_parse_error)?;
            normalize_bookmark_file(file)
        }
        serde_json::Value::Object(map) => legacy_entries_from_object(&map),
        _ => BookmarkFile::default(),
    };
    Ok(parsed)
}

fn normalize_bookmark_file(file: BookmarkFile) -> BookmarkFile {
    let mut seen = HashSet::new();
    let entries = file
        .entries
        .into_iter()
        .map(normalize_entry)
        .filter(|entry| seen.insert(entry.id.clone()))
        .collect();
    BookmarkFile {
        version: BOOKMARK_FILE_VERSION,
        entries,
    }
}

fn normalize_entry(mut entry: BookmarkEntry) -> BookmarkEntry {
    entry.path = normalize_project_path(&entry.path);
    if entry.id.trim().is_empty() {
        entry.id = match entry.kind {
            BookmarkResourceKind::WikiPage => wiki_entry_id(&entry.path),
            BookmarkResourceKind::ExportHtml => entry
                .export_record_id
                .as_deref()
                .map(export_entry_id)
                .unwrap_or_else(|| format!("export_html:{}", entry.path)),
        };
    }
    entry
}

fn legacy_entries_from_object(map: &serde_json::Map<String, serde_json::Value>) -> BookmarkFile {
    for key in ["pages", "wikiPages", "bookmarks"] {
        if let Some(serde_json::Value::Array(items)) = map.get(key) {
            return legacy_entries_from_array(items);
        }
    }
    BookmarkFile::default()
}

fn legacy_entries_from_array(items: &[serde_json::Value]) -> BookmarkFile {
    let created_at = now_rfc3339();
    let mut seen = HashSet::new();
    let entries = items
        .iter()
        .filter_map(|item| legacy_path_and_title(item))
        .map(|(path, title)| {
            let path = normalize_project_path(&path);
            BookmarkEntry {
                id: wiki_entry_id(&path),
                kind: BookmarkResourceKind::WikiPage,
                path,
                title,
                export_record_id: None,
                created_at: created_at.clone(),
            }
        })
        .filter(|entry| seen.insert(entry.id.clone()))
        .collect();
    BookmarkFile {
        version: BOOKMARK_FILE_VERSION,
        entries,
    }
}

fn legacy_path_and_title(item: &serde_json::Value) -> Option<(String, String)> {
    match item {
        serde_json::Value::String(path) => Some((path.clone(), title_from_path(path))),
        serde_json::Value::Object(map) => {
            let path = map.get("path")?.as_str()?.to_string();
            let title = map
                .get("title")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| title_from_path(&path));
            Some((path, title))
        }
        _ => None,
    }
}

fn validate_wiki_page(
    context: &ProjectContext,
    relative_path: &str,
) -> Result<String, BackendError> {
    let normalized = normalize_project_path(relative_path);
    if !normalized.starts_with("wiki/") || !normalized.ends_with(".md") {
        return Err(BackendError::new(
            "PATH_OUTSIDE_WIKI",
            "Wiki bookmarks must reference a markdown page under wiki/.",
            false,
            true,
        )
        .with_details(serde_json::json!({ "path": normalized })));
    }

    let absolute = context.resolve_project_path(&normalized)?;
    if !absolute.exists() || !absolute.is_file() {
        return Err(BackendError::new(
            "WIKI_PAGE_NOT_FOUND",
            "Wiki page does not exist.",
            true,
            true,
        )
        .with_details(serde_json::json!({ "path": normalized })));
    }

    Ok(normalized)
}

fn validate_export_html_path(
    context: &ProjectContext,
    output_path: &str,
) -> Result<String, BackendError> {
    let normalized = normalize_project_path(output_path);
    if !normalized.starts_with("exports/html/") || !normalized.ends_with(".html") {
        return Err(BackendError::new(
            "PATH_OUTSIDE_EXPORTS_HTML",
            "Export bookmarks must reference HTML output under exports/html/.",
            false,
            true,
        )
        .with_details(serde_json::json!({ "path": normalized })));
    }

    let _ = context.resolve_project_path(&normalized)?;
    Ok(normalized)
}

fn toggle_entry(file: &mut BookmarkFile, id: &str, entry: BookmarkEntry) -> bool {
    file.version = BOOKMARK_FILE_VERSION;
    if let Some(index) = file.entries.iter().position(|existing| existing.id == id) {
        file.entries.remove(index);
        false
    } else {
        file.entries.push(entry);
        true
    }
}

fn title_from_path(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn wiki_entry_id(path: &str) -> String {
    format!("wiki_page:{path}")
}

fn export_entry_id(export_record_id: &str) -> String {
    format!("export_html:{export_record_id}")
}

fn bookmark_parse_error(err: serde_json::Error) -> BackendError {
    BackendError::new("BOOKMARK_PARSE_FAILED", err.to_string(), true, false)
        .with_details(serde_json::json!({ "path": BOOKMARKS_PATH }))
}

#[cfg(test)]
mod tests {
    use super::BookmarkService;
    use crate::models::bookmark::{BookmarkResourceKind, BOOKMARK_FILE_VERSION};
    use crate::models::export::{ExportRecord, ExportRoute, ExportStatus, ExportType};
    use crate::models::paths::ProjectContext;
    use std::path::PathBuf;

    fn tmp_context(suffix: &str) -> (ProjectContext, PathBuf) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("llm-wiki-bookmarks-{stamp}-{suffix}"));
        std::fs::create_dir_all(root.join(".app")).unwrap();
        std::fs::create_dir_all(root.join("wiki/concepts")).unwrap();
        std::fs::create_dir_all(root.join("exports/html")).unwrap();
        (ProjectContext::new("project-1", root.clone()), root)
    }

    fn write_wiki_page(root: &std::path::Path, relative_path: &str, contents: &str) {
        let path = root.join(relative_path.replace('/', std::path::MAIN_SEPARATOR_STR));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    fn export_record(id: &str, output_path: &str, status: ExportStatus) -> ExportRecord {
        ExportRecord {
            id: id.to_string(),
            export_type: ExportType::BeautifulRead,
            title: "Agent".into(),
            source_path: Some("wiki/concepts/agent.md".into()),
            output_path: output_path.to_string(),
            created_at: "2026-07-04T00:00:00Z".into(),
            route: ExportRoute::Byok,
            status,
            bookmarked: false,
            task_id: None,
        }
    }

    #[test]
    fn missing_bookmark_file_defaults_empty_v2() {
        let (context, root) = tmp_context("missing");
        let service = BookmarkService::default();

        let file = service.read_file(&context).unwrap();

        assert_eq!(file.version, BOOKMARK_FILE_VERSION);
        assert!(file.entries.is_empty());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_string_array_converts_to_wiki_entries() {
        let (context, root) = tmp_context("legacy-array");
        std::fs::write(
            context.app_dir.join("bookmarks.json"),
            r#"["wiki/concepts/agent.md", "wiki/index.md"]"#,
        )
        .unwrap();
        let service = BookmarkService::default();

        let file = service.read_file(&context).unwrap();
        let paths = service.wiki_page_paths(&context).unwrap();

        assert_eq!(file.version, BOOKMARK_FILE_VERSION);
        assert_eq!(file.entries.len(), 2);
        assert_eq!(file.entries[0].kind, BookmarkResourceKind::WikiPage);
        assert_eq!(file.entries[0].id, "wiki_page:wiki/concepts/agent.md");
        assert_eq!(file.entries[0].path, "wiki/concepts/agent.md");
        assert_eq!(file.entries[0].title, "agent");
        assert!(paths.contains("wiki/concepts/agent.md"));
        assert!(paths.contains("wiki/index.md"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn toggles_wiki_without_touching_markdown_and_persists_v2() {
        let (context, root) = tmp_context("wiki-toggle");
        let original = "---\ntitle: Agent\nstarred: true\n---\n# Agent\n";
        write_wiki_page(&root, "wiki/concepts/agent.md", original);
        let service = BookmarkService::default();

        let added = service
            .toggle_wiki_page(&context, "wiki/concepts/agent.md", "Agent")
            .unwrap();
        let stored = service.read_file(&context).unwrap();

        assert!(added.bookmarked);
        assert_eq!(added.relative_path, "wiki/concepts/agent.md");
        assert_eq!(
            std::fs::read_to_string(root.join("wiki/concepts/agent.md")).unwrap(),
            original
        );
        assert_eq!(stored.version, BOOKMARK_FILE_VERSION);
        assert_eq!(stored.entries.len(), 1);
        assert_eq!(stored.entries[0].kind, BookmarkResourceKind::WikiPage);

        let removed = service
            .toggle_wiki_page(&context, "wiki/concepts/agent.md", "Agent")
            .unwrap();
        assert!(!removed.bookmarked);
        assert!(service.read_file(&context).unwrap().entries.is_empty());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn corrupt_json_returns_recoverable_parse_error() {
        let (context, root) = tmp_context("corrupt");
        std::fs::write(context.app_dir.join("bookmarks.json"), "{not json").unwrap();
        let service = BookmarkService::default();

        let err = service
            .read_file(&context)
            .expect_err("corrupt bookmarks must return a parse error");

        assert_eq!(err.code, "BOOKMARK_PARSE_FAILED");
        assert!(err.recoverable);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_paths_outside_wiki_or_exports_html() {
        let (context, root) = tmp_context("bounds");
        write_wiki_page(&root, "raw/sources/source.md", "# Source");
        let service = BookmarkService::default();

        let wiki_err = service
            .toggle_wiki_page(&context, "raw/sources/source.md", "Source")
            .expect_err("wiki bookmark must stay under wiki");
        let export_err = service
            .toggle_export_html(
                &context,
                &export_record(
                    "export-1",
                    "exports/other/report.html",
                    ExportStatus::Succeeded,
                ),
            )
            .expect_err("export bookmark must stay under exports/html");

        assert_eq!(wiki_err.code, "PATH_OUTSIDE_WIKI");
        assert_eq!(export_err.code, "PATH_OUTSIDE_EXPORTS_HTML");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn toggles_export_without_requiring_missing_output() {
        let (context, root) = tmp_context("export-toggle");
        let service = BookmarkService::default();

        let added = service
            .toggle_export_html(
                &context,
                &export_record(
                    "export-1",
                    "exports/html/missing.html",
                    ExportStatus::Succeeded,
                ),
            )
            .unwrap();
        let ids = service.export_record_ids(&context).unwrap();
        let file = service.read_file(&context).unwrap();

        assert!(added.bookmarked);
        assert_eq!(added.export_record_id, "export-1");
        assert!(ids.contains("export-1"));
        assert_eq!(file.entries[0].kind, BookmarkResourceKind::ExportHtml);
        assert_eq!(
            file.entries[0].export_record_id.as_deref(),
            Some("export-1")
        );

        std::fs::remove_dir_all(root).unwrap();
    }
}
