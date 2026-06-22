//! Backend i18n helpers: map the user's `Settings.language` preference into
//! (a) the natural-language instruction appended to LLM/Agent prompts so
//! generated content answers in the user's language (CLAUDE.md hard rule:
//! "Agent 生成内容按用户语言偏好输出"), and (b) the OS-facing tray menu
//! labels/tooltip.
//!
//! Deterministic structured fields (JSON schema keys, file paths, frontmatter
//! keys, lint issueType enums) stay English regardless of this setting —
//! these helpers only steer the *prose* the model emits.

/// Return a short instruction telling the model which language to answer in,
/// e.g. `Respond in Simplified Chinese.` for `zh-CN`. Unknown codes fall back
/// to the raw code so a future locale still produces a usable instruction.
pub fn language_instruction(language: &str) -> String {
    let name = language_display_name(language);
    format!("Respond in {name}.")
}

/// Human-readable language name for prompt instructions and logs.
pub fn language_display_name(language: &str) -> &'static str {
    match language.trim() {
        "zh-CN" | "zh" | "zh-Hans" => "Simplified Chinese",
        "zh-TW" | "zh-Hant" => "Traditional Chinese",
        "en" | "en-US" | "en-GB" => "English",
        "ja" => "Japanese",
        "ko" => "Korean",
        "fr" => "French",
        "de" => "German",
        "es" => "Spanish",
        _ => "the user's preferred language",
    }
}

/// Tray menu labels + tooltip localized to the user's language.
/// Returns `(show, hide, quit, tooltip)`.
pub fn tray_labels(language: &str) -> (&'static str, &'static str, &'static str, &'static str) {
    match language.trim() {
        "zh-CN" | "zh" | "zh-Hans" => ("显示", "隐藏", "退出", "LLM Wiki 桌面版"),
        "zh-TW" | "zh-Hant" => ("顯示", "隱藏", "結束", "LLM Wiki 桌面版"),
        _ => ("Show", "Hide", "Quit", "LLM Wiki Desktop"),
    }
}

#[cfg(test)]
mod tests {
    use super::{language_display_name, language_instruction, tray_labels};

    #[test]
    fn language_instruction_names_common_locales() {
        assert!(language_instruction("zh-CN").contains("Simplified Chinese"));
        assert!(language_instruction("en").contains("English"));
        assert!(language_instruction("en-US").contains("English"));
    }

    #[test]
    fn language_instruction_falls_back_for_unknown_locale() {
        // Unknown code must still yield a usable instruction rather than panic.
        let instr = language_instruction("xx-YY");
        assert!(instr.starts_with("Respond in "));
    }

    #[test]
    fn tray_labels_switch_between_english_and_chinese() {
        assert_eq!(
            tray_labels("en"),
            ("Show", "Hide", "Quit", "LLM Wiki Desktop")
        );
        assert_eq!(
            tray_labels("zh-CN"),
            ("显示", "隐藏", "退出", "LLM Wiki 桌面版")
        );
        assert_eq!(
            tray_labels("zh"),
            ("显示", "隐藏", "退出", "LLM Wiki 桌面版")
        );
    }

    #[test]
    fn display_name_handles_whitespace() {
        assert_eq!(language_display_name("  zh-CN  "), "Simplified Chinese");
    }
}
