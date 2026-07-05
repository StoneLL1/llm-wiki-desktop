const MAX_CONVENIENCE_FILES: usize = 3;
const MAX_CONVENIENCE_CHANGED_CHARS: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatIntent {
    ReadOnly,
    Write,
    Ambiguous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvenienceAuditStatus {
    Passed,
    SoftViolation,
    HardViolation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangedFileKind {
    Modified,
    Added,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFile {
    pub path: String,
    pub kind: ChangedFileKind,
    pub changed_chars: usize,
}

impl ChangedFile {
    pub fn modified(path: impl Into<String>, changed_chars: usize) -> Self {
        Self {
            path: path.into(),
            kind: ChangedFileKind::Modified,
            changed_chars,
        }
    }

    pub fn added(path: impl Into<String>, changed_chars: usize) -> Self {
        Self {
            path: path.into(),
            kind: ChangedFileKind::Added,
            changed_chars,
        }
    }

    pub fn deleted(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            kind: ChangedFileKind::Deleted,
            changed_chars: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvenienceAuditReport {
    pub status: ConvenienceAuditStatus,
    pub affected_paths: Vec<String>,
    pub diff_summary: String,
    pub violation_reason: Option<String>,
}

#[derive(Default)]
pub struct ChatConvenienceService;

impl ChatConvenienceService {
    pub fn classify_chat_intent(&self, input: &str) -> ChatIntent {
        classify_chat_intent(input)
    }

    pub fn audit_changed_paths(&self, changes: Vec<ChangedFile>) -> ConvenienceAuditReport {
        audit_changed_paths(changes)
    }

    pub fn convenience_prompt_suffix(&self) -> &'static str {
        convenience_prompt_suffix()
    }
}

pub fn classify_chat_intent(input: &str) -> ChatIntent {
    let normalized = input.trim().to_lowercase();
    if normalized.is_empty() {
        return ChatIntent::Ambiguous;
    }

    let has_write = contains_any(
        &normalized,
        &[
            "保存", "存成", "写入", "写成", "新建", "创建", "新增", "修改", "更新", "编辑", "改写",
            "重写", "整理", "补", "添加", "加入", "删除", "移除", "save", "write", "edit",
            "update", "create", "add", "append", "delete", "remove", "rewrite",
        ],
    );
    if has_write {
        return ChatIntent::Write;
    }

    let has_read_only = contains_any(
        &normalized,
        &[
            "分析",
            "解释",
            "说明",
            "看看",
            "看一下",
            "检查",
            "评价",
            "评估",
            "问题",
            "为什么",
            "是什么",
            "怎么样",
            "analyze",
            "explain",
            "review",
            "inspect",
            "summarize",
            "what",
            "why",
            "how",
        ],
    );
    if has_read_only {
        return ChatIntent::ReadOnly;
    }

    ChatIntent::Ambiguous
}

pub fn audit_changed_paths(changes: Vec<ChangedFile>) -> ConvenienceAuditReport {
    let affected_paths: Vec<String> = changes
        .iter()
        .map(|change| normalize_project_path(&change.path))
        .collect();

    if let Some(reason) = hard_violation_reason(&changes) {
        return ConvenienceAuditReport {
            status: ConvenienceAuditStatus::HardViolation,
            diff_summary: summarize_changes(changes.len(), &affected_paths),
            affected_paths,
            violation_reason: Some(reason),
        };
    }

    if changes.len() > MAX_CONVENIENCE_FILES {
        return ConvenienceAuditReport {
            status: ConvenienceAuditStatus::SoftViolation,
            diff_summary: summarize_changes(changes.len(), &affected_paths),
            affected_paths,
            violation_reason: Some(format!(
                "Convenience edits are limited to {MAX_CONVENIENCE_FILES} wiki Markdown files."
            )),
        };
    }

    if changes
        .iter()
        .any(|change| change.changed_chars > MAX_CONVENIENCE_CHANGED_CHARS)
    {
        return ConvenienceAuditReport {
            status: ConvenienceAuditStatus::SoftViolation,
            diff_summary: summarize_changes(changes.len(), &affected_paths),
            affected_paths,
            violation_reason: Some(format!(
                "Convenience edits are limited to {MAX_CONVENIENCE_CHANGED_CHARS} changed characters per file."
            )),
        };
    }

    ConvenienceAuditReport {
        status: ConvenienceAuditStatus::Passed,
        diff_summary: summarize_changes(changes.len(), &affected_paths),
        affected_paths,
        violation_reason: None,
    }
}

pub fn convenience_prompt_suffix() -> &'static str {
    "\n\nConvenience edit policy:\n\
     - Write only small Markdown edits under wiki/.\n\
     - Never delete files.\n\
     - Never modify raw/sources/ or .app/settings.json or .app/agent-config.json.\n\
     - Change at most 3 wiki Markdown files and at most 2000 characters per file.\n\
     - You may read, list, and search project files and network documentation.\n\
     - Do not install packages, download binaries, or run remote scripts."
}

fn contains_any(input: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| input.contains(needle))
}

fn hard_violation_reason(changes: &[ChangedFile]) -> Option<String> {
    for change in changes {
        let path = normalize_project_path(&change.path);
        let lower_path = path.to_lowercase();

        if matches!(change.kind, ChangedFileKind::Deleted) {
            return Some(format!("Convenience edits cannot delete files: {path}"));
        }
        if lower_path.starts_with("raw/sources/") {
            return Some(format!(
                "Convenience edits cannot modify raw sources: {path}"
            ));
        }
        if matches!(
            lower_path.as_str(),
            ".app/settings.json" | ".app/agent-config.json"
        ) {
            return Some(format!(
                "Convenience edits cannot modify protected app config: {path}"
            ));
        }
        if !lower_path.starts_with("wiki/") {
            return Some(format!(
                "Convenience edits must stay under wiki Markdown files: {path}"
            ));
        }
        if !lower_path.ends_with(".md") {
            return Some(format!(
                "Convenience edits can only modify Markdown files under wiki/: {path}"
            ));
        }
    }
    None
}

fn normalize_project_path(path: &str) -> String {
    path.trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string()
}

fn summarize_changes(count: usize, affected_paths: &[String]) -> String {
    if count == 0 {
        return "No file changes.".to_string();
    }
    let preview = affected_paths
        .iter()
        .take(3)
        .cloned()
        .collect::<Vec<String>>()
        .join(", ");
    if count <= 3 {
        format!("{count} file change(s): {preview}")
    } else {
        format!("{count} file change(s): {preview}, ...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_read_only_write_and_ambiguous_intents() {
        assert_eq!(
            classify_chat_intent("分析一下这个页面的问题"),
            ChatIntent::ReadOnly
        );
        assert_eq!(
            classify_chat_intent("帮我整理这一页并补摘要"),
            ChatIntent::Write
        );
        assert_eq!(
            classify_chat_intent("save this answer as a page"),
            ChatIntent::Write
        );
        assert_eq!(
            classify_chat_intent("这页有点乱，帮我看看"),
            ChatIntent::ReadOnly
        );
        assert_eq!(classify_chat_intent("帮我处理一下"), ChatIntent::Ambiguous);
    }

    #[test]
    fn audit_accepts_three_small_wiki_markdown_changes() {
        let report = audit_changed_paths(vec![
            ChangedFile::modified("wiki/a.md", 100),
            ChangedFile::modified("wiki/index.md", 100),
            ChangedFile::modified("wiki/log.md", 100),
        ]);
        assert_eq!(report.status, ConvenienceAuditStatus::Passed);
    }

    #[test]
    fn audit_soft_violates_large_or_many_wiki_changes() {
        let many = audit_changed_paths(vec![
            ChangedFile::modified("wiki/a.md", 10),
            ChangedFile::modified("wiki/b.md", 10),
            ChangedFile::modified("wiki/c.md", 10),
            ChangedFile::modified("wiki/d.md", 10),
        ]);
        assert_eq!(many.status, ConvenienceAuditStatus::SoftViolation);

        let large = audit_changed_paths(vec![ChangedFile::modified("wiki/a.md", 2001)]);
        assert_eq!(large.status, ConvenienceAuditStatus::SoftViolation);
    }

    #[test]
    fn audit_hard_violates_delete_raw_config_and_outside_wiki() {
        for change in [
            ChangedFile::deleted("wiki/a.md"),
            ChangedFile::modified("raw/sources/pdfs/a.pdf", 10),
            ChangedFile::modified(".app/settings.json", 10),
            ChangedFile::modified("purpose.md", 10),
        ] {
            let report = audit_changed_paths(vec![change]);
            assert_eq!(report.status, ConvenienceAuditStatus::HardViolation);
        }
    }
}
