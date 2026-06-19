pub fn normalize_project_path(path: &str) -> String {
    path.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::normalize_project_path;
    use crate::models::paths::ProjectContext;
    use std::path::PathBuf;

    #[test]
    fn normalizes_windows_separators() {
        assert_eq!(
            normalize_project_path("wiki\\概念\\Agent.md"),
            "wiki/概念/Agent.md"
        );
    }

    #[test]
    fn project_context_resolves_safe_relative_paths() {
        let root = PathBuf::from("D:/Projects/wiki");
        let context = ProjectContext::new("project-1", root.clone());

        let resolved = context
            .resolve_project_path("wiki\\概念\\Agent.md")
            .expect("safe relative path should resolve");

        assert_eq!(resolved, root.join("wiki").join("概念").join("Agent.md"));
    }

    #[test]
    fn project_context_rejects_parent_traversal() {
        let context = ProjectContext::new("project-1", PathBuf::from("D:/Projects/wiki"));

        let error = context
            .resolve_project_path("../outside.md")
            .expect_err("parent traversal must be rejected");

        assert_eq!(error.code, "PATH_TRAVERSAL");
        assert!(error.user_action_required);
    }

    #[test]
    fn project_context_rejects_absolute_path_injection() {
        let context = ProjectContext::new("project-1", PathBuf::from("D:/Projects/wiki"));

        let error = context
            .resolve_project_path("C:/Users/Aletta/secrets.md")
            .expect_err("absolute path injection must be rejected");

        assert_eq!(error.code, "PATH_ABSOLUTE_NOT_ALLOWED");
        assert!(error.user_action_required);
    }

    #[test]
    fn project_context_converts_absolute_path_to_project_relative() {
        let root = PathBuf::from("D:/Projects/wiki");
        let context = ProjectContext::new("project-1", root.clone());

        let relative = context
            .to_project_relative(&root.join("wiki").join("概念").join("Agent.md"))
            .expect("path under project root should convert");

        assert_eq!(relative, "wiki/概念/Agent.md");
    }

    #[test]
    fn project_context_rejects_relative_conversion_outside_root() {
        let context = ProjectContext::new("project-1", PathBuf::from("D:/Projects/wiki"));

        let error = context
            .to_project_relative(&PathBuf::from("D:/Other/wiki/Agent.md"))
            .expect_err("outside path must be rejected");

        assert_eq!(error.code, "PATH_OUTSIDE_PROJECT");
        assert!(error.user_action_required);
    }

    #[test]
    fn project_context_rejects_relative_conversion_that_escapes_lexically() {
        let context = ProjectContext::new("project-1", PathBuf::from("D:/Projects/wiki"));

        let error = context
            .to_project_relative(&PathBuf::from("D:/Projects/wiki/../secrets.md"))
            .expect_err("lexical escape after prefix stripping must be rejected");

        assert_eq!(error.code, "PATH_TRAVERSAL");
        assert!(error.user_action_required);
    }

    #[test]
    fn project_context_rejects_empty_and_current_directory_paths() {
        let context = ProjectContext::new("project-1", PathBuf::from("D:/Projects/wiki"));

        let empty_error = context
            .resolve_project_path("")
            .expect_err("empty path must be rejected");
        let dot_error = context
            .resolve_project_path(".")
            .expect_err("current directory path must be rejected");

        assert_eq!(empty_error.code, "PATH_INVALID");
        assert_eq!(dot_error.code, "PATH_INVALID");
    }

    #[test]
    fn project_context_rejects_rooted_unix_and_unc_style_inputs() {
        let context = ProjectContext::new("project-1", PathBuf::from("D:/Projects/wiki"));

        let unix_error = context
            .resolve_project_path("/etc/passwd")
            .expect_err("rooted unix path must be rejected");
        let unc_error = context
            .resolve_project_path("\\\\server\\share\\secrets.md")
            .expect_err("UNC-style path must be rejected");

        assert_eq!(unix_error.code, "PATH_ABSOLUTE_NOT_ALLOWED");
        assert_eq!(unc_error.code, "PATH_ABSOLUTE_NOT_ALLOWED");
    }

    #[test]
    #[cfg(unix)]
    fn project_context_rejects_symlink_escape_where_detectable() {
        use std::fs;
        use std::os::unix::fs::symlink;
        use std::time::{SystemTime, UNIX_EPOCH};

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("llm-wiki-path-test-{suffix}"));
        let root = base.join("project");
        let outside = base.join("outside");

        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        symlink(&outside, root.join("linked-out")).unwrap();

        let context = ProjectContext::new("project-1", root.clone());
        let error = context
            .resolve_project_path("linked-out/file.md")
            .expect_err("symlink escape must be rejected when detectable");

        assert_eq!(error.code, "PATH_OUTSIDE_PROJECT");

        fs::remove_dir_all(base).unwrap();
    }
}
