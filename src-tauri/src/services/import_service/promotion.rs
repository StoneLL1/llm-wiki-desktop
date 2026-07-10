use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use crate::errors::BackendError;
use crate::models::import::{ConflictResolution, ImportPreview};
use crate::models::paths::ProjectContext;
use crate::services::file_store::FileStore;

use super::source_actions::remove_project_files;

impl super::ImportService {
    /// Promote each confirmed source's browsable text into
    /// `wiki/sources/<name>.md`, written verbatim with a `type: source`
    /// frontmatter header so the original is readable immediately without
    /// compiling. The text comes from either:
    ///   - the staged extracted Markdown at `raw/extracted/…` (PDF/DOCX/etc), or
    ///   - the archived Markdown original at `raw/sources/…` (plain `.md`
    ///     imports, which produce no extracted text because the file already IS
    ///     Markdown).
    ///
    /// Returns `(by_archive, promoted_paths, staged_to_delete)`:
    ///   - `by_archive` maps each entry's `archived_path` to its promoted
    ///     `wiki/sources/…` path (used to rewrite `extracted_text_path` before
    ///     indexing so `.app/source-index.json` records the browsable page);
    ///   - `promoted_paths` is the list of promoted paths (for rollback);
    ///   - `staged_to_delete` is the set of transient `raw/extracted/…` files
    ///     to remove once indexing succeeds (the archived original is never
    ///     deleted).
    ///
    /// Entries resolved Skip/LinkToExisting, with empty text, or with no
    /// usable text source are skipped. On error, any files promoted so far in
    /// this call are removed before returning.
    pub(super) fn promote_extracted_to_sources(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        preview: &ImportPreview,
    ) -> Result<(HashMap<String, String>, Vec<String>, Vec<String>), BackendError> {
        let sources_dir = context.wiki_dir.join("sources");
        let mut used_names: HashSet<String> = file_store
            .list_markdown_files(&sources_dir)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
            })
            .collect();

        file_store.ensure_dir(context, "wiki/sources")?;

        let mut by_archive: HashMap<String, String> = HashMap::new();
        let mut promoted: Vec<String> = Vec::new();
        let mut staged_to_delete: Vec<String> = Vec::new();
        for entry in &preview.files {
            if matches!(
                entry
                    .conflict
                    .as_ref()
                    .and_then(|conflict| conflict.resolution.as_ref()),
                Some(ConflictResolution::Skip | ConflictResolution::LinkToExisting)
            ) {
                continue;
            }

            // Resolve the verbatim text and whether it lives in transient
            // staging (extracted) or is the immutable archived original (.md).
            let archived = entry.archived_path.replace('\\', "/");
            let (text, staged_path): (String, Option<String>) =
                if let Some(staged_raw) = entry.extracted_text_path.as_ref() {
                    let staged = staged_raw.replace('\\', "/");
                    if !staged.starts_with("raw/extracted/") {
                        continue;
                    }
                    let staged_abs = match context.resolve_project_path(&staged) {
                        Ok(path) => path,
                        Err(_) => continue,
                    };
                    match fs::read_to_string(&staged_abs) {
                        Ok(text) => (text, Some(staged)),
                        Err(_) => continue,
                    }
                } else if archived.starts_with("raw/sources/") && archived.ends_with(".md") {
                    let archived_abs = match context.resolve_project_path(&archived) {
                        Ok(path) => path,
                        Err(_) => continue,
                    };
                    match fs::read_to_string(&archived_abs) {
                        Ok(text) => (text, None),
                        Err(_) => continue,
                    }
                } else {
                    continue;
                };

            if text.trim().is_empty() {
                continue;
            }

            let base = source_page_filename(&entry.original_name);
            let name = resolve_source_collision(&base, &used_names);
            used_names.insert(name.clone());
            let relative = format!("wiki/sources/{}", name);
            let content = build_source_page(&entry.original_name, &text);

            if let Err(error) = file_store.write_markdown(context, &relative, &content) {
                remove_project_files(context, &promoted);
                return Err(error);
            }
            promoted.push(relative.clone());
            by_archive.insert(archived.clone(), relative);
            if let Some(staged) = staged_path {
                staged_to_delete.push(staged);
            }
        }

        Ok((by_archive, promoted, staged_to_delete))
    }
}

