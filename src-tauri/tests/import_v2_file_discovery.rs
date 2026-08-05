use llm_wiki_desktop_lib::models::import_v2::{
    ImportInput, ImportInputKind, ImportItem, ImportResourceMode, ImportSession,
};
use llm_wiki_desktop_lib::models::import_v2_file::{
    FileContentKind, FileFormat, FileScanPolicy, FileSkipReason,
};
use llm_wiki_desktop_lib::models::paths::ProjectContext;
use llm_wiki_desktop_lib::services::import_v2::file_discovery::{
    identify_file, new_import_inputs, FileDiscoveryService,
};
use llm_wiki_desktop_lib::services::import_v2::routes_for_format;
use std::fs;

#[derive(Clone, Copy)]
struct LocalFormatCase {
    fixture: &'static str,
    format: FileFormat,
    content_kind: FileContentKind,
    routes: &'static [&'static str],
}

const LOCAL_FORMAT_CASES: &[LocalFormatCase] = &[
    LocalFormatCase {
        fixture: "note.md",
        format: FileFormat::Markdown,
        content_kind: FileContentKind::Document,
        routes: &["file.native"],
    },
    LocalFormatCase {
        fixture: "note.txt",
        format: FileFormat::Text,
        content_kind: FileContentKind::Document,
        routes: &["file.native"],
    },
    LocalFormatCase {
        fixture: "page.html",
        format: FileFormat::Html,
        content_kind: FileContentKind::Document,
        routes: &["file.native"],
    },
    LocalFormatCase {
        fixture: "small.csv",
        format: FileFormat::Csv,
        content_kind: FileContentKind::Document,
        routes: &["file.csv-package"],
    },
    LocalFormatCase {
        fixture: "legacy.doc",
        format: FileFormat::Doc,
        content_kind: FileContentKind::Document,
        routes: &["pack.office-legacy", "pack.office-oxide", "agent.office"],
    },
    LocalFormatCase {
        fixture: "document.docx",
        format: FileFormat::Docx,
        content_kind: FileContentKind::Document,
        routes: &[
            "office.modern.docx",
            "pack.markitdown",
            "pack.office-oxide",
            "agent.office",
        ],
    },
    LocalFormatCase {
        fixture: "legacy.xls",
        format: FileFormat::Xls,
        content_kind: FileContentKind::Document,
        routes: &["pack.office-legacy", "pack.office-oxide", "agent.office"],
    },
    LocalFormatCase {
        fixture: "workbook.xlsx",
        format: FileFormat::Xlsx,
        content_kind: FileContentKind::Document,
        routes: &[
            "office.modern.xlsx",
            "pack.markitdown",
            "pack.office-oxide",
            "agent.office",
        ],
    },
    LocalFormatCase {
        fixture: "legacy.ppt",
        format: FileFormat::Ppt,
        content_kind: FileContentKind::Document,
        routes: &["pack.office-legacy", "pack.office-oxide", "agent.office"],
    },
    LocalFormatCase {
        fixture: "presentation.pptx",
        format: FileFormat::Pptx,
        content_kind: FileContentKind::Document,
        routes: &[
            "office.modern.pptx",
            "pack.markitdown",
            "pack.office-oxide",
            "agent.office",
        ],
    },
    LocalFormatCase {
        fixture: "document.pdf",
        format: FileFormat::Pdf,
        content_kind: FileContentKind::Document,
        routes: &[
            "pdf.text",
            "pdf.layout",
            "ocr.cjk-accurate",
            "ocr.basic",
            "agent.pdf",
        ],
    },
    LocalFormatCase {
        fixture: "image.png",
        format: FileFormat::Png,
        content_kind: FileContentKind::Image,
        routes: &["ocr.cjk-accurate", "ocr.basic"],
    },
    LocalFormatCase {
        fixture: "image.jpg",
        format: FileFormat::Jpeg,
        content_kind: FileContentKind::Image,
        routes: &["ocr.cjk-accurate", "ocr.basic"],
    },
    LocalFormatCase {
        fixture: "image.webp",
        format: FileFormat::Webp,
        content_kind: FileContentKind::Image,
        routes: &["ocr.cjk-accurate", "ocr.basic"],
    },
    LocalFormatCase {
        fixture: "image.bmp",
        format: FileFormat::Bmp,
        content_kind: FileContentKind::Image,
        routes: &["ocr.cjk-accurate", "ocr.basic"],
    },
    LocalFormatCase {
        fixture: "image.tiff",
        format: FileFormat::Tiff,
        content_kind: FileContentKind::Image,
        routes: &["ocr.cjk-accurate", "ocr.basic"],
    },
    LocalFormatCase {
        fixture: "media.heic",
        format: FileFormat::Heic,
        content_kind: FileContentKind::Image,
        routes: &["ocr.cjk-accurate", "ocr.basic"],
    },
    LocalFormatCase {
        fixture: "media.heif",
        format: FileFormat::Heif,
        content_kind: FileContentKind::Image,
        routes: &["ocr.cjk-accurate", "ocr.basic"],
    },
    LocalFormatCase {
        fixture: "animated.gif",
        format: FileFormat::AnimatedGif,
        content_kind: FileContentKind::Video,
        routes: &["media.companion", "media.asr"],
    },
    LocalFormatCase {
        fixture: "audio.mp3",
        format: FileFormat::Mp3,
        content_kind: FileContentKind::Audio,
        routes: &["media.companion", "media.asr"],
    },
    LocalFormatCase {
        fixture: "audio.wav",
        format: FileFormat::Wav,
        content_kind: FileContentKind::Audio,
        routes: &["media.companion", "media.asr"],
    },
    LocalFormatCase {
        fixture: "media.m4a",
        format: FileFormat::M4a,
        content_kind: FileContentKind::Audio,
        routes: &["media.companion", "media.asr"],
    },
    LocalFormatCase {
        fixture: "audio.aac",
        format: FileFormat::Aac,
        content_kind: FileContentKind::Audio,
        routes: &["media.companion", "media.asr"],
    },
    LocalFormatCase {
        fixture: "audio.flac",
        format: FileFormat::Flac,
        content_kind: FileContentKind::Audio,
        routes: &["media.companion", "media.asr"],
    },
    LocalFormatCase {
        fixture: "audio.ogg",
        format: FileFormat::Ogg,
        content_kind: FileContentKind::Audio,
        routes: &["media.companion", "media.asr"],
    },
    LocalFormatCase {
        fixture: "audio.opus",
        format: FileFormat::Opus,
        content_kind: FileContentKind::Audio,
        routes: &["media.companion", "media.asr"],
    },
    LocalFormatCase {
        fixture: "audio.wma",
        format: FileFormat::Wma,
        content_kind: FileContentKind::Audio,
        routes: &["media.companion", "media.asr"],
    },
    LocalFormatCase {
        fixture: "media.mp4",
        format: FileFormat::Mp4,
        content_kind: FileContentKind::Video,
        routes: &["media.companion", "media.asr"],
    },
    LocalFormatCase {
        fixture: "media.mov",
        format: FileFormat::Mov,
        content_kind: FileContentKind::Video,
        routes: &["media.companion", "media.asr"],
    },
    LocalFormatCase {
        fixture: "video.mkv",
        format: FileFormat::Mkv,
        content_kind: FileContentKind::Video,
        routes: &["media.companion", "media.asr"],
    },
    LocalFormatCase {
        fixture: "video.webm",
        format: FileFormat::Webm,
        content_kind: FileContentKind::Video,
        routes: &["media.companion", "media.asr"],
    },
    LocalFormatCase {
        fixture: "video.avi",
        format: FileFormat::Avi,
        content_kind: FileContentKind::Video,
        routes: &["media.companion", "media.asr"],
    },
    LocalFormatCase {
        fixture: "media.m4v",
        format: FileFormat::M4v,
        content_kind: FileContentKind::Video,
        routes: &["media.companion", "media.asr"],
    },
    LocalFormatCase {
        fixture: "video.wmv",
        format: FileFormat::Wmv,
        content_kind: FileContentKind::Video,
        routes: &["media.companion", "media.asr"],
    },
    LocalFormatCase {
        fixture: "subtitle.srt",
        format: FileFormat::Srt,
        content_kind: FileContentKind::Subtitle,
        routes: &["media.subtitle"],
    },
    LocalFormatCase {
        fixture: "subtitle.vtt",
        format: FileFormat::Vtt,
        content_kind: FileContentKind::Subtitle,
        routes: &["media.subtitle"],
    },
    LocalFormatCase {
        fixture: "subtitle.ass",
        format: FileFormat::Ass,
        content_kind: FileContentKind::Subtitle,
        routes: &["media.subtitle"],
    },
    LocalFormatCase {
        fixture: "subtitle.lrc",
        format: FileFormat::Lrc,
        content_kind: FileContentKind::Subtitle,
        routes: &["media.subtitle"],
    },
];

