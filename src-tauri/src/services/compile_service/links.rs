use std::collections::{HashMap, HashSet};

use crate::errors::BackendError;
use crate::models::compile::CompileManifest;
use crate::models::paths::ProjectContext;
use crate::services::{FileStore, LintService};
use crate::utils::markdown_utils::{extract_wikilinks, parse_frontmatter, split_frontmatter};

fn link_error(path: &str, target: &str) -> BackendError {
    BackendError::new(
        "COMPILE_LINK_INVALID",
        format!("Wiki candidate would introduce a broken or unsafe link in {path}: {target}"),
        true,
        true,
    )
    .with_details(serde_json::json!({ "path": path, "target": target }))
}

fn decode_percent(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = bytes.get(index + 1..index + 3)?;
            let byte = u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?;
            decoded.push(byte);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn anchor_slug(value: &str) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for character in value.trim().chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() || character == '_' {
            if separator && !slug.is_empty() {
                slug.push('-');
            }
            separator = false;
            slug.push(character);
        } else if character.is_whitespace() || character == '-' {
            separator = true;
        }
    }
    slug
}

fn wikilink_anchors(body: &str) -> Vec<(String, String)> {
    let mut anchors = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("[[") {
        let after_open = &rest[start + 2..];
        let Some(end) = after_open.find("]]") else {
            break;
        };
        let link = after_open[..end].split('|').next().unwrap_or("");
        if let Some((target, anchor)) = link.split_once('#') {
            if !anchor.trim().is_empty() {
                anchors.push((target.trim().to_string(), anchor.trim().to_string()));
            }
        }
        rest = &after_open[end + 2..];
    }
    anchors
}

fn markdown_anchors(body: &str) -> Vec<(String, String)> {
    let mut anchors = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("](") {
        let after_open = &rest[start + 2..];
        let Some(end) = after_open.find(')') else {
            break;
        };
        let destination = after_open[..end].trim().trim_matches(['<', '>']);
        let destination = destination.split_whitespace().next().unwrap_or("");
        if let Some((target, anchor)) = destination.split_once('#') {
            if (target.is_empty() || target.to_ascii_lowercase().ends_with(".md"))
                && !anchor.trim().is_empty()
            {
                anchors.push((target.to_string(), anchor.to_string()));
            }
        }
        rest = &after_open[end + 1..];
    }
    anchors
}

fn page_has_anchor(body: &str, anchor: &str) -> bool {
    let wanted = anchor_slug(anchor);
    !wanted.is_empty()
        && body.lines().any(|line| {
            let heading = line.trim_start();
            let hashes = heading
                .chars()
                .take_while(|character| *character == '#')
                .count();
            (1..=6).contains(&hashes)
                && heading.chars().nth(hashes) == Some(' ')
                && anchor_slug(heading[hashes..].trim().trim_end_matches('#').trim()) == wanted
        })
}

fn resolve_wiki_page<'a>(
    current_path: &str,
    target: &str,
    paths: &'a HashSet<String>,
    aliases: &'a HashMap<String, Option<String>>,
) -> Option<&'a str> {
    let target = target.trim_end_matches(".md").to_lowercase();
    let folder = current_path
        .rsplit_once('/')
        .map(|(folder, _)| folder)
        .unwrap_or("wiki");
    let exact = [
        format!("{target}.md"),
        format!("wiki/{target}.md"),
        format!("{folder}/{target}.md"),
    ];
    for expected in exact {
        if let Some(found) = paths.iter().find(|path| path.to_lowercase() == expected) {
            return Some(found);
        }
    }
    let mut basename = paths.iter().filter(|path| {
        path.rsplit('/')
            .next()
            .unwrap_or(path)
            .trim_end_matches(".md")
            .to_lowercase()
            == target
    });
    if let Some(found) = basename.next() {
        if basename.next().is_none() {
            return Some(found);
        }
    }
    aliases.get(&target)?.as_deref()
}

