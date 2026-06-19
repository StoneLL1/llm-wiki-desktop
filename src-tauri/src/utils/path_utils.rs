pub fn normalize_project_path(path: &str) -> String {
    path.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::normalize_project_path;

    #[test]
    fn normalizes_windows_separators() {
        assert_eq!(
            normalize_project_path("wiki\\概念\\Agent.md"),
            "wiki/概念/Agent.md"
        );
    }
}
