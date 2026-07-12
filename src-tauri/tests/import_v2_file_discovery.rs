use llm_wiki_desktop_lib::models::import_v2_file::{FileScanPolicy, FileSkipReason};
use llm_wiki_desktop_lib::models::paths::ProjectContext;
use llm_wiki_desktop_lib::services::import_v2::file_discovery::{identify_file, new_import_inputs, FileDiscoveryService};
use llm_wiki_desktop_lib::models::import_v2::{ImportInput, ImportInputKind, ImportItem, ImportResourceMode, ImportSession};
use std::fs;

fn context(root: &std::path::Path) -> ProjectContext {
    ProjectContext::new("p", root.to_path_buf())
}

#[test]
fn scans_breadth_first_and_reports_limits_without_placeholder_files() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let input = temp.path().join("输入");
    fs::create_dir_all(project.join("wiki")).unwrap();
    fs::create_dir_all(input.join("一层").join("二层")).unwrap();
    fs::write(input.join("首页.md"), "# 首页").unwrap();
    fs::write(input.join("一层").join("说明.md"), "# 说明").unwrap();
    fs::write(input.join("一层").join("二层").join("太深.md"), "x").unwrap();
    fs::write(input.join("程序.exe"), b"MZ").unwrap();
    fs::write(input.join(".hidden.md"), "secret").unwrap();
    fs::write(project.join("wiki").join("internal.md"), "x").unwrap();

    let result = FileDiscoveryService::default()
        .scan(
            &context(&project),
            &[input.clone(), project.join("wiki")],
            FileScanPolicy { max_depth: 1, max_files: 10, max_file_bytes: 1024, include_hidden: false },
            |_| {},
            || false,
        )
        .unwrap();
    assert_eq!(result.files.iter().map(|f| f.display_name.as_str()).collect::<Vec<_>>(), vec!["首页.md", "说明.md"]);
    assert!(result.skipped.iter().any(|s| s.reason == FileSkipReason::UnsupportedFormat));
    assert!(result.skipped.iter().any(|s| s.reason == FileSkipReason::HiddenOrSystem));
    assert!(result.skipped.iter().any(|s| s.reason == FileSkipReason::ProjectInternal));
    assert!(result.skipped.iter().any(|s| s.reason == FileSkipReason::DepthLimitExceeded));
    assert!(result.truncated);
}

#[test]
fn enforces_file_count_size_duplicates_and_cancellation() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let input = temp.path().join("input");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&input).unwrap();
    fs::write(input.join("a.md"), "a").unwrap();
    fs::write(input.join("b.md"), "0123456789").unwrap();
    let service = FileDiscoveryService::default();
    let result = service.scan(&context(&project), &[input.join("a.md"), input.clone(), input.join("a.md")], FileScanPolicy { max_depth: 3, max_files: 1, max_file_bytes: 4, include_hidden: false }, |_| {}, || false).unwrap();
    assert_eq!(result.files.len(), 1);
    assert!(result.skipped.iter().any(|s| s.reason == FileSkipReason::Duplicate));
    assert!(result.skipped.iter().any(|s| s.reason == FileSkipReason::FileTooLarge));
    let cancelled = service.scan(&context(&project), &[input], FileScanPolicy::default(), |_| {}, || true);
    assert!(cancelled.is_err());
}

#[test]
fn requires_real_ooxml_structure_and_sniffs_magic() {
    let temp = tempfile::tempdir().unwrap();
    let fake = temp.path().join("fake.docx");
    fs::write(&fake, b"PK\x03\x04not-a-zip").unwrap();
    assert!(identify_file(&fake, b"PK\x03\x04not-a-zip").is_err());
    let docx = temp.path().join("real.docx");
    {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(fs::File::create(&docx).unwrap());
        zip.start_file("[Content_Types].xml", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(b"<Types/>").unwrap();
        zip.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(b"<document/>").unwrap();
        zip.finish().unwrap();
    }
    let prefix = fs::read(&docx).unwrap();
    assert_eq!(identify_file(&docx, &prefix[..prefix.len().min(8192)]).unwrap().magic, "ooxml-zip");
    let unsafe_docx = temp.path().join("unsafe.docx");
    {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(fs::File::create(&unsafe_docx).unwrap());
        for name in ["[Content_Types].xml", "word/document.xml", "..\\escape.xml"] {
            zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(b"x").unwrap();
        }
        zip.finish().unwrap();
    }
    let prefix = fs::read(&unsafe_docx).unwrap();
    assert!(identify_file(&unsafe_docx, &prefix[..prefix.len().min(8192)]).is_err());
    let pdf = temp.path().join("paper.pdf");
    fs::write(&pdf, b"%PDF-1.7\n").unwrap();
    assert_eq!(identify_file(&pdf, b"%PDF-1.7\n").unwrap().mime, "application/pdf");
    let markdown = temp.path().join("note.md");
    fs::write(&markdown, "你好").unwrap();
    assert_eq!(identify_file(&markdown, "你好".as_bytes()).unwrap().mime, "text/markdown");
}

#[test]
fn repeated_append_drops_existing_and_in_batch_duplicates() {
    let mut session = ImportSession::new("s", "p", ImportResourceMode::Balanced);
    session.items.push(ImportItem::queued("i", ImportInput { kind: ImportInputKind::File, display_name: "a.md".into(), locator: "C:/Input/A.md".into(), normalized_locator: Some("c:/input/a.md".into()) }));
    let file = llm_wiki_desktop_lib::models::import_v2_file::DiscoveredFile { source_path: "C:/Input/A.md".into(), relative_path: "A.md".into(), display_name: "A.md".into(), format: llm_wiki_desktop_lib::models::import_v2_file::FileFormat::Markdown, size_bytes: 1, identity: llm_wiki_desktop_lib::models::import_v2_file::FileIdentity { extension: "md".into(), magic: "utf-8".into(), mime: "text/markdown".into() } };
    assert!(new_import_inputs(&session, [file.clone(), file]).is_empty());
}

#[cfg(unix)]
#[test]
fn skips_symlinks_before_following_them() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let input = temp.path().join("input");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&input).unwrap();
    fs::write(input.join("a.md"), "a").unwrap();
    symlink(&input, temp.path().join("loop")).unwrap();
    let result = FileDiscoveryService::default().scan(&context(&project), &[temp.path().join("loop")], FileScanPolicy::default(), |_| {}, || false).unwrap();
    assert!(result.files.is_empty());
    assert_eq!(result.skipped[0].reason, FileSkipReason::SymlinkOrReparsePoint);
}