fn batch3_matrix_fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/import-v2/local/batch3/matrix")
        .join(name)
}

fn context(root: &std::path::Path) -> ProjectContext {
    ProjectContext::new("p", root.to_path_buf())
}

#[test]
fn discovers_and_routes_the_complete_local_format_matrix_from_real_fixtures() {
    for case in LOCAL_FORMAT_CASES {
        let path = batch3_matrix_fixture(case.fixture);
        let bytes = fs::read(&path).unwrap();
        let (format, identity) = identify_file(&path, &bytes[..bytes.len().min(8192)]).unwrap();
        assert_eq!(format, case.format, "fixture {}", case.fixture);
        assert_eq!(
            format.content_kind(),
            case.content_kind,
            "fixture {}",
            case.fixture
        );
        assert_eq!(
            routes_for_format(format),
            case.routes,
            "fixture {}",
            case.fixture
        );
        assert!(
            !identity.extension_mismatch,
            "fixture {} unexpectedly mismatched",
            case.fixture
        );
    }
}

#[test]
fn trustworthy_content_wins_over_a_mismatched_extension() {
    let fixture = batch3_matrix_fixture("document.pdf");
    let temp = tempfile::tempdir().unwrap();
    let misleading = temp.path().join("报告.txt");
    fs::copy(fixture, &misleading).unwrap();
    let bytes = fs::read(&misleading).unwrap();
    let (format, identity) = identify_file(&misleading, &bytes[..bytes.len().min(8192)]).unwrap();
    assert_eq!(format, FileFormat::Pdf);
    assert!(identity.extension_mismatch);
    assert_eq!(routes_for_format(format)[0], "pdf.text");
}