fn page_aliases(
    context: &ProjectContext,
    paths: &HashSet<String>,
    candidate_contents: &HashMap<String, &str>,
) -> HashMap<String, Option<String>> {
    let mut aliases: HashMap<String, Option<String>> = HashMap::new();
    for path in paths {
        let content = candidate_contents
            .get(path)
            .map(|value| (*value).to_string())
            .or_else(|| FileStore.read_markdown(context, path).ok());
        let Some(content) = content else { continue };
        let frontmatter = split_frontmatter(&content).frontmatter;
        let Some(frontmatter) = frontmatter else {
            continue;
        };
        let parsed = parse_frontmatter(&frontmatter);
        let mut titles = parsed.get_list("aliases");
        if let Some(title) = parsed.get_scalar("title") {
            titles.push(title);
        }
        for title in titles {
            let key = title.trim().trim_end_matches(".md").to_lowercase();
            if key.is_empty() {
                continue;
            }
            aliases
                .entry(key)
                .and_modify(|existing| {
                    if existing.as_deref() != Some(path.as_str()) {
                        *existing = None;
                    }
                })
                .or_insert_with(|| Some(path.clone()));
        }
    }
    aliases
}

fn broken_anchor_links(
    context: &ProjectContext,
    path: &str,
    content: &str,
    markdown_paths: &HashSet<String>,
    candidate_contents: &HashMap<String, &str>,
    aliases: &HashMap<String, Option<String>>,
) -> Result<HashSet<String>, BackendError> {
    let mut broken = HashSet::new();
    for (target, raw_anchor) in wikilink_anchors(content) {
        let decoded_target = decode_percent(&target).ok_or_else(|| link_error(path, &target))?;
        let anchor = decode_percent(&raw_anchor).ok_or_else(|| link_error(path, &raw_anchor))?;
        let resolved = if decoded_target.is_empty() {
            Some(path)
        } else {
            resolve_wiki_page(path, &decoded_target, markdown_paths, aliases)
        };
        let Some(resolved) = resolved else { continue };
        let body = if resolved == path {
            Some(content.to_string())
        } else if let Some(candidate) = candidate_contents.get(resolved) {
            Some((*candidate).to_string())
        } else {
            Some(FileStore.read_markdown(context, resolved)?)
        };
        if !body
            .as_deref()
            .is_some_and(|body| page_has_anchor(body, &anchor))
        {
            broken.insert(format!("[[{target}#{raw_anchor}]]"));
        }
    }
    for (target, raw_anchor) in markdown_anchors(content) {
        let decoded_target = decode_percent(&target).ok_or_else(|| link_error(path, &target))?;
        let anchor = decode_percent(&raw_anchor).ok_or_else(|| link_error(path, &raw_anchor))?;
        let resolved = if decoded_target.is_empty() {
            Some(path.to_string())
        } else {
            LintService::candidate_resource_paths(path, &decoded_target)
                .into_iter()
                .find(|candidate| markdown_paths.contains(candidate))
        };
        let Some(resolved) = resolved else { continue };
        let body = if resolved == path {
            content.to_string()
        } else if let Some(candidate) = candidate_contents.get(&resolved) {
            (*candidate).to_string()
        } else {
            FileStore.read_markdown(context, &resolved)?
        };
        if !page_has_anchor(&body, &anchor) {
            broken.insert(format!("{target}#{raw_anchor}"));
        }
    }
    Ok(broken)
}

