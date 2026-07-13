use std::{collections::HashMap, path::Path, sync::RwLock, time::Duration};

use serde::Serialize;

use crate::{errors::BackendError, models::import_v2_file::CapabilityRequirement};

use super::{
    capability_pack::{CapabilityPackManager, ResolvedCapabilityPack},
    file_router::CapabilitySnapshot,
    ImportV2Service,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityRuntimeStatus {
    pub capability_id: String,
    pub route: String,
    pub available: bool,
    pub reason: Option<String>,
}

#[derive(Default)]
pub struct ImportCapabilityRuntime {
    statuses: RwLock<Vec<CapabilityRuntimeStatus>>,
    browser_pack: RwLock<Option<ResolvedCapabilityPack>>,
}

impl ImportCapabilityRuntime {
    pub fn load_installed(&self, install_root: &Path, service: &ImportV2Service) {
        self.load_installed_with_keys(install_root, service, embedded_trusted_keys());
    }

    fn load_installed_with_keys(
        &self,
        install_root: &Path,
        service: &ImportV2Service,
        keys: HashMap<String, Vec<u8>>,
    ) {
        let manager = CapabilityPackManager::new(install_root.to_path_buf(), keys);
        let mut statuses = Vec::new();
        for spec in PACK_SPECS {
            let requirement = CapabilityRequirement {
                capability_id: spec.id.into(),
                minimum_version: None,
                protocol_version: "2".into(),
                target_triple: target_triple(),
                accepted_license_expressions: spec.licenses.iter().map(|v| (*v).into()).collect(),
            };
            let result = manager.resolve(&requirement).and_then(|pack| {
                let browser_pack = (spec.id == "browser-runtime").then(|| pack.clone());
                if spec.id == "office-oxide"
                    && !CapabilitySnapshot::from_installation(
                        false,
                        true,
                        &pack.root.join("qualification.json"),
                        &requirement.target_triple,
                        false,
                    )
                    .office_oxide_qualified
                {
                    return Err(BackendError::new(
                        crate::errors::IMPORT_V2_CAPABILITY_INVALID,
                        "The Office parser qualification evidence is missing or invalid.",
                        false,
                        false,
                    ));
                }
                service.register_capability_pack(
                    pack,
                    spec.route.into(),
                    spec.extensions.iter().map(|v| (*v).into()).collect(),
                    Duration::from_secs(180),
                )?;
                if let Some(pack) = browser_pack {
                    *self.browser_pack.write().map_err(|_| {
                        BackendError::new(
                            "IMPORT_V2_CAPABILITY_LOCKED",
                            "Capability runtime is unavailable.",
                            true,
                            false,
                        )
                    })? = Some(pack);
                }
                Ok(())
            });
            statuses.push(CapabilityRuntimeStatus {
                capability_id: spec.id.into(),
                route: spec.route.into(),
                available: result.is_ok(),
                reason: result.err().map(safe_reason),
            });
        }
        if let Ok(mut state) = self.statuses.write() {
            *state = statuses;
        }
    }

    pub fn statuses(&self) -> Vec<CapabilityRuntimeStatus> {
        self.statuses.read().map(|v| v.clone()).unwrap_or_default()
    }
    pub fn browser_pack(&self) -> Option<ResolvedCapabilityPack> {
        self.browser_pack
            .read()
            .ok()
            .and_then(|value| value.clone())
    }
}

struct PackSpec {
    id: &'static str,
    route: &'static str,
    extensions: &'static [&'static str],
    licenses: &'static [&'static str],
}
const PACK_SPECS: &[PackSpec] = &[
    PackSpec {
        id: "browser-runtime-lite",
        route: "web.generic.readability",
        extensions: &[],
        licenses: &["Apache-2.0 AND MIT"],
    },
    PackSpec {
        id: "browser-runtime",
        route: "web.generic.browser",
        extensions: &[],
        licenses: &["Apache-2.0"],
    },
    PackSpec {
        id: "browser-runtime-lite",
        route: "web.wechat.article",
        extensions: &[],
        licenses: &["Apache-2.0 AND MIT"],
    },
    PackSpec {
        id: "browser-runtime-lite",
        route: "web.zhihu.content",
        extensions: &[],
        licenses: &["Apache-2.0 AND MIT"],
    },
    PackSpec {
        id: "browser-runtime-lite",
        route: "web.bilibili.video",
        extensions: &[],
        licenses: &["Apache-2.0 AND MIT"],
    },
    PackSpec {
        id: "media-metadata",
        route: "web.bilibili.metadata",
        extensions: &[],
        licenses: &["MIT"],
    },
    PackSpec {
        id: "document-standard",
        route: "pack.markitdown",
        extensions: &["docx", "xlsx", "pptx", "pdf"],
        licenses: &["MIT"],
    },
    PackSpec {
        id: "document-layout",
        route: "pdf.layout",
        extensions: &["pdf"],
        licenses: &["MIT"],
    },
    PackSpec {
        id: "office-legacy",
        route: "pack.office-legacy",
        extensions: &["doc", "xls", "ppt"],
        licenses: &["MPL-2.0 OR LGPL-3.0-or-later"],
    },
    PackSpec {
        id: "office-oxide",
        route: "pack.office-oxide",
        extensions: &["doc", "docx", "xls", "xlsx", "ppt", "pptx"],
        licenses: &["MIT OR Apache-2.0"],
    },
    PackSpec {
        id: "ocr-basic",
        route: "ocr.basic",
        extensions: &["pdf"],
        licenses: &["Apache-2.0 AND BSD-2-Clause"],
    },
    PackSpec {
        id: "ocr-cjk-accurate",
        route: "ocr.cjk-accurate",
        extensions: &["pdf"],
        licenses: &["Apache-2.0"],
    },
    PackSpec {
        id: "media-runtime",
        route: "media.subtitle",
        extensions: &["srt", "vtt", "lrc", "ass", "ssa"],
        licenses: &["LGPL-2.1-or-later"],
    },
    PackSpec {
        id: "asr-whisper",
        route: "media.asr",
        extensions: &["mp3", "wav", "m4a", "mp4", "mov", "mkv"],
        licenses: &["MIT AND LGPL-2.1-or-later"],
    },
];

