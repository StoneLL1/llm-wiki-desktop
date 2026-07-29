use std::fs;

use llm_wiki_desktop_lib::models::import_v2_file::FileFormat;
use llm_wiki_desktop_lib::services::import_v2::file_router::{
    CapabilitySnapshot, FileRoutePlanner,
};

#[test]
fn legacy_formats_route_through_isolated_conversion_then_qualified_fallback() {
    let capabilities = CapabilitySnapshot {
        office_legacy: true,
        office_oxide_installed: true,
        office_oxide_qualified: true,
        agent_available: true,
        ..Default::default()
    };
    for (format, modern) in [
        (FileFormat::Doc, "office.modern.docx"),
        (FileFormat::Xls, "office.modern.xlsx"),
        (FileFormat::Ppt, "office.modern.pptx"),
    ] {
        let routes = FileRoutePlanner::plan(format, capabilities);
        assert_eq!(routes[0].route, "pack.office-legacy");
        assert_eq!(routes[0].required_pack, Some("office-legacy"));
        assert_eq!(routes[1].route, modern);
        assert_eq!(routes[2].route, "pack.office-oxide");
        assert_eq!(routes[3].route, "agent.office");
    }
}

#[test]
fn unavailable_converter_never_skips_directly_to_an_unqualified_oxide_pack() {
    let routes = FileRoutePlanner::plan(
        FileFormat::Doc,
        CapabilitySnapshot {
            office_oxide_installed: true,
            office_oxide_qualified: false,
            agent_available: true,
            ..Default::default()
        },
    );
    assert_eq!(
        routes.iter().map(|r| r.route).collect::<Vec<_>>(),
        ["agent.office"]
    );
}

#[test]
fn manifest_and_runner_freeze_isolation_and_cache_contracts() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let manifest =
        fs::read_to_string(root.join("capabilities/office-legacy/manifest.json")).unwrap();
    assert!(manifest.contains("MPL-2.0 OR LGPL-3.0-or-later"));
    assert!(manifest.contains("runner/office-legacy-pack"));
    assert!(root
        .join("capabilities/office-legacy/runner/office-legacy-pack")
        .is_file());

    let runner =
        fs::read_to_string(root.join("capabilities/office-legacy/runner/office_legacy_pack.py"))
            .unwrap();
    for contract in [
        "shell=False",
        "profile.resolve().as_uri()",
        "\"-env:UserInstallation=\" + profile_uri(profile)",
        "MacroSecurityLevel",
        "DisablePlugins",
        "UpdateCheck",
        "start_new_session=True",
        "kill_process_tree",
        "[Content_Types].xml",
        "converted/",
        "source.bin",
        "LEGACY_OFFICE_OLE_MAY_BE_LOST",
        "LEGACY_OFFICE_ACTIVEX_REMOVED",
        "LEGACY_OFFICE_ANIMATION_MAY_BE_LOST",
    ] {
        assert!(
            runner.contains(contract),
            "missing runner contract: {contract}"
        );
    }
    assert!(!runner.contains("pip install"));
    assert!(!runner.contains("os.system"));
}

#[test]
fn modern_document_runner_consumes_only_a_staging_contained_chained_artifact() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let runner =
        fs::read_to_string(root.join("capabilities/document-standard/runner/markitdown_pack.py"))
            .unwrap();
    assert!(runner.contains("chainedInput"));
    assert!(runner.contains("contained(staging, source) if chained"));
    assert!(!runner.contains("pip install"));
}
