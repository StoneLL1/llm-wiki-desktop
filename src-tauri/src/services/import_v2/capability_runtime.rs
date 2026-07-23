use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::RwLock,
    time::Duration,
};

use serde::Serialize;

use crate::{errors::BackendError, models::import_v2_file::CapabilityRequirement};

use super::{
    capability_pack::{CapabilityPackManager, ResolvedCapabilityPack},
    file_router::CapabilitySnapshot,
    ImportV2Service,
};

const STARTUP_LOADER_STACK_BYTES: usize = 8 * 1024 * 1024;

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
    install_root: RwLock<Option<PathBuf>>,
    #[cfg(debug_assertions)]
    development: RwLock<Option<(PathBuf, Vec<u8>)>>,
}

impl ImportCapabilityRuntime {
    /// Capability manifests and file inventories are verified off the Windows
    /// GUI main thread, whose PE stack reserve is only 1 MiB in development.
    /// The caller still waits for completion so readiness is deterministic
    /// before the first frontend request can arrive.
    pub fn load_startup(
        &self,
        install_root: &Path,
        development: Option<(&Path, &Path)>,
        service: &ImportV2Service,
    ) -> Result<bool, BackendError> {
        run_startup_loader(move || {
            self.load_installed(install_root, service);
            #[cfg(debug_assertions)]
            {
                return development.is_some_and(|(development_root, public_key_path)| {
                    self.load_development(development_root, public_key_path, service)
                });
            }
            #[cfg(not(debug_assertions))]
            {
                let _ = development;
                false
            }
        })
    }

    pub fn load_installed(&self, install_root: &Path, service: &ImportV2Service) {
        if let Ok(mut root) = self.install_root.write() {
            *root = Some(install_root.to_path_buf());
        }
        self.load_installed_with_keys(install_root, service, embedded_trusted_keys(), false);
        #[cfg(debug_assertions)]
        self.apply_development_overlay(service);
    }

    #[cfg(debug_assertions)]
    pub fn load_development(
        &self,
        install_root: &Path,
        public_key_path: &Path,
        service: &ImportV2Service,
    ) -> bool {
        let key = std::fs::read_to_string(public_key_path)
            .ok()
            .and_then(|value| decode_hex(value.trim()));
        let Some(key) = key.filter(|value| value.len() == 32) else {
            return false;
        };
        if let Ok(mut development) = self.development.write() {
            *development = Some((install_root.to_path_buf(), key));
        }
        self.apply_development_overlay(service);
        self.statuses().iter().any(|status| {
            status.capability_id == "asr-sensevoice-small"
                && status.route == "media.asr"
                && status.available
        })
    }

    #[cfg(debug_assertions)]
    fn apply_development_overlay(&self, service: &ImportV2Service) {
        let development = self.development.read().ok().and_then(|value| value.clone());
        let Some((install_root, key)) = development else {
            return;
        };
        let production = self.statuses();
        self.load_installed_with_keys(
            &install_root,
            service,
            HashMap::from([("development-local".into(), key)]),
            true,
        );
        let merged = merge_development_statuses(production, self.statuses());
        if let Ok(mut statuses) = self.statuses.write() {
            *statuses = merged;
        }
    }

    fn load_installed_with_keys(
        &self,
        install_root: &Path,
        service: &ImportV2Service,
        keys: HashMap<String, Vec<u8>>,
        replace_existing: bool,
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
                let route = spec.route.into();
                let extensions = spec.extensions.iter().map(|v| (*v).into()).collect();
                let timeout = Duration::from_secs(spec.timeout_seconds);
                if replace_existing {
                    service.replace_capability_pack(pack, route, extensions, timeout)?;
                } else {
                    service.register_capability_pack(pack, route, extensions, timeout)?;
                }
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
    pub fn install_root(&self) -> Option<PathBuf> {
        self.install_root.read().ok().and_then(|root| root.clone())
    }
}

fn run_startup_loader<T, F>(work: F) -> Result<T, BackendError>
where
    T: Send,
    F: FnOnce() -> T + Send,
{
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("import-capability-loader".into())
            .stack_size(STARTUP_LOADER_STACK_BYTES)
            .spawn_scoped(scope, work)
            .map_err(|error| startup_loader_error("could not be started", error.to_string()))?;
        worker.join().map_err(|payload| {
            startup_loader_error("panicked", panic_payload_message(payload.as_ref()))
        })
    })
}

fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|message| (*message).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "non-string panic payload".into())
}

fn startup_loader_error(message: &str, detail: String) -> BackendError {
    BackendError::new(
        "IMPORT_V2_CAPABILITY_LOADER_FAILED",
        format!("The import capability loader {message}."),
        true,
        false,
    )
    .with_details(serde_json::json!({ "error": detail }))
}

