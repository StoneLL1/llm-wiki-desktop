use sha2::Digest;
use std::fs;

use llm_wiki_desktop_lib::services::import_v2::ocr_router::{
    character_accuracy, non_empty_table_cell_accuracy, OcrBlock, OcrCoordinates, OcrLayoutHint,
    OcrPackAvailability, OcrPackMetadata, OcrPageResult, OcrQualityThresholds, OcrRequest,
    OcrRoute, OcrRouter, OCR_PREPROCESS_VERSION,
};
use tempfile::tempdir;

fn request(languages: &[&str], hints: Vec<OcrLayoutHint>) -> OcrRequest {
    OcrRequest {
        page_image_paths: vec!["item-staging/pages/0001.png".into()],
        languages: languages.iter().map(|value| (*value).into()).collect(),
        layout_hints: hints,
        thresholds: OcrQualityThresholds {
            minimum_character_accuracy: 0.95,
            minimum_table_cell_accuracy: 0.95,
            minimum_block_confidence: 0.75,
        },
    }
}

fn pack(id: &str, languages: &[&str], available: bool) -> OcrPackMetadata {
    OcrPackMetadata {
        pack_id: id.into(),
        engine_version: if id == "ocr-basic" { "5.4.1" } else { "3.0.0" }.into(),
        model_version: "2024.11".into(),
        model_sha256: "a".repeat(64),
        languages: languages.iter().map(|value| (*value).into()).collect(),
        license_expression: "Apache-2.0".into(),
        download_bytes: 128_000_000,
        installed_bytes: 256_000_000,
        availability: if available {
            OcrPackAvailability::Installed
        } else {
            OcrPackAvailability::NotInstalled
        },
    }
}

#[test]
fn routes_english_to_basic_and_cjk_tables_to_accurate() {
    let router = OcrRouter::new(
        pack("ocr-basic", &["eng", "chi_sim", "chi_tra"], true),
        pack("ocr-cjk-accurate", &["chi_sim", "chi_tra"], true),
    );
    assert_eq!(
        router.plan(&request(&["eng"], vec![])).route,
        OcrRoute::Basic
    );
    assert_eq!(
        router
            .plan(&request(&["chi_sim"], vec![OcrLayoutHint::Table]))
            .route,
        OcrRoute::CjkAccurate
    );
}

#[test]
fn missing_accurate_pack_is_honest_waiting_capability() {
    let router = OcrRouter::new(
        pack("ocr-basic", &["eng"], true),
        pack("ocr-cjk-accurate", &["chi_sim", "chi_tra"], false),
    );
    let plan = router.plan(&request(&["chi_tra"], vec![OcrLayoutHint::Table]));
    assert_eq!(plan.route, OcrRoute::WaitingCapability);
    let requirement = plan.requirement.expect("download requirement");
    assert_eq!(requirement.capability_id, "ocr-cjk-accurate");
    assert_eq!(requirement.license_expression, "Apache-2.0");
    assert!(requirement.download_bytes > 0);
    assert!(requirement.required_disk_bytes >= requirement.installed_bytes);
    assert!(!plan.cloud_fallback);
}

#[test]
fn cache_key_covers_every_reuse_boundary() {
    let base = request(&["eng", "chi_sim"], vec![]);
    let key = base.cache_key("input", "5.4.1", "models");
    assert_ne!(key, base.cache_key("changed", "5.4.1", "models"));
    assert_ne!(key, base.cache_key("input", "5.4.2", "models"));
    assert_ne!(key, base.cache_key("input", "5.4.1", "changed"));
    let mut changed_language = base.clone();
    changed_language.languages = vec!["eng".into()];
    assert_ne!(key, changed_language.cache_key("input", "5.4.1", "models"));
    assert!(base
        .cache_material("input", "5.4.1", "models")
        .contains(OCR_PREPROCESS_VERSION));
}

#[test]
fn validates_pinned_model_hash_and_language_availability() {
    let directory = tempdir().unwrap();
    let model = directory.path().join("eng.traineddata");
    fs::write(&model, b"pinned-model").unwrap();
    let hash = format!("{:x}", sha2::Sha256::digest(b"pinned-model"));
    assert!(
        llm_wiki_desktop_lib::services::import_v2::ocr_router::validate_model(&model, &hash)
            .is_ok()
    );
    assert!(
        llm_wiki_desktop_lib::services::import_v2::ocr_router::validate_model(
            &model,
            &"0".repeat(64)
        )
        .is_err()
    );
}

#[test]
fn quality_contract_is_measured_without_claiming_missing_real_models() {
    let result = OcrPageResult {
        page_index: 0,
        blocks: vec![OcrBlock {
            text: "English 中文".into(),
            confidence: 0.98,
            coordinates: OcrCoordinates {
                x: 1,
                y: 2,
                width: 30,
                height: 12,
            },
            table_cell: Some(OcrCoordinates {
                x: 0,
                y: 0,
                width: 40,
                height: 20,
            }),
        }],
        confidence: 0.98,
        engine_id: "tesseract".into(),
        engine_version: "5.4.1".into(),
        model_version: "2024.11".into(),
        model_sha256: "a".repeat(64),
    };
    assert_eq!(result.blocks[0].coordinates.width, 30);
    assert!(result.meets_confidence_threshold(&request(&["eng"], vec![]).thresholds));
    assert!(character_accuracy("English 中文", "English 中文") >= 0.95);
    assert!(non_empty_table_cell_accuracy(&["A", "中"], &["A", "中"]) >= 0.95);
}

#[test]
fn page_images_are_confined_to_item_staging() {
    let directory = tempdir().unwrap();
    fs::create_dir(directory.path().join("pages")).unwrap();
    fs::write(directory.path().join("pages/0001.png"), b"png").unwrap();
    let mut value = request(&["eng"], vec![]);
    value.page_image_paths = vec!["pages/0001.png".into()];
    assert!(value.validate_staging_paths(directory.path()).is_ok());
    value.page_image_paths = vec!["../outside.png".into()];
    assert!(value.validate_staging_paths(directory.path()).is_err());
}

#[test]
fn source_manifests_never_claim_unbuilt_payloads() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("capabilities");
    let basic: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("ocr-basic/manifest.json")).unwrap()).unwrap();
    let accurate: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("ocr-cjk-accurate/manifest.json")).unwrap())
            .unwrap();
    assert_eq!(basic["engine"]["name"], "Tesseract");
    assert_eq!(basic["licenseExpression"], "Apache-2.0 AND BSD-2-Clause");
    assert_eq!(basic["payloadStatus"], "release_ci_required");
    assert_eq!(accurate["engine"]["name"], "RapidOCR with ONNX Runtime");
    assert_eq!(accurate["engine"]["version"], "3.8.1");
    assert_eq!(
        accurate["licenseExpression"],
        "Apache-2.0 AND MIT AND BSD-3-Clause AND HPND AND MPL-2.0 AND PSF-2.0 AND LGPL-2.1-only AND LGPL-3.0-only"
    );
    assert_eq!(accurate["targetTriples"].as_array().unwrap().len(), 0);
    assert_eq!(accurate["signature"], "");
    assert_eq!(accurate["payloadStatus"], "release_ci_required");
}