fn embedded_trusted_keys() -> HashMap<String, Vec<u8>> {
    let encoded: HashMap<String, String> =
        serde_json::from_str(include_str!("../../../../capabilities/trusted-keys.json"))
            .unwrap_or_default();
    encoded
        .into_iter()
        .filter_map(|(id, hex)| decode_hex(&hex).map(|key| (id, key)))
        .collect()
}
fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if value.len() % 2 != 0 {
        return None;
    }
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).ok())
        .collect()
}
fn target_triple() -> String {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "windows") => "x86_64-pc-windows-msvc",
        ("aarch64", "macos") => "aarch64-apple-darwin",
        ("x86_64", "macos") => "x86_64-apple-darwin",
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        _ => "unsupported-target",
    }
    .into()
}
fn safe_reason(error: BackendError) -> String {
    error.code
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::import_v2::capability_pack::CapabilityPackManifest;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use sha2::{Digest, Sha256};

    #[test]
    fn empty_embedded_trust_store_fails_closed_without_disabling_native_imports() {
        let root = std::env::temp_dir().join(format!("cap-runtime-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let service = ImportV2Service::default();
        let runtime = ImportCapabilityRuntime::default();
        runtime.load_installed(&root, &service);
        assert!(runtime.statuses().iter().all(|status| !status.available));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn signed_installed_pack_is_registered_but_untrusted_placeholder_is_not() {
        let root =
            std::env::temp_dir().join(format!("cap-runtime-signed-{}", uuid::Uuid::new_v4()));
        let pack_root = root.join("document-standard/1.2.0");
        std::fs::create_dir_all(&pack_root).unwrap();
        std::fs::write(pack_root.join("runner.bin"), b"verified runtime").unwrap();
        let key = Ed25519KeyPair::from_seed_unchecked(&[9; 32]).unwrap();
        let mut manifest = CapabilityPackManifest {
            schema_version: 1,
            pack_id: "document-standard".into(),
            version: "1.2.0".into(),
            protocol_version: "2".into(),
            target_triples: vec![target_triple()],
            archive_sha256: format!("{:x}", Sha256::digest(b"verified runtime")),
            license_expression: "MIT".into(),
            entrypoint: "runner.bin".into(),
            compressed_bytes: 16,
            installed_bytes: 16,
            signing_key_id: "release-test".into(),
            signature: String::new(),
            files: vec![],
        };
        manifest.signature = key
            .sign(&manifest.signing_payload().unwrap())
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        std::fs::write(
            pack_root.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let service = ImportV2Service::default();
        let runtime = ImportCapabilityRuntime::default();
        runtime.load_installed_with_keys(
            &root,
            &service,
            HashMap::from([("release-test".into(), key.public_key().as_ref().to_vec())]),
        );
        let status = runtime
            .statuses()
            .into_iter()
            .find(|status| status.capability_id == "document-standard")
            .unwrap();
        assert!(status.available);
        assert!(service
            .registered_engine_routes()
            .unwrap()
            .iter()
            .any(|route| route == "pack.markitdown"));
        std::fs::remove_dir_all(root).ok();
    }
}