#[test]
fn ocr_asr_and_companion_edge_fixtures_reach_their_batch_three_routes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/import-v2/local/batch3");
    for (name, format, routes) in [
        (
            "image-with-text.png",
            FileFormat::Png,
            &["ocr.cjk-accurate", "ocr.basic"][..],
        ),
        (
            "image-no-text.png",
            FileFormat::Png,
            &["ocr.cjk-accurate", "ocr.basic"][..],
        ),
        (
            "speech.wav",
            FileFormat::Wav,
            &["media.companion", "media.asr"][..],
        ),
        (
            "silence.wav",
            FileFormat::Wav,
            &["media.companion", "media.asr"][..],
        ),
        (
            "companion-lrc.lrc",
            FileFormat::Lrc,
            &["media.subtitle"][..],
        ),
    ] {
        let path = root.join(name);
        let bytes = fs::read(&path).unwrap();
        let (actual, _) = identify_file(&path, &bytes[..bytes.len().min(8192)]).unwrap();
        assert_eq!(actual, format, "{name}");
        assert_eq!(routes_for_format(actual), routes, "{name}");
    }
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
            FileScanPolicy {
                max_depth: 1,
                max_files: 10,
                max_file_bytes: 1024,
                include_hidden: false,
            },
            |_| {},
            || false,
        )
        .unwrap();
    assert_eq!(
        result
            .files
            .iter()
            .map(|f| f.display_name.as_str())
            .collect::<Vec<_>>(),
        vec!["首页.md", "说明.md"]
    );
    let first = &result.files[0];
    assert_eq!(first.source_identity.size_bytes, first.size_bytes);
    assert_eq!(first.source_identity.sha256.len(), 64);
    assert_eq!(first.source_identity.magic.len(), 64);
    assert!(std::path::Path::new(&first.source_identity.canonical_path).is_absolute());
    assert!(result
        .skipped
        .iter()
        .any(|s| s.reason == FileSkipReason::UnsupportedFormat));
    assert!(result
        .skipped
        .iter()
        .any(|s| s.reason == FileSkipReason::HiddenOrSystem));
    assert!(result
        .skipped
        .iter()
        .any(|s| s.reason == FileSkipReason::ProjectInternal));
    assert!(result
        .skipped
        .iter()
        .any(|s| s.reason == FileSkipReason::DepthLimitExceeded));
    assert!(result.truncated);
}