fn build_source_page(original_name: &str, text: &str) -> String {
    let sources_value = yaml_scalar(original_name);
    let title_value = yaml_scalar(&source_display_title(original_name));
    match split_frontmatter(text) {
        Some((frontmatter, body)) => {
            let filtered: String = frontmatter
                .lines()
                .filter(|line| {
                    let trimmed = line.trim_start();
                    !(trimmed.starts_with("type:")
                        || trimmed.starts_with("sources:")
                        || trimmed.starts_with("title:"))
                })
                .map(|line| format!("{}\n", line))
                .collect();
            format!(
                "---\ntype: source\nsources: {}\ntitle: {}\n{}---\n\n{}",
                sources_value, title_value, filtered, body
            )
        }
        None => format!(
            "---\ntype: source\nsources: {}\ntitle: {}\n---\n\n{}",
            sources_value, title_value, text
        ),
    }
}

fn source_display_title(original_name: &str) -> String {
    Path::new(original_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Source")
        .to_string()
}

fn source_page_filename(original_name: &str) -> String {
    let stem = Path::new(original_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("source");
    format!("{}.md", sanitize_source_name(stem))
}

/// CJK-safe, traversal-safe name: keep alphanumerics (Unicode-aware), `-`,
/// and `_`; replace the rest with `-`; collapse runs and trim edges; cap to
/// 60 chars. Never produces an empty string.
fn sanitize_source_name(stem: &str) -> String {
    let mut out: String = stem
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    let trimmed = out.trim_matches('-');
    let mut out = if trimmed.is_empty() {
        "source".to_string()
    } else {
        trimmed.to_string()
    };
    const CAP: usize = 60;
    if out.chars().count() > CAP {
        let capped: String = out.chars().take(CAP).collect();
        let trimmed = capped.trim_matches('-');
        out = if trimmed.is_empty() {
            "source".to_string()
        } else {
            trimmed.to_string()
        };
    }
    out
}

fn resolve_source_collision(base: &str, used: &HashSet<String>) -> String {
    if !used.contains(base) {
        return base.to_string();
    }
    let stem = base.trim_end_matches(".md");
    let mut index = 2;
    loop {
        let candidate = format!("{}-{}.md", stem, index);
        if !used.contains(&candidate) {
            return candidate;
        }
        index += 1;
    }
}

/// Quote `value` as a YAML scalar. JSON string encoding is a valid YAML
/// double-quoted scalar, so this is safe for titles/filenames containing
/// spaces, CJK, quotes, or punctuation.
fn yaml_scalar(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

/// Split a leading YAML frontmatter block. Returns `(frontmatter_inner, body)`
/// where `frontmatter_inner` is the text between the opening and closing
/// `---` fences (fences excluded) and `body` is everything after the closing
/// fence. Returns `None` when the text does not begin with a closed
/// frontmatter block.
fn split_frontmatter(text: &str) -> Option<(String, String)> {
    let first_newline = text.find('\n').unwrap_or(text.len());
    if text[..first_newline].trim_end_matches(['\r', '\n']) != "---" {
        return None;
    }
    let mut frontmatter = String::new();
    let mut body = String::new();
    let mut closed = false;
    let after_first = if first_newline < text.len() {
        &text[first_newline + 1..]
    } else {
        ""
    };
    for line in after_first.split_inclusive('\n') {
        if !closed {
            let fence = line.trim_end_matches(['\r', '\n']);
            if fence == "---" || fence == "..." {
                closed = true;
                continue;
            }
            frontmatter.push_str(line);
        } else {
            body.push_str(line);
        }
    }
    if !closed {
        return None;
    }
    // Guard against a `---` that is a Markdown horizontal rule, not a
    // frontmatter block: only accept the block as frontmatter if it looks like
    // YAML (the first non-blank inner line is a `key:` entry, list item, or
    // comment). Otherwise treat the whole text as body so the verbatim
    // original is preserved instead of being split into invalid YAML.
    let looks_like_yaml = frontmatter
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| {
            let trimmed = line.trim_start();
            trimmed.contains(':') || trimmed.starts_with('-') || trimmed.starts_with('#')
        })
        .unwrap_or(true); // an empty block (`---\n---`) is valid YAML.
    if !looks_like_yaml {
        return None;
    }
    Some((frontmatter, body))
}

/// Clone `preview`, setting each promoted entry's `extracted_text_path` to its
/// promoted `wiki/sources/` path (keyed by `archived_path`). Used so
/// `record_confirmed_sources` indexes the browsable page — including plain
/// `.md` imports whose `extracted_text_path` was previously `None`.
pub(super) fn remap_extracted_paths(
    preview: &ImportPreview,
    by_archive: &HashMap<String, String>,
) -> ImportPreview {
    if by_archive.is_empty() {
        return preview.clone();
    }
    let mut clone = preview.clone();
    for entry in &mut clone.files {
        let archived = entry.archived_path.replace('\\', "/");
        if let Some(promoted) = by_archive.get(&archived) {
            entry.extracted_text_path = Some(promoted.clone());
        }
    }
    clone
}
