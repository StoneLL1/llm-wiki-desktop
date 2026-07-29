use llm_wiki_desktop_lib::models::import_v2_file::FileScanPolicy;
use llm_wiki_desktop_lib::models::paths::ProjectContext;
use llm_wiki_desktop_lib::services::import_v2::file_discovery::FileDiscoveryService;
use std::fs;

#[test]
fn production_folder_scan_preserves_cjk_long_relative_path_and_case() {
    let temp = tempfile::tempdir().unwrap();
    let project_root = temp.path().join("project");
    let scan_root = temp.path().join("incoming");
    fs::create_dir_all(&project_root).unwrap();
    let mut relative = std::path::PathBuf::new();
    for index in 1..=9 {
        relative.push(format!("第{index:02}层{}", "资料归档".repeat(8)));
    }
    relative.push("研究笔记.MD");
    let source = scan_root.join(&relative);
    assert!(
        relative.to_string_lossy().encode_utf16().count() > 260,
        "fixture must exceed the traditional Windows MAX_PATH boundary"
    );
    assert!(
        source.to_string_lossy().encode_utf16().count() > 260,
        "absolute source fixture must exercise a real long path"
    );
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, "# 保留大小写和中文相对路径").unwrap();

    let result = FileDiscoveryService
        .scan(
            &ProjectContext::new("p", project_root),
            &[scan_root],
            FileScanPolicy::default(),
            |_| {},
            || false,
        )
        .unwrap();
    assert_eq!(result.files.len(), 1);
    assert_eq!(
        result.files[0].relative_path,
        relative.to_string_lossy().replace('\\', "/")
    );
    assert!(result.files[0].source_path.ends_with("研究笔记.MD"));
}

#[cfg(target_os = "linux")]
#[test]
fn production_folder_scan_keeps_case_and_normalization_distinct_linux_files() {
    let temp = tempfile::tempdir().unwrap();
    let project_root = temp.path().join("project");
    let scan_root = temp.path().join("incoming");
    fs::create_dir_all(&project_root).unwrap();
    fs::create_dir_all(&scan_root).unwrap();
    for name in ["A.md", "a.md", "Café.md", "Cafe\u{301}.md"] {
        fs::write(scan_root.join(name), format!("# {name}")).unwrap();
    }

    let result = FileDiscoveryService
        .scan(
            &ProjectContext::new("p", project_root),
            &[scan_root],
            FileScanPolicy::default(),
            |_| {},
            || false,
        )
        .unwrap();
    let names = result
        .files
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(names.len(), 4);
    for name in ["A.md", "a.md", "Café.md", "Cafe\u{301}.md"] {
        assert!(names.contains(name), "missing {name}: {names:?}");
    }
}