#[test]
fn folder_scan_emits_only_file_inputs_and_keeps_unsupported_in_the_summary() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let folder = temp.path().join("资料集");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(folder.join("章节")).unwrap();
    fs::write(folder.join("章节").join("说明.md"), "# folder member").unwrap();
    fs::write(folder.join("不可导入.exe"), b"MZ").unwrap();

    let result = FileDiscoveryService::default()
        .scan(
            &context(&project),
            &[folder],
            FileScanPolicy::default(),
            |_| {},
            || false,
        )
        .unwrap();
    let session = ImportSession::new("folder", "p", ImportResourceMode::Balanced);
    let inputs = new_import_inputs(&session, result.files);
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0].kind, ImportInputKind::File);
    assert_eq!(inputs[0].display_name, "章节/说明.md");
    assert!(result.skipped.iter().any(|skipped| {
        skipped.reason == FileSkipReason::UnsupportedFormat
            && skipped.relative_path.as_deref() == Some("不可导入.exe")
    }));
    assert!(
        session.items.is_empty(),
        "a folder Source must never be queued"
    );
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
    let result = service
        .scan(
            &context(&project),
            &[input.join("a.md"), input.clone(), input.join("a.md")],
            FileScanPolicy {
                max_depth: 3,
                max_files: 1,
                max_file_bytes: 4,
                include_hidden: false,
            },
            |_| {},
            || false,
        )
        .unwrap();
    assert_eq!(result.files.len(), 1);
    assert!(result
        .skipped
        .iter()
        .any(|s| s.reason == FileSkipReason::Duplicate));
    assert!(result
        .skipped
        .iter()
        .any(|s| s.reason == FileSkipReason::FileTooLarge));
    let cancelled = service.scan(
        &context(&project),
        &[input],
        FileScanPolicy::default(),
        |_| {},
        || true,
    );
    assert!(cancelled.is_err());
}

#[test]
fn batch_a_expected_red_counts_one_discovery_callback_per_file() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    fs::create_dir_all(&project).unwrap();
    for count in [100usize, 1_000, 10_000] {
        let input = temp.path().join(format!("input-{count}"));
        fs::create_dir_all(&input).unwrap();
        for index in 0..count {
            fs::write(input.join(format!("{index:05}.md")), "# fixture").unwrap();
        }
        let mut callback_invocations = 0usize;
        let mut callback_items = 0usize;
        let result = FileDiscoveryService::default()
            .scan(
                &context(&project),
                &[input],
                FileScanPolicy::default(),
                |batch| {
                    callback_invocations += 1;
                    callback_items += batch.len();
                },
                || false,
            )
            .unwrap();
        assert_eq!(result.files.len(), count);
        assert!(
            callback_invocations <= count.div_ceil(128) + 1,
            "discovery callback count must remain bounded for {count} files"
        );
        assert_eq!(callback_items, count);
    }
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
        zip.start_file(
            "[Content_Types].xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"<Types/>").unwrap();
        zip.start_file(
            "word/document.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"<document/>").unwrap();
        zip.finish().unwrap();
    }
    let prefix = fs::read(&docx).unwrap();
    assert_eq!(
        identify_file(&docx, &prefix[..prefix.len().min(8192)])
            .unwrap()
            .1
            .magic,
        "ooxml-zip"
    );
    let unsafe_docx = temp.path().join("unsafe.docx");
    {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(fs::File::create(&unsafe_docx).unwrap());
        for name in ["[Content_Types].xml", "word/document.xml", "..\\escape.xml"] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"x").unwrap();
        }
        zip.finish().unwrap();
    }
    let prefix = fs::read(&unsafe_docx).unwrap();
    assert!(identify_file(&unsafe_docx, &prefix[..prefix.len().min(8192)]).is_err());
    let pdf = temp.path().join("paper.pdf");
    fs::write(&pdf, b"%PDF-1.7\n").unwrap();
    assert_eq!(
        identify_file(&pdf, b"%PDF-1.7\n").unwrap().1.mime,
        "application/pdf"
    );
    let markdown = temp.path().join("note.md");
    fs::write(&markdown, "你好").unwrap();
    assert_eq!(
        identify_file(&markdown, "你好".as_bytes()).unwrap().1.mime,
        "text/markdown"
    );
    let (legacy, _, had_errors) = encoding_rs::GB18030.encode("# 标题");
    assert!(!had_errors);
    let legacy_path = temp.path().join("legacy.md");
    fs::write(&legacy_path, &legacy).unwrap();
    assert_eq!(
        identify_file(&legacy_path, &legacy).unwrap().1.mime,
        "text/markdown"
    );
}