#[cfg(debug_assertions)]
fn merge_development_statuses(
    production: Vec<CapabilityRuntimeStatus>,
    development: Vec<CapabilityRuntimeStatus>,
) -> Vec<CapabilityRuntimeStatus> {
    let mut merged = production
        .into_iter()
        .map(|status| ((status.capability_id.clone(), status.route.clone()), status))
        .collect::<HashMap<_, _>>();
    for status in development {
        let key = (status.capability_id.clone(), status.route.clone());
        if status.available || !merged.contains_key(&key) {
            merged.insert(key, status);
        }
    }
    let mut statuses = merged.into_values().collect::<Vec<_>>();
    statuses.sort_by(|left, right| {
        left.capability_id
            .cmp(&right.capability_id)
            .then_with(|| left.route.cmp(&right.route))
    });
    statuses
}

struct PackSpec {
    id: &'static str,
    route: &'static str,
    extensions: &'static [&'static str],
    licenses: &'static [&'static str],
    timeout_seconds: u64,
}
const DEFAULT_PACK_TIMEOUT_SECONDS: u64 = 180;
const OCR_PACK_TIMEOUT_SECONDS: u64 = 15 * 60;
const ASR_PACK_TIMEOUT_SECONDS: u64 = (30 * 60) + (2 * 60 * 60) + (5 * 60);
const BROWSER_BUNDLE_LICENSE: &str = "Apache-2.0 AND MIT AND BSD-2-Clause AND BSD-3-Clause AND ISC AND MIT-0 AND LicenseRef-Bundled-Third-Party-Notices";
const PACK_SPECS: &[PackSpec] = &[
    PackSpec {
        id: "browser-runtime-lite",
        route: "web.generic.readability",
        extensions: &[],
        licenses: &[BROWSER_BUNDLE_LICENSE],
        timeout_seconds: DEFAULT_PACK_TIMEOUT_SECONDS,
    },
    PackSpec {
        id: "browser-runtime",
        route: "web.generic.browser",
        extensions: &[],
        licenses: &[BROWSER_BUNDLE_LICENSE],
        timeout_seconds: DEFAULT_PACK_TIMEOUT_SECONDS,
    },
    PackSpec {
        id: "browser-runtime-lite",
        route: "web.wechat.article",
        extensions: &[],
        licenses: &[BROWSER_BUNDLE_LICENSE],
        timeout_seconds: DEFAULT_PACK_TIMEOUT_SECONDS,
    },
    PackSpec {
        id: "browser-runtime",
        route: "web.wechat.article",
        extensions: &[],
        licenses: &[BROWSER_BUNDLE_LICENSE],
        timeout_seconds: DEFAULT_PACK_TIMEOUT_SECONDS,
    },
    PackSpec {
        id: "browser-runtime-lite",
        route: "web.zhihu.content",
        extensions: &[],
        licenses: &[BROWSER_BUNDLE_LICENSE],
        timeout_seconds: DEFAULT_PACK_TIMEOUT_SECONDS,
    },
    PackSpec {
        id: "media-metadata",
        route: "web.bilibili.metadata",
        extensions: &[],
        licenses: &["MIT"],
        timeout_seconds: DEFAULT_PACK_TIMEOUT_SECONDS,
    },
    PackSpec {
        id: "document-standard",
        route: "pack.markitdown",
        extensions: &["docx", "xlsx", "pptx", "pdf"],
        licenses: &["MIT"],
        timeout_seconds: DEFAULT_PACK_TIMEOUT_SECONDS,
    },
    PackSpec {
        id: "document-layout",
        route: "pdf.layout",
        extensions: &["pdf"],
        licenses: &["MIT"],
        timeout_seconds: DEFAULT_PACK_TIMEOUT_SECONDS,
    },
    PackSpec {
        id: "office-legacy",
        route: "pack.office-legacy",
        extensions: &["doc", "xls", "ppt"],
        licenses: &["MPL-2.0 OR LGPL-3.0-or-later"],
        timeout_seconds: DEFAULT_PACK_TIMEOUT_SECONDS,
    },
    PackSpec {
        id: "office-oxide",
        route: "pack.office-oxide",
        extensions: &["doc", "docx", "xls", "xlsx", "ppt", "pptx"],
        licenses: &["MIT OR Apache-2.0"],
        timeout_seconds: DEFAULT_PACK_TIMEOUT_SECONDS,
    },
    PackSpec {
        id: "ocr-basic",
        route: "ocr.basic",
        extensions: &["pdf", "avif", "gif", "jpeg", "jpg", "png", "tiff", "webp"],
        licenses: &["Apache-2.0 AND BSD-2-Clause"],
        timeout_seconds: OCR_PACK_TIMEOUT_SECONDS,
    },
    PackSpec {
        id: "ocr-cjk-accurate",
        route: "ocr.cjk-accurate",
        extensions: &["bmp", "jpeg", "jpg", "png", "tif", "tiff", "webp"],
        licenses: &["Apache-2.0 AND MIT AND BSD-3-Clause AND HPND AND MPL-2.0 AND PSF-2.0 AND LGPL-2.1-only AND LGPL-3.0-only"],
        timeout_seconds: OCR_PACK_TIMEOUT_SECONDS,
    },
    PackSpec {
        id: "media-runtime",
        route: "media.subtitle",
        extensions: &["srt", "vtt", "lrc", "ass", "ssa"],
        licenses: &["LGPL-2.1-or-later"],
        timeout_seconds: DEFAULT_PACK_TIMEOUT_SECONDS,
    },
    PackSpec {
        id: "asr-sensevoice-small",
        route: "media.asr",
        extensions: &[
            "aac", "flac", "m4a", "mka", "mp3", "ogg", "opus", "wav", "avi", "m4v",
            "mkv", "mov", "mp4", "mpeg", "mpg", "webm",
        ],
        licenses: &["Apache-2.0 AND LGPL-3.0-or-later AND MIT"],
        timeout_seconds: ASR_PACK_TIMEOUT_SECONDS,
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
pub fn target_triple() -> String {
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

    #[test]
    fn startup_loader_uses_the_named_large_stack_worker() {
        let thread_name =
            run_startup_loader(|| std::thread::current().name().map(str::to_owned)).unwrap();
        assert_eq!(thread_name.as_deref(), Some("import-capability-loader"));
        assert_eq!(STARTUP_LOADER_STACK_BYTES, 8 * 1024 * 1024);
    }

    #[test]
    #[cfg(debug_assertions)]
    fn development_overlay_only_replaces_routes_that_are_ready() {
        let status = |id: &str, route: &str, available: bool| CapabilityRuntimeStatus {
            capability_id: id.into(),
            route: route.into(),
            available,
            reason: (!available).then(|| "missing".into()),
        };
        let merged = merge_development_statuses(
            vec![
                status("browser-runtime", "web.generic.browser", true),
                status("asr-sensevoice-small", "media.asr", false),
            ],
            vec![
                status("browser-runtime", "web.generic.browser", false),
                status("asr-sensevoice-small", "media.asr", true),
            ],
        );
        assert!(merged
            .iter()
            .any(|entry| { entry.route == "web.generic.browser" && entry.available }));
        assert!(merged
            .iter()
            .any(|entry| entry.route == "media.asr" && entry.available));
    }
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
    fn production_pack_timeouts_and_formats_match_long_running_runners() {
        let asr = PACK_SPECS
            .iter()
            .find(|spec| spec.id == "asr-sensevoice-small")
            .unwrap();
        assert!(asr.timeout_seconds > (30 * 60) + (2 * 60 * 60));
        assert!(asr.extensions.contains(&"webm"));
        assert!(asr.extensions.contains(&"opus"));

        let ocr = PACK_SPECS
            .iter()
            .find(|spec| spec.id == "ocr-cjk-accurate")
            .unwrap();
        assert_eq!(
            ocr.extensions,
            &["bmp", "jpeg", "jpg", "png", "tif", "tiff", "webp"]
        );
        assert!(ocr.timeout_seconds >= 15 * 60);
    }

    #[test]
    #[ignore = "requires scripts/prepare-sensevoice-dev.mjs"]
    fn prepared_development_sensevoice_pack_passes_runtime_integrity() {
        let development_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.dev-capabilities");
        let service = ImportV2Service::default();
        let runtime = ImportCapabilityRuntime::default();
        let production_root = std::env::temp_dir().join(format!(
            "cap-runtime-production-root-{}",
            uuid::Uuid::new_v4()
        ));
        assert!(runtime
            .load_startup(
                &production_root,
                Some((
                    &development_root.join("installed"),
                    &development_root.join("development-public-key.hex"),
                )),
                &service,
            )
            .unwrap());
        assert_eq!(
            runtime.install_root().as_deref(),
            Some(production_root.as_path())
        );
        if !runtime
            .statuses()
            .iter()
            .any(|status| status.route == "media.asr" && status.available)
        {
            let key = decode_hex(
                std::fs::read_to_string(development_root.join("development-public-key.hex"))
                    .unwrap()
                    .trim(),
            )
            .unwrap();
            let manager = CapabilityPackManager::new(
                development_root.join("installed"),
                HashMap::from([("development-local".into(), key)]),
            );
            let error = manager
                .resolve(&CapabilityRequirement {
                    capability_id: "asr-sensevoice-small".into(),
                    minimum_version: None,
                    protocol_version: "2".into(),
                    target_triple: target_triple(),
                    accepted_license_expressions: vec![
                        "Apache-2.0 AND LGPL-3.0-or-later AND MIT".into()
                    ],
                })
                .unwrap_err();
            panic!(
                "development pack validation failed: {}: {}",
                error.code, error.message
            );
        }
        assert!(service
            .registered_engine_routes()
            .unwrap()
            .iter()
            .any(|route| route == "media.asr"));
        std::fs::remove_dir_all(production_root).ok();
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
            entrypoint_args: Vec::new(),
            executable_files: Vec::new(),
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
            false,
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
