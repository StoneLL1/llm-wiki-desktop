use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use llm_wiki_desktop_lib::errors::{
    IMPORT_V2_CAPABILITY_INVALID, IMPORT_V2_CAPABILITY_UNAVAILABLE,
};
use llm_wiki_desktop_lib::models::import_v2_file::CapabilityRequirement;
use llm_wiki_desktop_lib::services::import_v2::capability_pack::{
    CapabilityPackManager, CapabilityPackManifest, PackHealth,
};
use ring::signature::{Ed25519KeyPair, KeyPair};
use sha2::{Digest, Sha256};

const SEED: [u8; 32] = [7; 32];

struct Fixture {
    root: PathBuf,
    key: Ed25519KeyPair,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("llm-wiki-pack-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        Self {
            root,
            key: Ed25519KeyPair::from_seed_unchecked(&SEED).unwrap(),
        }
    }
    fn requirement(&self) -> CapabilityRequirement {
        CapabilityRequirement {
            capability_id: "document-standard".into(),
            minimum_version: Some("1.0.0".into()),
            protocol_version: "2".into(),
            target_triple: "x86_64-pc-windows-msvc".into(),
            accepted_license_expressions: vec!["MIT".into()],
        }
    }
    fn manager(&self) -> CapabilityPackManager {
        CapabilityPackManager::new(
            self.root.clone(),
            HashMap::from([(
                "release-2026".into(),
                self.key.public_key().as_ref().to_vec(),
            )]),
        )
    }
    fn install(&self, version: &str) -> PathBuf {
        let root = self.root.join("document-standard").join(version);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("runner.exe"), b"immutable pack").unwrap();
        let mut manifest = CapabilityPackManifest {
            schema_version: 1,
            pack_id: "document-standard".into(),
            version: version.into(),
            protocol_version: "2".into(),
            target_triples: vec!["x86_64-pc-windows-msvc".into()],
            archive_sha256: format!("{:x}", Sha256::digest(b"immutable pack")),
            license_expression: "MIT".into(),
            entrypoint: "runner.exe".into(),
            compressed_bytes: 14,
            installed_bytes: 14,
            signing_key_id: "release-2026".into(),
            signature: String::new(),
        };
        manifest.signature = hex(self.key.sign(&manifest.signing_payload().unwrap()).as_ref());
        fs::write(
            root.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        root
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn rewrite(path: &Path, change: impl FnOnce(&mut CapabilityPackManifest)) {
    let manifest_path = path.join("manifest.json");
    let mut manifest: CapabilityPackManifest =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    change(&mut manifest);
    fs::write(manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
}

#[test]
fn resolves_signed_hash_verified_compatible_pack() {
    let fixture = Fixture::new("valid");
    fixture.install("1.2.0");
    let pack = fixture.manager().resolve(&fixture.requirement()).unwrap();
    assert_eq!(pack.manifest.version, "1.2.0");
    assert!(pack.entrypoint.ends_with("runner.exe"));
}

#[test]
fn rejects_unsigned_tampered_hash_protocol_platform_license_and_traversal() {
    let cases: Vec<(&str, Box<dyn Fn(&mut CapabilityPackManifest)>)> = vec![
        ("unsigned", Box::new(|m| m.signature.clear())),
        ("hash", Box::new(|m| m.archive_sha256 = "00".repeat(32))),
        ("protocol", Box::new(|m| m.protocol_version = "3".into())),
        (
            "platform",
            Box::new(|m| m.target_triples = vec!["aarch64-apple-darwin".into()]),
        ),
        (
            "license",
            Box::new(|m| m.license_expression = "GPL-3.0".into()),
        ),
        (
            "traversal",
            Box::new(|m| m.entrypoint = "../runner.exe".into()),
        ),
    ];
    for (name, change) in cases {
        let fixture = Fixture::new(name);
        let root = fixture.install("1.0.0");
        rewrite(&root, change);
        assert_eq!(
            fixture
                .manager()
                .resolve(&fixture.requirement())
                .unwrap_err()
                .code,
            IMPORT_V2_CAPABILITY_INVALID,
            "case {name}"
        );
    }
}

#[test]
fn rejects_archive_bytes_changed_after_manifest_was_signed() {
    let fixture = Fixture::new("archive-tamper");
    let root = fixture.install("1.0.0");
    fs::write(root.join("runner.exe"), b"tampered pack").unwrap();
    assert_eq!(
        fixture
            .manager()
            .resolve(&fixture.requirement())
            .unwrap_err()
            .code,
        IMPORT_V2_CAPABILITY_INVALID
    );
}

#[test]
fn rolls_back_to_last_healthy_side_by_side_version() {
    let fixture = Fixture::new("rollback");
    fixture.install("1.0.0");
    fixture.install("2.0.0");
    let manager = fixture.manager();
    manager.mark_health("document-standard", "2.0.0", PackHealth::Unhealthy);
    assert_eq!(
        manager
            .resolve(&fixture.requirement())
            .unwrap()
            .manifest
            .version,
        "1.0.0"
    );
    manager.mark_health("document-standard", "1.0.0", PackHealth::Unhealthy);
    assert_eq!(
        manager.resolve(&fixture.requirement()).unwrap_err().code,
        IMPORT_V2_CAPABILITY_UNAVAILABLE
    );
}
