use std::sync::atomic::AtomicBool;

use llm_wiki_desktop_lib::models::project::{ProjectFormat, ProjectHealth};
use llm_wiki_desktop_lib::services::assess_project_folder;

fn assess_missing(required_path: &str) {
    let root = tempfile::tempdir().unwrap();
    for directory in [".app", "raw", "raw/sources", "wiki", "exports", "skills"] {
        if directory != required_path {
            std::fs::create_dir_all(root.path().join(directory)).unwrap();
        }
    }
    std::fs::write(root.path().join("purpose.md"), "# Purpose\n").unwrap();
    std::fs::write(root.path().join("schema.md"), "# Schema\n").unwrap();
    std::fs::write(root.path().join("wiki/legacy-page.md"), "# Legacy\n").unwrap();
    let config = tempfile::tempdir().unwrap();

    let assessment = assess_project_folder(
        root.path().to_string_lossy().as_ref(),
        config.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(assessment.format, ProjectFormat::NativeLegacy, "{required_path}");
    assert_eq!(assessment.health, ProjectHealth::Repairable, "{required_path}");
    assert!(!assessment.repair_available, "{required_path}");
}

#[test]
fn expected_red_legacy_required_directory_gaps_have_no_supported_repair() {
    assess_missing("raw/sources");
    assess_missing(".app/tasks");
}