#[test]
fn repeated_append_drops_existing_and_in_batch_duplicates() {
    let mut session = ImportSession::new("s", "p", ImportResourceMode::Balanced);
    session.items.push(ImportItem::queued(
        "i",
        ImportInput {
            kind: ImportInputKind::File,
            display_name: "a.md".into(),
            locator: "C:/Input/A.md".into(),
            normalized_locator: Some("C:/Input/A.md".into()),
            source_identity: None,
            media_save_mode: Default::default(),
        },
    ));
    let file = llm_wiki_desktop_lib::models::import_v2_file::DiscoveredFile {
        source_path: "C:/Input/A.md".into(),
        relative_path: "A.md".into(),
        display_name: "A.md".into(),
        format: llm_wiki_desktop_lib::models::import_v2_file::FileFormat::Markdown,
        content_kind: llm_wiki_desktop_lib::models::import_v2_file::FileContentKind::Document,
        size_bytes: 1,
        identity: llm_wiki_desktop_lib::models::import_v2_file::FileIdentity {
            extension: "md".into(),
            magic: "utf-8".into(),
            mime: "text/markdown".into(),
            detection_method:
                llm_wiki_desktop_lib::models::import_v2_file::FileDetectionMethod::StructuredText,
            extension_mismatch: false,
        },
        source_identity: llm_wiki_desktop_lib::models::import_v2::SourceIdentity {
            canonical_path: "C:/Input/A.md".into(),
            size_bytes: 1,
            modified_nanos: None,
            file_id: None,
            sha256: "00".repeat(32),
            magic: "11".repeat(32),
        },
        large_data: None,
    };
    assert!(new_import_inputs(&session, [file.clone(), file]).is_empty());
}

#[test]
fn repeated_append_does_not_collapse_case_distinct_locators() {
    let mut session = ImportSession::new("s", "p", ImportResourceMode::Balanced);
    session.items.push(ImportItem::queued(
        "i",
        ImportInput {
            kind: ImportInputKind::File,
            display_name: "a.md".into(),
            locator: "C:/Input/a.md".into(),
            normalized_locator: Some("C:/Input/a.md".into()),
            source_identity: None,
            media_save_mode: Default::default(),
        },
    ));
    let file = llm_wiki_desktop_lib::models::import_v2_file::DiscoveredFile {
        source_path: "C:/Input/A.md".into(),
        relative_path: "A.md".into(),
        display_name: "A.md".into(),
        format: llm_wiki_desktop_lib::models::import_v2_file::FileFormat::Markdown,
        content_kind: llm_wiki_desktop_lib::models::import_v2_file::FileContentKind::Document,
        size_bytes: 1,
        identity: llm_wiki_desktop_lib::models::import_v2_file::FileIdentity {
            extension: "md".into(),
            magic: "utf-8".into(),
            mime: "text/markdown".into(),
            detection_method:
                llm_wiki_desktop_lib::models::import_v2_file::FileDetectionMethod::StructuredText,
            extension_mismatch: false,
        },
        source_identity: llm_wiki_desktop_lib::models::import_v2::SourceIdentity {
            canonical_path: "C:/Input/A.md".into(),
            size_bytes: 1,
            modified_nanos: None,
            file_id: None,
            sha256: "00".repeat(32),
            magic: "11".repeat(32),
        },
        large_data: None,
    };

    let inputs = new_import_inputs(&session, [file.clone(), file]);
    assert_eq!(inputs.len(), 1);
    assert_eq!(
        inputs[0].normalized_locator.as_deref(),
        Some("C:/Input/A.md")
    );
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
    let result = FileDiscoveryService::default()
        .scan(
            &context(&project),
            &[temp.path().join("loop")],
            FileScanPolicy::default(),
            |_| {},
            || false,
        )
        .unwrap();
    assert!(result.files.is_empty());
    assert_eq!(
        result.skipped[0].reason,
        FileSkipReason::SymlinkOrReparsePoint
    );
}
