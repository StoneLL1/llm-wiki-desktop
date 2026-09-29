//! Pure Markdown helpers for the wiki reader/editor/search pipeline.
//!
//! These intentionally avoid pulling in a YAML crate or a Markdown AST: the
//! real sample vault uses a small, stable subset of frontmatter (scalar keys,
//! inline `[a, b]` lists, and block `- item` lists), and a hand-rolled parser
//! is enough to surface titles, tags, sources, aliases, and outgoing
//! `[[wikilink]]` targets. The models layer maps these raw strings onto typed
//! DTOs; nothing here depends on `crate::models` to keep the dependency arrow
//! one-way.

use std::collections::HashMap;

/// Result of splitting a file into its optional YAML frontmatter and body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontmatterSplit {
    pub frontmatter: Option<String>,
    pub body: String,
}

/// Split off a leading `---\n...\n---` frontmatter block. The returned body has
/// the frontmatter (and its fences) removed; `frontmatter` holds the inner YAML
/// text without fences. A missing or malformed block yields `None` + full body.
pub fn split_frontmatter(content: &str) -> FrontmatterSplit {
    let stripped = content.strip_prefix('\u{FEFF}').unwrap_or(content);
    if !stripped.starts_with("---") {
        return FrontmatterSplit {
            frontmatter: None,
            body: stripped.to_string(),
        };
    }

    // Skip the opening fence line.
    let after_opening = &stripped[3..];
    let rest = after_opening
        .strip_prefix('\n')
        .or_else(|| after_opening.strip_prefix("\r\n"))
        .unwrap_or(after_opening);

    // Find the closing fence on its own line.
    let mut closing: Option<usize> = None;
    let mut offset: usize = 0;
    for line in rest.split_inclusive('\n') {
        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
        if trimmed == "---" || trimmed == "..." {
            closing = Some(offset);
            break;
        }
        // No newline at end of buffer on the last line.
        if !line.ends_with('\n') && (trimmed == "---" || trimmed == "...") {
            closing = Some(offset);
            break;
        }
        offset += line.len();
    }

    let Some(end) = closing else {
        return FrontmatterSplit {
            frontmatter: None,
            body: stripped.to_string(),
        };
    };

    let fm = rest[..end].trim_end().to_string();
    // Skip past the closing fence line and any blank lines that follow.
    let mut after = &rest[end..];
    after = after.strip_prefix("---").unwrap_or(after);
    // Strip the newline that ends the closing-fence line, plus any additional
    // blank lines between the fence and the body content.
    while let Some(stripped) = after
        .strip_prefix('\n')
        .or_else(|| after.strip_prefix("\r\n"))
    {
        after = stripped;
    }

    FrontmatterSplit {
        frontmatter: Some(fm),
        body: after.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrontmatterValue {
    Scalar(String),
    List(Vec<String>),
}

impl FrontmatterValue {
    pub fn as_scalar(&self) -> Option<&str> {
        match self {
            FrontmatterValue::Scalar(s) => Some(s.as_str()),
            FrontmatterValue::List(_) => None,
        }
    }

    pub fn as_list(&self) -> Vec<String> {
        match self {
            FrontmatterValue::Scalar(s) => vec![s.clone()],
            FrontmatterValue::List(items) => items.clone(),
        }
    }
}

/// A minimal ordered key/value frontmatter representation. Insertion order is
/// preserved so callers can serialize it back deterministically, but lookups
/// are case-insensitive on keys (Obsidian/LDJ wikis mix `Title` and `title`).
#[derive(Debug, Clone, Default)]
pub struct Frontmatter {
    pub entries: Vec<(String, FrontmatterValue)>,
}

impl Frontmatter {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn normalize_key(key: &str) -> String {
        key.trim().to_ascii_lowercase()
    }

    pub fn get(&self, key: &str) -> Option<&FrontmatterValue> {
        let needle = Self::normalize_key(key);
        self.entries
            .iter()
            .find(|(k, _)| Self::normalize_key(k) == needle)
            .map(|(_, v)| v)
    }

    pub fn get_scalar(&self, key: &str) -> Option<String> {
        self.get(key)
            .and_then(|v| v.as_scalar().map(|s| s.to_string()))
    }

    pub fn get_list(&self, key: &str) -> Vec<String> {
        self.get(key).map(|v| v.as_list()).unwrap_or_default()
    }

    pub fn collect(&self) -> HashMap<String, String> {
        self.entries
            .iter()
            .map(|(k, v)| {
                (
                    Self::normalize_key(k),
                    v.as_scalar().unwrap_or("").to_string(),
                )
            })
            .collect()
    }
}

/// Parse a frontmatter YAML body into a best-effort ordered key/value map.
pub fn parse_frontmatter(raw: &str) -> Frontmatter {
    let mut fm = Frontmatter::empty();
    if raw.trim().is_empty() {
        return fm;
    }

    let lines: Vec<&str> = raw.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim_end();
        if line.trim().is_empty() || line.trim().starts_with('#') {
            i += 1;
            continue;
        }
        let Some(colon) = line.find(':') else {
            i += 1;
            continue;
        };
        let key = line[..colon].trim().to_string();
        let value_part = line[colon + 1..].trim().to_string();

        let (value, next_i) = if value_part.is_empty() {
            let (items, j) = collect_block_list(&lines, i + 1);
            if !items.is_empty() {
                (FrontmatterValue::List(items), j)
            } else {
                (FrontmatterValue::Scalar(String::new()), i + 1)
            }
        } else if value_part.starts_with('[') && value_part.ends_with(']') {
            let inner = &value_part[1..value_part.len() - 1];
            let items: Vec<String> = inner
                .split(',')
                .map(|part| unquote_scalar(part.trim()))
                .filter(|item| !item.is_empty())
                .collect();
            (FrontmatterValue::List(items), i + 1)
        } else if matches!(value_part.as_str(), "|" | ">" | ">-" | "|-") {
            // Folded multi-line scalars are rare in this vault; capture the first line.
            match lines.get(i + 1) {
                Some(next) => (FrontmatterValue::Scalar(unquote_scalar(next.trim())), i + 2),
                None => (FrontmatterValue::Scalar(String::new()), i + 1),
            }
        } else {
            (FrontmatterValue::Scalar(unquote_scalar(&value_part)), i + 1)
        };

        fm.entries.push((key, value));
        i = next_i;
    }

    fm
}

fn collect_block_list(lines: &[&str], start: usize) -> (Vec<String>, usize) {
    let mut items = Vec::new();
    let mut j = start;
    while j < lines.len() {
        let next = lines[j];
        if next.trim().is_empty() {
            j += 1;
            continue;
        }
        let indent = next.len() - next.trim_start().len();
        let trimmed = next.trim_start();
        if indent > 0 && (trimmed.starts_with("- ") || trimmed == "-") {
            let item = if trimmed == "-" {
                String::new()
            } else {
                unquote_scalar(trimmed[2..].trim())
            };
            items.push(item);
            j += 1;
        } else {
            break;
        }
    }
    (items, j)
}

fn unquote_scalar(value: &str) -> String {
    let trimmed = value.trim();
    if (trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2)
        || (trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() >= 2)
    {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

/// Determine the page title: prefer an H1 in the body, then a frontmatter
/// `title`, then the filename stem.
pub fn extract_title(body: &str, frontmatter: &Frontmatter, file_name: &str) -> String {
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            let title = rest.trim().to_string();
            if !title.is_empty() {
                return title;
            }
        }
        // First non-frontmatter line that isn't an H1 stops the H1 search.
        if !trimmed.starts_with('#') {
            break;
        }
    }

    if let Some(title) = frontmatter.get_scalar("title") {
        if !title.is_empty() {
            return title;
        }
    }

    file_name
        .strip_suffix(".md")
        .or_else(|| file_name.strip_suffix(".markdown"))
        .unwrap_or(file_name)
        .to_string()
}

/// Rewrite every `[[target...]]` wikilink whose target matches `old_stem`
/// (case-insensitive, comparing the part before `|` or `#`) so the target
/// becomes `new_stem`, preserving any alias, anchor, and surrounding text.
///
/// This is the rename-referential-integrity path: when a page `old.md` is
/// renamed to `new.md`, every other page that links `[[old]]`, `[[old|Alias]]`,
/// `[[old#Section]]`, or `[[old#Section|Alias]]` must be updated so the link
/// still resolves. Matching is on the link *target* (the file stem), the same
/// field `extract_wikilinks` produces, so the two stay consistent. Returns the
/// rewritten body and the number of links replaced (0 ⇒ nothing to do).
///
/// The replacement preserves the original alias and anchor exactly: only the
/// target segment is swapped. `new_stem` is inserted verbatim (callers pass the
/// already-lowercased-or-not stem they want shown).
pub fn rewrite_wikilinks(body: &str, old_stem: &str, new_stem: &str) -> (String, usize) {
    let old_lower = old_stem.to_ascii_lowercase();
    if old_lower.is_empty() || old_lower == new_stem.to_ascii_lowercase() {
        return (body.to_string(), 0);
    }
    let bytes = body.as_bytes();
    let mut out = String::with_capacity(body.len());
    let mut replaced = 0usize;
    let mut i = 0usize;
    let mut last_copy_end = 0usize;

    while i + 1 < bytes.len() {
        if bytes[i] == b'[' && bytes[i + 1] == b'[' {
            let start = i + 2;
            let mut depth = 2;
            let mut j = start;
            while j < bytes.len() {
                match bytes[j] {
                    b'[' => depth += 1,
                    b']' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            if j < bytes.len() && depth == 0 {
                // j points to the final `]` that brought depth to 0; the inner
                // content is [start..j-1) (the preceding `]` is at j-1).
                let end = j.saturating_sub(1);
                let inner = &body[start..end];
                // Split target from alias/anchor the same way extract_wikilinks
                // does: target = before first `|` or `#`.
                let target_end = inner.find(['|', '#']).unwrap_or(inner.len());
                let target = inner[..target_end].trim();
                if target.to_ascii_lowercase() == old_lower {
                    // Flush text before this link, then re-emit with new target.
                    out.push_str(&body[last_copy_end..i]);
                    out.push_str("[[");
                    out.push_str(new_stem);
                    out.push_str(&inner[target_end..]);
                    out.push_str("]]");
                    replaced += 1;
                    last_copy_end = j + 1;
                }
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    if replaced == 0 {
        return (body.to_string(), 0);
    }
    out.push_str(&body[last_copy_end..]);
    (out, replaced)
}

/// Extract `[[target]]` and `[[target|alias]]` wikilink targets from the body,
/// deduplicated, order preserved. Heading anchors (`[[#section]]`) yield the
/// empty string and are skipped.
pub fn extract_wikilinks(body: &str) -> Vec<String> {
    let mut targets: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for link in wikilink_spans(body) {
        if seen.insert(link.target.to_ascii_lowercase()) {
            targets.push(link.target);
        }
    }
    targets
}

/// Byte ranges in the original Markdown body. Every Lint edit uses these same
/// ranges as the scanner, so examples in code cannot be mistaken for links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikilinkSpan {
    pub start: usize,
    pub end: usize,
    pub target: String,
    pub label: String,
}

pub fn wikilink_spans(body: &str) -> Vec<WikilinkSpan> {
    let mut found = Vec::new();
    let mut offset = 0;
    let mut fence: Option<(u8, usize)> = None;
    let mut code_ticks = 0;
    for line in body.split_inclusive('\n') {
        let bytes = line.as_bytes();
        let indent = bytes.iter().take_while(|&&byte| byte == b' ').count();
        if indent <= 3 && indent < bytes.len() && matches!(bytes[indent], b'`' | b'~') {
            let marker = bytes[indent];
            let count = bytes[indent..]
                .iter()
                .take_while(|&&byte| byte == marker)
                .count();
            if let Some((open_marker, open_count)) = fence {
                if marker == open_marker && count >= open_count {
                    fence = None;
                }
                offset += line.len();
                continue;
            }
            if count >= 3 && code_ticks == 0 {
                fence = Some((marker, count));
                offset += line.len();
                continue;
            }
        }
        if fence.is_some() {
            offset += line.len();
            continue;
        }
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\\' && i + 1 < bytes.len() {
                i += 2;
                continue;
            }
            if bytes[i] == b'`' {
                let count = bytes[i..].iter().take_while(|&&byte| byte == b'`').count();
                if code_ticks == 0 {
                    code_ticks = count;
                } else if code_ticks == count {
                    code_ticks = 0;
                }
                i += count;
                continue;
            }
            if code_ticks == 0 && bytes[i..].starts_with(b"[[") {
                let mut depth = 2;
                let mut j = i + 2;
                while j < bytes.len() && depth > 0 {
                    if bytes[j] == b'[' {
                        depth += 1;
                    } else if bytes[j] == b']' {
                        depth -= 1;
                    }
                    j += 1;
                }
                if depth == 0 && j >= i + 4 {
                    let end = j;
                    let inner = &line[i + 2..end - 2];
                    let (destination, alias) = inner.split_once('|').unwrap_or((inner, ""));
                    let target = destination.split('#').next().unwrap_or("").trim();
                    if !target.is_empty() {
                        found.push(WikilinkSpan {
                            start: offset + i,
                            end: offset + end,
                            target: target.to_string(),
                            label: if alias.is_empty() {
                                target.to_string()
                            } else {
                                alias.to_string()
                            },
                        });
                    }
                    i = end;
                    continue;
                }
            }
            i += 1;
        }
        offset += line.len();
    }
    found
}

/// Mark Markdown prose bytes that can contain live links. This deliberately
/// excludes fenced and inline code as well as escaped punctuation.
pub fn markdown_prose_mask(body: &str) -> Vec<bool> {
    let mut visible = vec![false; body.len()];
    let mut offset = 0;
    let mut fence: Option<(u8, usize)> = None;
    let mut code_ticks = 0;
    for line in body.split_inclusive('\n') {
        let bytes = line.as_bytes();
        let indent = bytes.iter().take_while(|&&byte| byte == b' ').count();
        if indent <= 3 && indent < bytes.len() && matches!(bytes[indent], b'`' | b'~') {
            let marker = bytes[indent];
            let count = bytes[indent..]
                .iter()
                .take_while(|&&byte| byte == marker)
                .count();
            if let Some((open_marker, open_count)) = fence {
                if marker == open_marker && count >= open_count {
                    fence = None;
                }
                offset += line.len();
                continue;
            }
            if count >= 3 && code_ticks == 0 {
                fence = Some((marker, count));
                offset += line.len();
                continue;
            }
        }
        if fence.is_some() {
            offset += line.len();
            continue;
        }
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\\' && i + 1 < bytes.len() {
                visible[offset + i] = true;
                i += 2;
                continue;
            }
            if bytes[i] == b'`' {
                let count = bytes[i..].iter().take_while(|&&byte| byte == b'`').count();
                if code_ticks == 0 {
                    code_ticks = count;
                } else if code_ticks == count {
                    code_ticks = 0;
                }
                i += count;
                continue;
            }
            if code_ticks == 0 {
                visible[offset + i] = true;
            }
            i += 1;
        }
        offset += line.len();
    }
    visible
}

/// Approximate word count. Splits body (excluding code fences) on whitespace.
pub fn count_words(body: &str) -> usize {
    let mut count = 0;
    let mut in_fence = false;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        count += line.split_whitespace().count();
    }
    count
}

