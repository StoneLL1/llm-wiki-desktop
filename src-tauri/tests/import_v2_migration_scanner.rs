use std::fs;
use std::path::Path;

use llm_wiki_desktop_lib::services::import_v2::migration::{
    DefaultLegacyScanner, LegacyScanner, ScannerLimits,
};
use tempfile::tempdir;

fn snapshot(root: &Path) -> Vec<(String, Vec<u8>, Option<std::time::SystemTime>)> {
    fn visit(root: &Path, path: &Path, output: &mut Vec<(String, Vec<u8>, Option<std::time::SystemTime>)>) {
        let Ok(entries) = fs::read_dir(path) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let metadata = fs::symlink_metadata(&path).unwrap();
            let bytes = if metadata.is_file() { fs::read(&path).unwrap() } else { Vec::new() };
            output.push((relative, bytes, metadata.modified().ok()));
            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                visit(root, &path, output);
            }
        }
    }
    let mut output = Vec::new();
    visit(root, root, &mut output);
    output.sort_by(|left, right| left.0.cmp(&right.0));
    output
}

#[test]
fn scanner_is_read_only_and_fingerprints_metadata_deterministically() {
    let directory = tempdir().unwrap();
    let root = directory.path();
    fs::create_dir_all(root.join(".app/import-history")).unwrap();
    fs::create_dir_all(root.join(".app/tasks")).unwrap();
    fs::create_dir_all(root.join("raw/sources")).unwrap();
    fs::create_dir_all(root.join("wiki")).unwrap();
    fs::write(
        root.join(".app/source-index.json"),
        r#"{"schemaVersion":1,"sources":{"raw/sources/资料.md":["wiki/资料.md"]}}"#,
    )
    .unwrap();
    fs::write(root.join(".app/import-history/old.json"), r#"{"recordId":"old"}"#).unwrap();
    fs::write(root.join(".app/tasks/old.log"), "legacy task output").unwrap();
    fs::write(root.join("raw/sources/资料.md"), "source").unwrap();
    fs::write(root.join("wiki/资料.md"), "wiki").unwrap();

    let before = snapshot(root);
    let scanner = DefaultLegacyScanner::default();
    let first = scanner.scan(root).unwrap();
    let second = scanner.scan(root).unwrap();
    assert_eq!(first.fingerprint, second.fingerprint);
    assert!(!first.records.is_empty());
    assert!(first.scanned_files.iter().any(|file| file.relative_path == "raw/sources/资料.md"));
    assert_eq!(before, snapshot(root));
    assert!(!root.join(".app/import-v2-migration").exists());
}

#[test]
fn scanner_warns_on_corrupt_metadata_and_does_not_follow_links() {
    let directory = tempdir().unwrap();
    let root = directory.path();
    fs::create_dir_all(root.join(".app/import-history")).unwrap();
    fs::write(root.join(".app/source-index.json"), b"{broken").unwrap();
    fs::write(root.join(".app/import-history/broken.json"), b"{also-broken").unwrap();

    let outside = tempdir().unwrap();
    fs::write(outside.path().join("secret.md"), "must not be read").unwrap();
    let link = root.join("raw-link");
    #[cfg(windows)]
    let link_result = std::os::windows::fs::symlink_file(outside.path().join("secret.md"), &link);
    #[cfg(unix)]
    let link_result = std::os::unix::fs::symlink(outside.path().join("secret.md"), &link);
    if link_result.is_ok() {
        let inventory = DefaultLegacyScanner::default().scan(root).unwrap();
        assert!(inventory.scanned_files.iter().all(|file| file.relative_path != "raw-link"));
        assert!(inventory.warnings.iter().any(|warning| warning.code == "MIGRATION_SYMLINK_SKIPPED"));
    }
}

#[test]
fn scanner_enforces_file_and_metadata_limits_with_typed_warnings() {
    let directory = tempdir().unwrap();
    let root = directory.path();
    fs::create_dir_all(root.join(".app")).unwrap();
    fs::write(root.join(".app/source-index.json"), "{}".repeat(10)).unwrap();
    let scanner = DefaultLegacyScanner::new(ScannerLimits {
        max_files: 1,
        max_metadata_bytes: 4,
    });
    let inventory = scanner.scan(root).unwrap();
    assert!(inventory.warnings.iter().any(|warning| warning.code == "MIGRATION_SCAN_LIMIT"));
}