fn broken_links(
    context: &ProjectContext,
    path: &str,
    content: &str,
    markdown_paths: &HashSet<String>,
    deleted: &HashSet<String>,
    known_sources: &HashSet<String>,
    aliases: &HashMap<String, Option<String>>,
) -> Result<HashSet<String>, BackendError> {
    let mut broken = HashSet::new();
    for target in extract_wikilinks(content) {
        let Some(target) = decode_percent(target.trim()) else {
            broken.insert(format!("[[{target}]]"));
            continue;
        };
        if target.contains("://") || target.starts_with("mailto:") {
            continue;
        }
        if target.starts_with('/') || target.contains("..") || target.contains('\\') {
            broken.insert(format!("[[{target}]]"));
            continue;
        }
        let normalized = target.trim_end_matches(".md").to_lowercase();
        let page_folder = path
            .rsplit_once('/')
            .map(|(folder, _)| folder)
            .unwrap_or("wiki");
        let exists = markdown_paths.iter().any(|candidate| {
            let lower = candidate.to_lowercase();
            lower.trim_end_matches(".md") == normalized
                || lower
                    .strip_prefix("wiki/")
                    .unwrap_or(&lower)
                    .trim_end_matches(".md")
                    == normalized
                || lower.trim_end_matches(".md")
                    == format!("{page_folder}/{normalized}").to_lowercase()
                || candidate
                    .rsplit('/')
                    .next()
                    .unwrap_or(candidate)
                    .trim_end_matches(".md")
                    .to_lowercase()
                    == normalized
        }) || aliases.get(&normalized).is_some_and(Option::is_some)
            || known_sources.iter().any(|source| {
                let source = source.to_lowercase();
                source.trim_end_matches(".md") == normalized
                    || format!(
                        "sources/{}",
                        source
                            .rsplit('/')
                            .next()
                            .unwrap_or(&source)
                            .trim_end_matches(".md")
                    ) == normalized
            });
        if !exists {
            broken.insert(format!("[[{target}]]"));
        }
    }
    for raw in LintService::candidate_resource_refs(content) {
        let Some(target) = decode_percent(&raw) else {
            broken.insert(raw);
            continue;
        };
        if target.starts_with('/') || target.starts_with("//") || target.contains('\\') {
            broken.insert(target);
            continue;
        }
        let candidates = LintService::candidate_resource_paths(path, &target);
        if candidates.is_empty() {
            broken.insert(target);
            continue;
        }
        let exists = candidates.iter().any(|candidate| {
            markdown_paths.contains(candidate)
                || candidate.strip_prefix("wiki/sources/").is_some_and(|name| {
                    known_sources
                        .iter()
                        .any(|source| source.rsplit('/').next() == Some(name))
                })
                || (!deleted.contains(candidate)
                    && context
                        .resolve_project_path(candidate)
                        .is_ok_and(|absolute| absolute.is_file()))
        });
        if !exists {
            broken.insert(target);
        }
    }
    Ok(broken)
}

