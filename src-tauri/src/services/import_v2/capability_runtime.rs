use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::RwLock,
    time::Duration,
};

use serde::{Deserialize, Serialize};

use crate::tasks::task_model::CancellationToken;
use crate::{errors::BackendError, models::import_v2_file::CapabilityRequirement};

use super::{
    capability_installer::CapabilityInstallRecovery,
    capability_pack::{CapabilityPackManager, ResolvedCapabilityPack},
    file_router::CapabilitySnapshot,
    product_capability::ProductCapabilityManifest,
    ImportV2Service,
};

const STARTUP_LOADER_STACK_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityRuntimeStatus {
    pub capability_id: String,
    pub route: String,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub healthy_version: Option<String>,
    pub reason: Option<String>,
}

pub struct ProbedCapabilityVersion {
    pack: ResolvedCapabilityPack,
    routes: Vec<CapabilityRouteSpec>,
}

#[derive(Clone)]
struct CapabilityRouteSpec {
    route: String,
    extensions: Vec<String>,
    timeout: Duration,
}

#[derive(Default)]
pub struct ImportCapabilityRuntime {
    statuses: RwLock<Vec<CapabilityRuntimeStatus>>,
    browser_pack: RwLock<Option<ResolvedCapabilityPack>>,
    install_root: RwLock<Option<PathBuf>>,
    startup_recovery: RwLock<CapabilityInstallRecovery>,
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
            let recovery = super::capability_installer::recover_install_root(install_root)?;
            if let Ok(mut startup_recovery) = self.startup_recovery.write() {
                *startup_recovery = recovery;
            }
            self.load_installed(install_root, service);
            #[cfg(debug_assertions)]
            {
                return Ok(
                    development.is_some_and(|(development_root, public_key_path)| {
                        self.load_development(development_root, public_key_path, service)
                    }),
                );
            }
            #[cfg(not(debug_assertions))]
            {
                let _ = development;
                Ok(false)
            }
        })?
    }

    pub fn startup_recovery(&self) -> CapabilityInstallRecovery {
        self.startup_recovery
            .read()
            .map(|recovery| *recovery)
            .unwrap_or_default()
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
        let manager = CapabilityPackManager::for_installed(install_root.to_path_buf(), keys);
        let mut statuses = Vec::new();
        // One verified snapshot per pack for this startup pass. Routes share
        // the same files; failures are also shared, and nothing survives reload.
        let mut resolved_packs = HashMap::new();
        for spec in PACK_SPECS {
            let requirement = CapabilityRequirement {
                capability_id: spec.id.into(),
                minimum_version: None,
                protocol_version: "2".into(),
                target_triple: target_triple(),
                accepted_license_expressions: spec.licenses.iter().map(|v| (*v).into()).collect(),
            };
            let resolved = resolved_packs.entry(spec.id).or_insert_with(|| {
                let pack = manager.resolve(&requirement)?;
                validate_signed_product_contract(&pack)?;
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
                Ok::<_, BackendError>(pack)
            });
            let result = resolved.clone().and_then(|pack| {
                if spec.route == "media.embedded-subtitle"
                    && !declares_embedded_subtitle_decoder(&pack)
                {
                    return Err(capability_route_contract_error());
                }
                let healthy_version = pack.manifest.version.clone();
                let browser_pack = (spec.id == "browser-runtime").then(|| pack.clone());
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
                Ok(healthy_version)
            });
            let healthy_version = result.as_ref().ok().cloned();
            statuses.push(CapabilityRuntimeStatus {
                capability_id: spec.id.into(),
                route: spec.route.into(),
                available: result.is_ok(),
                healthy_version,
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

    /// Start the runtime once before activation. Full route qualification belongs
    /// to resource builds; installation checks that the program runs on this host.
    pub fn probe_version_routes(
        &self,
        install_root: &Path,
        capability_id: &str,
        version: &str,
        cancellation: &CancellationToken,
    ) -> Result<ProbedCapabilityVersion, BackendError> {
        let mut route_specs = route_specs_for(capability_id)?;
        let spec = PACK_SPECS
            .iter()
            .find(|spec| spec.id == capability_id)
            .ok_or_else(capability_route_contract_error)?;
        let requirement = CapabilityRequirement {
            capability_id: capability_id.into(),
            minimum_version: Some(version.into()),
            protocol_version: "2".into(),
            target_triple: target_triple(),
            accepted_license_expressions: spec
                .licenses
                .iter()
                .map(|value| (*value).into())
                .collect(),
        };
        let manager = CapabilityPackManager::for_installed(
            install_root.to_path_buf(),
            embedded_trusted_keys(),
        );
        let pack = manager.resolve_version(&requirement, version)?;
        validate_signed_product_contract(&pack)?;
        if capability_id == "media-runtime" && !declares_embedded_subtitle_decoder(&pack) {
            route_specs.retain(|spec| spec.route != "media.embedded-subtitle");
        }
        probe_declared_routes(&pack, &route_specs, cancellation, |route| {
            super::pack_engine::probe_capability_pack(&pack, capability_id, route, cancellation)
        })?;
        Ok(ProbedCapabilityVersion {
            pack,
            routes: route_specs,
        })
    }

    /// Publish all probed routes and commit the activation journal while the
    /// routing write lock is held. A commit error restores the previous engine
    /// vector before any resolver can observe the failed snapshot.
    pub fn activate_probed_version_atomically<F>(
        &self,
        probed: ProbedCapabilityVersion,
        capability_id: &str,
        service: &ImportV2Service,
        commit: F,
    ) -> Result<(), BackendError>
    where
        F: FnOnce() -> Result<(), BackendError>,
    {
        if probed.pack.manifest.pack_id != capability_id
            || probed.routes.is_empty()
            || route_specs_for(capability_id)?
                .iter()
                .filter(|spec| {
                    spec.route != "media.embedded-subtitle"
                        || declares_embedded_subtitle_decoder(&probed.pack)
                })
                .map(|spec| spec.route.as_str())
                .ne(probed.routes.iter().map(|spec| spec.route.as_str()))
        {
            return Err(capability_route_contract_error());
        }
        let healthy_version = probed.pack.manifest.version.clone();
        let mut statuses = self.statuses.write().map_err(|_| runtime_locked())?;
        let mut browser_pack = self.browser_pack.write().map_err(|_| runtime_locked())?;
        let replacements = probed
            .routes
            .iter()
            .map(|spec| {
                (
                    probed.pack.clone(),
                    spec.route.clone(),
                    spec.extensions.clone(),
                    spec.timeout,
                )
            })
            .collect();
        service.replace_capability_packs_atomically(replacements, commit)?;
        if capability_id == "browser-runtime" {
            *browser_pack = Some(probed.pack.clone());
        }
        for route in probed.routes.iter().map(|spec| spec.route.as_str()) {
            if let Some(status) = statuses
                .iter_mut()
                .find(|status| status.capability_id == capability_id && status.route == route)
            {
                status.available = true;
                status.healthy_version = Some(healthy_version.clone());
                status.reason = None;
            } else {
                statuses.push(CapabilityRuntimeStatus {
                    capability_id: capability_id.into(),
                    route: route.into(),
                    available: true,
                    healthy_version: Some(healthy_version.clone()),
                    reason: None,
                });
            }
        }
        Ok(())
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SignedCapabilityContract {
    schema_version: u32,
    capability_id: String,
    target_triple: String,
    protocol_version: String,
    entrypoint: String,
    #[serde(default)]
    entrypoint_args: Vec<String>,
    routes: Vec<String>,
    formats: SignedCapabilityFormats,
    runtime: SignedRuntimePermissions,
    license_expression: String,
    source_locks: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SignedCapabilityFormats {
    extensions: Vec<String>,
    platform_content_types: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SignedRuntimePermissions {
    network: bool,
    subprocess: bool,
    filesystem: Vec<String>,
}

fn declares_embedded_subtitle_decoder(pack: &ResolvedCapabilityPack) -> bool {
    std::fs::read(pack.root.join("CAPABILITY-CONTRACT.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<SignedCapabilityContract>(&bytes).ok())
        .is_some_and(|contract| {
            contract
                .routes
                .iter()
                .any(|route| route == "media.embedded-subtitle")
        })
}

fn validate_signed_product_contract(pack: &ResolvedCapabilityPack) -> Result<(), BackendError> {
    if super::capability_pack::read_installation_receipt(&pack.root)?.is_some() {
        // The App catalog authenticated this program at installation. Build-time
        // qualification covers routes; startup needs only protocol compatibility.
        return Ok(());
    }
    let product =
        ProductCapabilityManifest::embedded().map_err(|_| capability_route_contract_error())?;
    let definition = product
        .definition(&pack.manifest.pack_id)
        .filter(|definition| definition.distribution_tier == "published")
        .ok_or_else(capability_route_contract_error)?;
    let contract_path = pack.root.join("CAPABILITY-CONTRACT.json");
    let bytes = std::fs::read(&contract_path).map_err(|_| capability_route_contract_error())?;
    let contract: SignedCapabilityContract =
        serde_json::from_slice(&bytes).map_err(|_| capability_route_contract_error())?;
    // The signed pack owns its actual dependency versions. Desktop recipes are
    // build inputs, not a compatibility pin for already installed runtimes.
    let source_locks_valid = !contract.source_locks.is_empty()
        && contract.source_locks.values().all(|lock| {
            ["version", "license"].iter().all(|key| {
                lock.get(key)
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|value| !value.is_empty())
            })
        });
    let legacy_media = pack.manifest.pack_id == "media-runtime"
        && contract.routes == ["media.subtitle", "media.keyframes"]
        && contract.formats.extensions == ["gif", "wma", "wmv", "srt", "vtt", "ass", "ssa", "lrc"];
    if contract.schema_version != 1
        || contract.capability_id != pack.manifest.pack_id
        || contract.target_triple != target_triple()
        || contract.protocol_version != pack.manifest.protocol_version
        || contract.entrypoint != pack.manifest.entrypoint
        || contract.entrypoint_args != pack.manifest.entrypoint_args
        || (!legacy_media && contract.routes != definition.routes)
        || (!legacy_media && contract.formats.extensions != definition.formats.extensions)
        || contract.formats.platform_content_types != definition.formats.platform_content_types
        || contract.runtime.network != definition.runtime.network
        || contract.runtime.subprocess != definition.runtime.subprocess
        || contract.runtime.filesystem != definition.runtime.filesystem
        || !source_locks_valid
        || contract.license_expression != pack.manifest.license_expression
        || contract.license_expression != definition.license_policy.expression
    {
        return Err(capability_route_contract_error());
    }
    Ok(())
}

fn probe_declared_routes<F>(
    pack: &ResolvedCapabilityPack,
    route_specs: &[CapabilityRouteSpec],
    cancellation: &CancellationToken,
    mut probe: F,
) -> Result<(), BackendError>
where
    F: FnMut(&str) -> Result<(), BackendError>,
{
    if cancellation.is_cancelled() {
        return Err(BackendError::new(
            crate::errors::IMPORT_V2_CANCELLED,
            "Capability health checks were cancelled.",
            true,
            false,
        ));
    }
    super::pack_engine::validate_entrypoint_unchanged(pack)?;
    for route_spec in route_specs.iter().take(1) {
        if cancellation.is_cancelled() {
            return Err(BackendError::new(
                crate::errors::IMPORT_V2_CANCELLED,
                "Capability health checks were cancelled.",
                true,
                false,
            ));
        }
        probe(&route_spec.route)?;
    }
    Ok(())
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

#[derive(Clone, Copy)]
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
        id: "browser-runtime",
        route: "web.x.post",
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
        extensions: &["doc", "docx", "xls", "xlsx", "ppt", "pptx", "pdf"],
        licenses: &["MIT AND PSF-2.0 AND MPL-2.0 AND LicenseRef-Bundled-Third-Party-Notices"],
        timeout_seconds: DEFAULT_PACK_TIMEOUT_SECONDS,
    },
    PackSpec {
        id: "document-layout",
        route: "pdf.layout",
        extensions: &["pdf"],
        licenses: &["MIT AND Apache-2.0 AND CDLA-Permissive-2.0 AND PSF-2.0 AND MPL-2.0 AND LicenseRef-Bundled-Third-Party-Notices"],
        timeout_seconds: DEFAULT_PACK_TIMEOUT_SECONDS,
    },
    PackSpec {
        id: "office-legacy",
        route: "pack.office-legacy",
        extensions: &["doc", "xls", "ppt"],
        licenses: &["(MPL-2.0 OR LGPL-3.0-or-later) AND PSF-2.0 AND MPL-2.0"],
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
        extensions: &["pdf", "png", "jpg", "jpeg", "webp", "tif", "tiff", "bmp"],
        licenses: &["Apache-2.0 AND MIT AND BSD-3-Clause AND HPND AND MPL-2.0 AND PSF-2.0 AND LGPL-2.1-only AND LGPL-3.0-only"],
        timeout_seconds: OCR_PACK_TIMEOUT_SECONDS,
    },
    PackSpec {
        id: "ocr-cjk-accurate",
        route: "ocr.cjk-accurate",
        extensions: &["pdf", "png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff", "heic", "heif"],
        licenses: &["Apache-2.0 AND MIT AND BSD-3-Clause AND HPND AND MPL-2.0 AND PSF-2.0 AND LGPL-2.1-only AND LGPL-3.0-only"],
        timeout_seconds: OCR_PACK_TIMEOUT_SECONDS,
    },
    PackSpec {
        id: "media-runtime",
        route: "media.embedded-subtitle",
        extensions: &["gif", "wma", "mp4", "mkv", "mov", "m4v", "webm", "avi", "wmv"],
        licenses: &["MIT AND LGPL-3.0-or-later"],
        timeout_seconds: DEFAULT_PACK_TIMEOUT_SECONDS,
    },
    PackSpec {
        id: "media-runtime",
        route: "media.keyframes",
        extensions: &["gif", "wma", "mp4", "mkv", "mov", "m4v", "webm", "avi", "wmv"],
        licenses: &["MIT AND LGPL-3.0-or-later"],
        timeout_seconds: DEFAULT_PACK_TIMEOUT_SECONDS,
    },
    PackSpec {
        id: "asr-sensevoice-small",
        route: "media.asr",
        extensions: &[
            "mp3", "wav", "m4a", "aac", "flac", "ogg", "opus", "wma", "mp4", "mov",
            "mkv", "webm", "avi", "m4v", "wmv",
        ],
        licenses: &["Apache-2.0 AND LGPL-3.0-or-later AND MIT"],
        timeout_seconds: ASR_PACK_TIMEOUT_SECONDS,
    },
    PackSpec {
        id: "asr-whisper",
        route: "media.asr",
        extensions: &[
            "mp3", "wav", "m4a", "aac", "flac", "ogg", "opus", "wma", "mp4", "mov",
            "mkv", "webm", "avi", "m4v", "wmv",
        ],
        licenses: &["MIT AND LGPL-3.0-or-later"],
        timeout_seconds: ASR_PACK_TIMEOUT_SECONDS,
    },
];

fn route_specs_for(capability_id: &str) -> Result<Vec<CapabilityRouteSpec>, BackendError> {
    let product =
        ProductCapabilityManifest::embedded().map_err(|_| capability_route_contract_error())?;
    let definition = product
        .definition(capability_id)
        .filter(|definition| definition.distribution_tier == "published")
        .ok_or_else(capability_route_contract_error)?;
    let mut specs = Vec::with_capacity(definition.routes.len());
    for route in &definition.routes {
        let matches = PACK_SPECS
            .iter()
            .filter(|spec| spec.id == capability_id && spec.route == route)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(capability_route_contract_error());
        }
        let spec = matches[0];
        specs.push(CapabilityRouteSpec {
            route: route.clone(),
            extensions: spec
                .extensions
                .iter()
                .map(|value| (*value).into())
                .collect(),
            timeout: Duration::from_secs(spec.timeout_seconds),
        });
    }
    if specs.is_empty()
        || PACK_SPECS
            .iter()
            .filter(|spec| spec.id == capability_id)
            .count()
            != specs.len()
    {
        return Err(capability_route_contract_error());
    }
    Ok(specs)
}

fn capability_route_contract_error() -> BackendError {
    BackendError::new(
        "IMPORT_V2_CAPABILITY_HEALTH_CHECK_FAILED",
        "The capability route contract does not match the product manifest.",
        true,
        false,
    )
}

fn runtime_locked() -> BackendError {
    BackendError::new(
        "IMPORT_V2_CAPABILITY_LOCKED",
        "Capability runtime is unavailable.",
        true,
        false,
    )
}

fn embedded_trusted_keys() -> HashMap<String, Vec<u8>> {
    let encoded: HashMap<String, String> = serde_json::from_str(include_str!(concat!(
        env!("OUT_DIR"),
        "/capabilities/trusted-keys.json"
    )))
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
            healthy_version: available.then(|| "1.0.0".into()),
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
    use crate::services::import_v2::capability_pack::{CapabilityPackFile, CapabilityPackManifest};
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
            &["pdf", "png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff", "heic", "heif"]
        );
        assert!(ocr.timeout_seconds >= 15 * 60);
    }

    #[test]
    fn runtime_specs_resolve_to_product_definitions() {
        let product =
            super::super::product_capability::ProductCapabilityManifest::embedded().unwrap();
        for spec in PACK_SPECS {
            let definition = product
                .definition(spec.id)
                .unwrap_or_else(|| panic!("{} is missing from the product manifest", spec.id));
            assert!(definition.routes.iter().any(|route| route == spec.route));
            assert!(spec
                .licenses
                .iter()
                .all(|license| *license == definition.license_policy.expression));
            assert_eq!(
                spec.extensions,
                definition
                    .formats
                    .extensions
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                "{} / {} format contract drifted from the product manifest",
                spec.id,
                spec.route
            );
        }
    }

    #[test]
    fn every_published_capability_has_an_exact_ordered_runtime_route_contract() {
        let product = ProductCapabilityManifest::embedded().unwrap();
        for definition in product.published_definitions() {
            let specs = route_specs_for(&definition.capability_id).unwrap();
            assert_eq!(
                specs
                    .iter()
                    .map(|spec| spec.route.as_str())
                    .collect::<Vec<_>>(),
                definition
                    .routes
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                "{} must probe every product route before activation",
                definition.capability_id
            );
        }
    }

    #[test]
    fn a_failed_route_probe_stops_the_multi_route_release_before_publication() {
        let specs = route_specs_for("browser-runtime").unwrap();
        let mut observed = Vec::new();
        let root = tempfile::tempdir().unwrap();
        let pack = signed_test_pack(root.path(), "browser-runtime");
        let error = probe_declared_routes(&pack, &specs, &CancellationToken::new(), |route| {
            observed.push(route.to_owned());
            if route == "web.generic.browser" {
                Err(BackendError::new(
                    "IMPORT_V2_CAPABILITY_HEALTH_CHECK_FAILED",
                    "fixture route failed",
                    true,
                    false,
                ))
            } else {
                Ok(())
            }
        })
        .unwrap_err();

        assert_eq!(error.code, "IMPORT_V2_CAPABILITY_HEALTH_CHECK_FAILED");
        assert_eq!(observed, ["web.generic.browser"]);
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
    fn legacy_media_contract_keeps_keyframes_without_advertising_model_free_decoder() {
        let root = tempfile::tempdir().unwrap();
        let pack = signed_test_pack(root.path(), "media-runtime");
        assert!(declares_embedded_subtitle_decoder(&pack));
        validate_signed_product_contract(&pack).unwrap();
        let contract_path = pack.root.join("CAPABILITY-CONTRACT.json");
        let mut contract: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&contract_path).unwrap()).unwrap();
        contract["routes"] = serde_json::json!(["media.subtitle", "media.keyframes"]);
        contract["formats"]["extensions"] =
            serde_json::json!(["gif", "wma", "wmv", "srt", "vtt", "ass", "ssa", "lrc"]);
        std::fs::write(&contract_path, serde_json::to_vec(&contract).unwrap()).unwrap();
        validate_signed_product_contract(&pack).unwrap();
        assert!(!declares_embedded_subtitle_decoder(&pack));
        // Compatibility is limited to the known old route/format declaration.
        contract["routes"] = serde_json::json!(["media.subtitle", "untrusted.route"]);
        std::fs::write(&contract_path, serde_json::to_vec(&contract).unwrap()).unwrap();
        assert!(validate_signed_product_contract(&pack).is_err());
    }

    fn test_keys() -> HashMap<String, Vec<u8>> {
        let key = Ed25519KeyPair::from_seed_unchecked(&[9; 32]).unwrap();
        HashMap::from([("release-test".into(), key.public_key().as_ref().to_vec())])
    }

    fn signed_test_pack(root: &Path, capability_id: &str) -> ResolvedCapabilityPack {
        let pack_root = root.join(capability_id).join("1.2.0");
        std::fs::create_dir_all(&pack_root).unwrap();
        std::fs::write(pack_root.join("runner.bin"), b"verified runtime").unwrap();
        let product: serde_json::Value =
            serde_json::from_str(super::super::product_capability::PRODUCT_MANIFEST_JSON).unwrap();
        let definition = product["definitions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|definition| definition["capabilityId"] == capability_id)
            .unwrap();
        let contract = serde_json::to_vec_pretty(&serde_json::json!({
            "schemaVersion": 1,
            "capabilityId": capability_id,
            "targetTriple": target_triple(),
            "protocolVersion": "2",
            "entrypoint": "runner.bin",
            "entrypointArgs": [],
            "routes": definition["routes"],
            "formats": definition["formats"],
            "runtime": definition["runtime"],
            "licenseExpression": definition["licensePolicy"]["expression"],
            "sourceLocks": {
                "compatibleOlderRuntime": { "version": "1.0.0-previous-desktop", "license": "MIT" }
            }
        }))
        .unwrap();
        std::fs::write(pack_root.join("CAPABILITY-CONTRACT.json"), &contract).unwrap();
        let key = Ed25519KeyPair::from_seed_unchecked(&[9; 32]).unwrap();
        let mut manifest = CapabilityPackManifest {
            schema_version: 2,
            pack_id: capability_id.into(),
            version: "1.2.0".into(),
            protocol_version: "2".into(),
            target_triples: vec![target_triple()],
            archive_sha256: String::new(),
            license_expression: definition["licensePolicy"]["expression"]
                .as_str()
                .unwrap()
                .into(),
            entrypoint: "runner.bin".into(),
            entrypoint_args: Vec::new(),
            executable_files: Vec::new(),
            compressed_bytes: 0,
            installed_bytes: 0,
            signing_key_id: "release-test".into(),
            signature: String::new(),
            files: vec![
                CapabilityPackFile {
                    path: "CAPABILITY-CONTRACT.json".into(),
                    sha256: format!("{:x}", Sha256::digest(&contract)),
                    bytes: contract.len() as u64,
                },
                CapabilityPackFile {
                    path: "runner.bin".into(),
                    sha256: format!("{:x}", Sha256::digest(b"verified runtime")),
                    bytes: b"verified runtime".len() as u64,
                },
            ],
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

        CapabilityPackManager::new(root.to_owned(), test_keys())
            .resolve_version(
                &CapabilityRequirement {
                    capability_id: capability_id.into(),
                    minimum_version: None,
                    protocol_version: "2".into(),
                    target_triple: target_triple(),
                    accepted_license_expressions: vec![manifest.license_expression],
                },
                "1.2.0",
            )
            .unwrap()
    }

    #[test]
    fn compatible_signed_pack_keeps_its_dependency_versions_across_desktop_updates() {
        let root = tempfile::tempdir().unwrap();
        signed_test_pack(root.path(), "document-standard");
        let service = ImportV2Service::default();
        let runtime = ImportCapabilityRuntime::default();
        runtime.load_installed_with_keys(root.path(), &service, test_keys(), false);
        let status = runtime
            .statuses()
            .into_iter()
            .find(|status| status.capability_id == "document-standard")
            .unwrap();
        assert!(status.available, "{:?}", status.reason);
        assert!(service
            .registered_engine_routes()
            .unwrap()
            .iter()
            .any(|route| route == "pack.markitdown"));
    }

    fn measured_hash_bytes(work: impl FnOnce()) -> u64 {
        use super::super::capability_pack::HASHED_RUNTIME_BYTES;
        let before = HASHED_RUNTIME_BYTES.with(|count| count.get());
        work();
        HASHED_RUNTIME_BYTES.with(|count| count.get()) - before
    }

    #[test]
    fn startup_does_not_rehash_payloads_and_detects_missing_entrypoints() {
        let root = tempfile::tempdir().unwrap();
        let pack = signed_test_pack(root.path(), "browser-runtime");
        let service = ImportV2Service::default();
        let runtime = ImportCapabilityRuntime::default();
        let bytes = measured_hash_bytes(|| {
            runtime.load_installed_with_keys(root.path(), &service, test_keys(), false)
        });
        assert_eq!(
            bytes, 0,
            "startup should not reread installed program or model bytes"
        );
        let statuses: Vec<_> = runtime
            .statuses()
            .into_iter()
            .filter(|status| status.capability_id == "browser-runtime")
            .collect();
        assert_eq!(
            statuses.len(),
            route_specs_for("browser-runtime").unwrap().len()
        );
        assert!(statuses.iter().all(|status| status.available));
        std::fs::remove_file(&pack.entrypoint).unwrap();
        runtime.load_installed_with_keys(root.path(), &service, test_keys(), true);
        assert!(runtime
            .statuses()
            .iter()
            .filter(|status| status.capability_id == "browser-runtime")
            .all(|status| !status.available));
    }

    #[test]
    fn multi_route_health_starts_the_runtime_once_without_rehashing_payloads() {
        let root = tempfile::tempdir().unwrap();
        let pack = signed_test_pack(root.path(), "browser-runtime");
        let routes = route_specs_for("browser-runtime").unwrap();
        let mut called = Vec::new();
        let bytes = measured_hash_bytes(|| {
            probe_declared_routes(&pack, &routes, &CancellationToken::new(), |route| {
                called.push(route.to_owned());
                Ok(())
            })
            .unwrap()
        });
        assert_eq!(
            called,
            routes
                .iter()
                .take(1)
                .map(|spec| spec.route.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(bytes, 0);
    }

    #[test]
    fn missing_entrypoint_and_cancelled_preparation_never_become_activatable() {
        let root = tempfile::tempdir().unwrap();
        let pack = signed_test_pack(root.path(), "browser-runtime");
        let routes = route_specs_for("browser-runtime").unwrap();
        std::fs::remove_file(&pack.entrypoint).unwrap();
        assert!(
            probe_declared_routes(&pack, &routes, &CancellationToken::new(), |_| {
                panic!("missing runtime must not be started")
            })
            .is_err()
        );
        let token = CancellationToken::new();
        token.cancel();
        let bytes = measured_hash_bytes(|| {
            let error = probe_declared_routes(&pack, &routes, &token, |_| {
                panic!("cancelled probe must not run")
            })
            .unwrap_err();
            assert_eq!(error.code, crate::errors::IMPORT_V2_CANCELLED);
        });
        assert_eq!(bytes, 0);
    }
}