/// Slugify a chat question into a filesystem-safe `wiki/queries/` filename stem.
/// Keeps lowercase ASCII alphanumerics and CJK/code-point letters, collapses
/// other separators to `-`, trims to a bounded length, and falls back to
/// `query-<suffix>` when nothing usable remains (so a pure-punctuation or empty
/// question still yields a unique, valid filename).
pub fn slugify_query(query: &str, fallback_suffix: &str) -> String {
    const MAX_LEN: usize = 60;
    let mut out = String::new();
    let mut prev_dash = true;
    for ch in query.trim().chars() {
        let keep = ch.is_alphanumeric() && (ch.is_ascii() || (ch as u32) >= 0x4E00); // ASCII alnum or non-ASCII (CJK etc.)
        if keep {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    let mut stem: String = trimmed.chars().take(MAX_LEN).collect();
    stem = stem.trim_matches('-').to_string();
    if stem.is_empty() {
        stem = format!("query-{fallback_suffix}");
    }
    stem
}

/// Build a short snippet around the first case-insensitive match of any query
/// term, for search result previews. Returns `None` if no term matches.
pub fn snippet_for_query(body: &str, query_lower: &str, radius: usize) -> Option<String> {
    let query = query_lower.trim();
    if query.is_empty() {
        return None;
    }
    let terms: Vec<&str> = query.split_whitespace().collect();
    let body_lower = body.to_ascii_lowercase();
    let mut earliest: Option<usize> = None;
    for term in terms {
        let needle = term.to_ascii_lowercase();
        if needle.is_empty() {
            continue;
        }
        if let Some(pos) = body_lower.find(&needle) {
            earliest = Some(match earliest {
                Some(existing) => existing.min(pos),
                None => pos,
            });
        }
    }

    let start = earliest?;
    let line_start = body[..start].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let char_end = body[line_start..]
        .find('\n')
        .map(|p| line_start + p)
        .unwrap_or(body.len());
    // `str` ranges are byte offsets, while the requested radius is a visual
    // window. A byte offset may land in the middle of a UTF-8 code point (for
    // example, when the surrounding line contains CJK text), which would
    // panic when slicing below. Round the start down to the previous character
    // boundary and the end up to the next one, staying within this line.
    let mut snippet_start = start.saturating_sub(radius);
    if snippet_start < line_start {
        snippet_start = line_start;
    }
    while snippet_start > line_start && !body.is_char_boundary(snippet_start) {
        snippet_start -= 1;
    }
    let mut snippet_end = char_end.min(start.saturating_add(radius.max(query.len())));
    while snippet_end < char_end && !body.is_char_boundary(snippet_end) {
        snippet_end += 1;
    }
    let prefix = if snippet_start > line_start {
        "…"
    } else {
        ""
    };
    let suffix = if snippet_end < char_end { "…" } else { "" };
    let slice = body[snippet_start..snippet_end].trim();
    if slice.is_empty() {
        None
    } else {
        Some(format!("{prefix}{slice}{suffix}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_frontmatter_and_body() {
        let content = "---\ntitle: Hello\ntags: [a, b]\n---\n\n# Hello\n\nBody text.";
        let split = split_frontmatter(content);
        assert_eq!(
            split.frontmatter.as_deref(),
            Some("title: Hello\ntags: [a, b]")
        );
        assert!(split.body.starts_with("# Hello"));
    }

    #[test]
    fn handles_no_frontmatter() {
        let split = split_frontmatter("# Just a title\n\nbody");
        assert!(split.frontmatter.is_none());
        assert!(split.body.starts_with("# Just"));
    }

    #[test]
    fn handles_unclosed_frontmatter_as_body() {
        let split = split_frontmatter("---\ntitle: dangling\n\nno close fence");
        assert!(split.frontmatter.is_none());
        assert!(split.body.contains("dangling"));
    }

    #[test]
    fn parses_scalar_and_inline_list_and_block_list() {
        let raw = "title: Agent Memory\ntype: concept\ntags: [memory, context]\nsources:\n  - raw/articles/a.md\n  - raw/articles/b.md\nstarred: true";
        let fm = parse_frontmatter(raw);
        assert_eq!(fm.get_scalar("title").as_deref(), Some("Agent Memory"));
        assert_eq!(fm.get_scalar("type").as_deref(), Some("concept"));
        assert_eq!(fm.get_list("tags"), vec!["memory", "context"]);
        assert_eq!(
            fm.get_list("sources"),
            vec!["raw/articles/a.md", "raw/articles/b.md"]
        );
        assert_eq!(fm.get_scalar("starred").as_deref(), Some("true"));
    }

    #[test]
    fn frontmatter_lookups_are_case_insensitive() {
        let fm = parse_frontmatter("Title: X");
        assert_eq!(fm.get_scalar("title").as_deref(), Some("X"));
        assert_eq!(fm.get_scalar("TITLE").as_deref(), Some("X"));
    }

    #[test]
    fn unquotes_scalar_values() {
        let fm = parse_frontmatter("author: \"Jane Doe\"\nnick: 'jd'");
        assert_eq!(fm.get_scalar("author").as_deref(), Some("Jane Doe"));
        assert_eq!(fm.get_scalar("nick").as_deref(), Some("jd"));
    }

    #[test]
    fn extract_title_prefers_h1_then_frontmatter_then_filename() {
        let fm = parse_frontmatter("title: From FM");
        assert_eq!(extract_title("# From H1\nbody", &fm, "file.md"), "From H1");
        assert_eq!(extract_title("body only", &fm, "file.md"), "From FM");
        let empty = Frontmatter::empty();
        assert_eq!(
            extract_title("body only", &empty, "agent-memory.md"),
            "agent-memory"
        );
    }

    #[test]
    fn extract_wikilinks_handles_alias_anchor_and_duplicates() {
        let body = "see [[react-pattern]] and [[react-pattern|ReAct]] plus [[concepts/x#section]] and [[plain]]";
        let links = extract_wikilinks(body);
        assert_eq!(links, vec!["react-pattern", "concepts/x", "plain"]);
    }

    #[test]
    fn extract_wikilinks_ignores_code_spans_partial() {
        // Both links are ordinary prose.
        let body = "link [[target-a]] and [[target-b]]";
        let result = extract_wikilinks(body);
        assert_eq!(result, vec!["target-a", "target-b"]);
    }

    #[test]
    fn wikilink_ranges_skip_fences_inline_code_and_escapes() {
        let body = "Real [[lost|label]] and `[[lost]]` and \\[[lost]]\n```md\n[[lost]]\n```\nMore [[lost#part]].";
        let links = wikilink_spans(body);
        assert_eq!(links.len(), 2);
        assert_eq!(&body[links[0].start..links[0].end], "[[lost|label]]");
        assert_eq!(&body[links[1].start..links[1].end], "[[lost#part]]");
        assert_eq!(extract_wikilinks(body), vec!["lost"]);
    }

    #[test]
    fn rewrite_wikilinks_replaces_plain_target_case_insensitively() {
        let body = "see [[react-pattern]] and [[React-Pattern]] here";
        let (out, n) = rewrite_wikilinks(body, "react-pattern", "reasoning-loop");
        assert_eq!(n, 2);
        assert_eq!(out, "see [[reasoning-loop]] and [[reasoning-loop]] here");
    }

    #[test]
    fn rewrite_wikilinks_preserves_alias_and_anchor() {
        let body = "[[old|My Alias]] then [[old#Section]] and [[old#Section|Alias]]";
        let (out, n) = rewrite_wikilinks(body, "old", "new");
        assert_eq!(n, 3);
        assert_eq!(
            out,
            "[[new|My Alias]] then [[new#Section]] and [[new#Section|Alias]]"
        );
    }

    #[test]
    fn rewrite_wikilinks_leaves_non_matching_links_untouched() {
        let body = "[[other]] and [[old]] and [[also-old]] text";
        let (out, n) = rewrite_wikilinks(body, "old", "new");
        // `also-old` is a different target stem, must not be touched.
        assert_eq!(n, 1);
        assert_eq!(out, "[[other]] and [[new]] and [[also-old]] text");
    }

    #[test]
    fn rewrite_wikilinks_returns_zero_when_no_match_and_preserves_cjk() {
        let body = "正文 [[概念甲]] 与 [[agent]]";
        let (out, n) = rewrite_wikilinks(body, "missing", "x");
        assert_eq!(n, 0);
        assert_eq!(out, body);
    }

    #[test]
    fn rewrite_wikilinks_rewrites_cjk_targets() {
        let body = "链接 [[智能体]] 及 [[智能体|AI]]";
        let (out, n) = rewrite_wikilinks(body, "智能体", "代理");
        assert_eq!(n, 2);
        assert_eq!(out, "链接 [[代理]] 及 [[代理|AI]]");
    }

    #[test]
    fn rewrite_wikilinks_is_noop_when_old_equals_new() {
        let body = "[[old]]";
        let (out, n) = rewrite_wikilinks(body, "old", "old");
        assert_eq!(n, 0);
        assert_eq!(out, body);
    }

    #[test]
    fn rewrite_wikilinks_handles_nested_brackets_inside_alias() {
        // An alias containing `]` is unusual but the depth counter must keep
        // the link intact until the true closing `]]`.
        let body = "see [[old|cite [1]]] next";
        let (out, n) = rewrite_wikilinks(body, "old", "new");
        assert_eq!(n, 1);
        assert_eq!(out, "see [[new|cite [1]]] next");
    }

    #[test]
    fn count_words_skips_code_fences() {
        let body = "intro words\n\n```rust\nlet x = 1;\n```\n\nafter fence two words";
        assert_eq!(count_words(body), 2 + 4);
    }

    #[test]
    fn snippet_finds_first_term_match() {
        let body = "intro line\nthis mentions AgentMemory here\ntail";
        let snip = snippet_for_query(body, "agentmemory", 10).unwrap();
        assert!(snip.to_ascii_lowercase().contains("agentmemory"));
    }

    #[test]
    fn snippet_preserves_utf8_boundaries_for_cjk_text() {
        let body = "这是中文前缀，命令后面还有中文内容";
        let snip = snippet_for_query(body, "命令", 5).unwrap();
        assert!(snip.contains("命令"));
    }

    #[test]
    fn snippet_returns_none_when_no_match() {
        assert!(snippet_for_query("nothing here", "missing", 5).is_none());
    }

    #[test]
    fn slugify_query_keeps_alnum_cjk_and_collapses_separators() {
        assert_eq!(
            slugify_query("What is the ReAct pattern?", "1"),
            "what-is-the-react-pattern"
        );
        // CJK is preserved (non-ASCII code points kept).
        assert_eq!(slugify_query("什么是 Agent？", "1"), "什么是-agent");
        // Punctuation/empty collapses to the fallback stem.
        assert_eq!(slugify_query("??? !!!", "42"), "query-42");
        assert_eq!(slugify_query("", "7"), "query-7");
        // Long queries are truncated to a bounded length.
        let long = "a".repeat(200);
        let slug = slugify_query(&long, "1");
        assert!(slug.len() <= 60);
        assert!(slug.starts_with('a'));
    }
}