/// Validate the virtual post-apply Wiki. Existing unrelated broken links do
/// not make a local update impossible; newly broken links and deletion impacts
/// do. No external URL is fetched.
pub(super) fn validate_final_links(
    context: &ProjectContext,
    manifest: &CompileManifest,
    known_sources: &HashSet<String>,
) -> Result<(), BackendError> {
    let mut current = HashMap::new();
    for absolute in FileStore.list_markdown_files(&context.wiki_dir)? {
        let path = context.to_project_relative(&absolute)?;
        current.insert(path, absolute);
    }
    let deleted = manifest.deletions.iter().cloned().collect::<HashSet<_>>();
    let mut final_paths = current.keys().cloned().collect::<HashSet<_>>();
    final_paths.extend(manifest.files.iter().map(|file| file.path.clone()));
    final_paths.retain(|path| !deleted.contains(path));
    let original_paths = current.keys().cloned().collect::<HashSet<_>>();
    let candidate_contents = manifest
        .files
        .iter()
        .map(|file| (file.path.clone(), file.content.as_str()))
        .collect::<HashMap<_, _>>();
    let old_aliases = page_aliases(context, &original_paths, &HashMap::new());
    let final_aliases = page_aliases(context, &final_paths, &candidate_contents);

    for file in &manifest.files {
        let before = current
            .get(&file.path)
            .map(|_| FileStore.read_markdown(context, &file.path))
            .transpose()?;
        let old_broken = before
            .as_deref()
            .map(|body| {
                broken_links(
                    context,
                    &file.path,
                    body,
                    &original_paths,
                    &HashSet::new(),
                    known_sources,
                    &old_aliases,
                )
            })
            .transpose()?
            .unwrap_or_default();
        let new_broken = broken_links(
            context,
            &file.path,
            &file.content,
            &final_paths,
            &deleted,
            known_sources,
            &final_aliases,
        )?;
        if let Some(new) = new_broken.difference(&old_broken).next() {
            return Err(link_error(&file.path, new));
        }
        let old_anchors = before
            .as_deref()
            .map(|body| {
                broken_anchor_links(
                    context,
                    &file.path,
                    body,
                    &original_paths,
                    &HashMap::new(),
                    &old_aliases,
                )
            })
            .transpose()?
            .unwrap_or_default();
        let new_anchors = broken_anchor_links(
            context,
            &file.path,
            &file.content,
            &final_paths,
            &candidate_contents,
            &final_aliases,
        )?;
        if let Some(new) = new_anchors.difference(&old_anchors).next() {
            return Err(link_error(&file.path, new));
        }
    }

    if !deleted.is_empty() || old_aliases != final_aliases {
        let source_root = context
            .layout
            .source_write_root
            .as_deref()
            .unwrap_or("wiki/sources");
        let modified = manifest
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<HashSet<_>>();
        for path in current.keys().filter(|path| {
            !deleted.contains(*path)
                && !modified.contains(path.as_str())
                && !path.starts_with(&format!("{source_root}/"))
        }) {
            let body = FileStore.read_markdown(context, path)?;
            let old_broken = broken_links(
                context,
                path,
                &body,
                &original_paths,
                &HashSet::new(),
                known_sources,
                &old_aliases,
            )?;
            let new_broken = broken_links(
                context,
                path,
                &body,
                &final_paths,
                &deleted,
                known_sources,
                &final_aliases,
            )?;
            if let Some(new) = new_broken.difference(&old_broken).next() {
                return Err(link_error(path, new));
            }
            let old_anchors = broken_anchor_links(
                context,
                path,
                &body,
                &original_paths,
                &HashMap::new(),
                &old_aliases,
            )?;
            let new_anchors = broken_anchor_links(
                context,
                path,
                &body,
                &final_paths,
                &candidate_contents,
                &final_aliases,
            )?;
            if let Some(new) = new_anchors.difference(&old_anchors).next() {
                return Err(link_error(path, new));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::compile::CompileFile;

    fn fixture() -> (tempfile::TempDir, ProjectContext) {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("wiki/concepts")).unwrap();
        std::fs::create_dir_all(root.path().join("wiki/sources")).unwrap();
        std::fs::write(root.path().join("wiki/sources/资料.md"), "# source\n").unwrap();
        let context = ProjectContext::new("links", root.path().to_path_buf());
        (root, context)
    }

    fn manifest(path: &str, content: &str) -> CompileManifest {
        CompileManifest {
            files: vec![CompileFile::new(path, content)],
            deletions: Vec::new(),
            summary: "test".into(),
        }
    }

    #[test]
    fn rejects_new_wikilinks_and_sources_line_targets() {
        let (_root, context) = fixture();
        let path = "wiki/concepts/page.md";
        assert_eq!(
            validate_final_links(&context, &manifest(path, "[[missing]]\n"), &HashSet::new())
                .unwrap_err()
                .code,
            "COMPILE_LINK_INVALID"
        );
        assert_eq!(
            validate_final_links(
                &context,
                &manifest(path, "> Sources: [missing](../sources/missing.md)\n"),
                &HashSet::new(),
            )
            .unwrap_err()
            .code,
            "COMPILE_LINK_INVALID"
        );
        validate_final_links(
            &context,
            &manifest(
                path,
                "> Sources: [资料](../sources/%E8%B5%84%E6%96%99.md)\n",
            ),
            &HashSet::new(),
        )
        .unwrap();
    }

    #[test]
    fn deletion_detects_new_inbound_breakage_but_ignores_old_unrelated_damage() {
        let (root, context) = fixture();
        std::fs::write(root.path().join("wiki/concepts/old.md"), "# old\n").unwrap();
        std::fs::write(
            root.path().join("wiki/concepts/reader.md"),
            "[[old]]\n[[already-missing]]\n",
        )
        .unwrap();
        let candidate = CompileManifest {
            files: Vec::new(),
            deletions: vec!["wiki/concepts/old.md".into()],
            summary: "delete".into(),
        };
        assert_eq!(
            validate_final_links(&context, &candidate, &HashSet::new())
                .unwrap_err()
                .code,
            "COMPILE_LINK_INVALID"
        );
        let harmless = manifest("wiki/concepts/new.md", "# new\n");
        validate_final_links(&context, &harmless, &HashSet::new()).unwrap();
    }

    #[test]
    fn verifies_new_wikilink_anchors_against_virtual_candidate_pages() {
        let (_root, context) = fixture();
        let good = CompileManifest {
            files: vec![
                CompileFile::new("wiki/concepts/中文页.md", "# 代理 记忆\n"),
                CompileFile::new(
                    "wiki/concepts/reader.md",
                    "See [[中文页#代理-记忆|说明]].\n",
                ),
            ],
            deletions: vec![],
            summary: "anchors".into(),
        };
        validate_final_links(&context, &good, &HashSet::new()).unwrap();
        let mut bad = good;
        bad.files[1].content = "See [[中文页#不存在]].\n".into();
        assert_eq!(
            validate_final_links(&context, &bad, &HashSet::new())
                .unwrap_err()
                .code,
            "COMPILE_LINK_INVALID"
        );
    }

    #[test]
    fn frontmatter_title_alias_resolves_to_candidate_page() {
        let (_root, context) = fixture();
        let candidate = CompileManifest {
            files: vec![
                CompileFile::new(
                    "wiki/concepts/internal.md",
                    "---\ntitle: 对外标题\naliases: [另一标题]\n---\n# 小节\n",
                ),
                CompileFile::new("wiki/concepts/reader.md", "[[另一标题#小节]]\n"),
            ],
            deletions: vec![],
            summary: "aliases".into(),
        };
        validate_final_links(&context, &candidate, &HashSet::new()).unwrap();
    }

    #[test]
    fn relative_markdown_anchor_must_exist() {
        let (_root, context) = fixture();
        let mut candidate = CompileManifest {
            files: vec![
                CompileFile::new("wiki/concepts/target.md", "# 正确章节\n"),
                CompileFile::new("wiki/concepts/reader.md", "[see](target.md#正确章节)\n"),
            ],
            deletions: vec![],
            summary: "markdown anchor".into(),
        };
        validate_final_links(&context, &candidate, &HashSet::new()).unwrap();
        candidate.files[1].content = "[see](target.md#不存在)\n".into();
        assert_eq!(
            validate_final_links(&context, &candidate, &HashSet::new())
                .unwrap_err()
                .code,
            "COMPILE_LINK_INVALID"
        );
    }

    #[test]
    fn removing_alias_breaks_existing_inbound_link() {
        let (root, context) = fixture();
        std::fs::write(
            root.path().join("wiki/concepts/target.md"),
            "---\naliases: [旧称]\n---\n# target\n",
        )
        .unwrap();
        std::fs::write(root.path().join("wiki/concepts/reader.md"), "[[旧称]]\n").unwrap();
        let candidate = manifest("wiki/concepts/target.md", "# target\n");
        assert_eq!(
            validate_final_links(&context, &candidate, &HashSet::new())
                .unwrap_err()
                .code,
            "COMPILE_LINK_INVALID"
        );
    }
}
