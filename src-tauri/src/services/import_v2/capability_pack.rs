use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::RwLock;

use ring::signature::{UnparsedPublicKey, ED25519};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::errors::{BackendError, IMPORT_V2_CAPABILITY_INVALID, IMPORT_V2_CAPABILITY_UNAVAILABLE};
use crate::models::import_v2_file::CapabilityRequirement;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityPackManifest {
    pub schema_version: u32,
    pub pack_id: String,
    pub version: String,
    pub protocol_version: String,
    pub target_triples: Vec<String>,
    pub archive_sha256: String,
    pub license_expression: String,
    pub entrypoint: String,
    pub compressed_bytes: u64,
    pub installed_bytes: u64,
    pub signing_key_id: String,
    pub signature: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackHealth {
    Healthy,
    Unhealthy,
}

#[derive(Debug, Clone)]
pub struct ResolvedCapabilityPack {
    pub manifest: CapabilityPackManifest,
    pub root: PathBuf,
    pub entrypoint: PathBuf,
}

pub struct CapabilityPackManager {
    install_root: PathBuf,
    trusted_keys: HashMap<String, Vec<u8>>,
    health: RwLock<HashMap<(String, String), PackHealth>>,
}

impl CapabilityPackManager {
    pub fn new(install_root: PathBuf, trusted_keys: HashMap<String, Vec<u8>>) -> Self {
        Self {
            install_root,
            trusted_keys,
            health: RwLock::new(HashMap::new()),
        }
    }

    pub fn mark_health(&self, pack_id: &str, version: &str, health: PackHealth) {
        if let Ok(mut states) = self.health.write() {
            states.insert((pack_id.to_owned(), version.to_owned()), health);
        }
    }

    pub fn resolve(
        &self,
        requirement: &CapabilityRequirement,
    ) -> Result<ResolvedCapabilityPack, BackendError> {
        let versions_root = self.install_root.join(&requirement.capability_id);
        let entries = fs::read_dir(&versions_root)
            .map_err(|_| unavailable("The requested capability pack is not installed."))?;
        let minimum = requirement
            .minimum_version
            .as_deref()
            .map(|v| VersionReq::parse(&format!(">={v}")))
            .transpose()
            .map_err(|_| invalid("The capability version requirement is invalid."))?;
        let mut candidates = Vec::new();
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let root = entry.path();
            let Ok(bytes) = fs::read(root.join("manifest.json")) else {
                continue;
            };
            let Ok(manifest) = serde_json::from_slice::<CapabilityPackManifest>(&bytes) else {
                continue;
            };
            if manifest.pack_id != requirement.capability_id {
                continue;
            }
            let version = Version::parse(&manifest.version)
                .map_err(|_| invalid("The capability manifest version is invalid."))?;
            if minimum.as_ref().is_some_and(|req| !req.matches(&version)) {
                continue;
            }
            candidates.push((version, root, manifest));
        }
        candidates.sort_by(|a, b| b.0.cmp(&a.0));
        let mut first_error = None;
        for (_, root, manifest) in candidates {
            if self.health.read().ok().and_then(|states| {
                states
                    .get(&(manifest.pack_id.clone(), manifest.version.clone()))
                    .copied()
            }) == Some(PackHealth::Unhealthy)
            {
                continue;
            }
            match self.validate_candidate(root, manifest, requirement) {
                Ok(pack) => return Ok(pack),
                Err(error) => first_error.get_or_insert(error),
            };
        }
        Err(first_error
            .unwrap_or_else(|| unavailable("No healthy compatible capability pack is installed.")))
    }

    fn validate_candidate(
        &self,
        root: PathBuf,
        manifest: CapabilityPackManifest,
        requirement: &CapabilityRequirement,
    ) -> Result<ResolvedCapabilityPack, BackendError> {
        if manifest.schema_version != 1
            || manifest.protocol_version != requirement.protocol_version
            || !manifest
                .target_triples
                .iter()
                .any(|triple| triple == &requirement.target_triple)
            || !requirement
                .accepted_license_expressions
                .iter()
                .any(|license| license == &manifest.license_expression)
        {
            return Err(invalid(
                "The capability manifest is incompatible with this request.",
            ));
        }
        let key = self
            .trusted_keys
            .get(&manifest.signing_key_id)
            .ok_or_else(|| invalid("The capability manifest is not signed by a trusted key."))?;
        let signature = decode_hex(&manifest.signature)?;
        let payload = manifest.signing_payload()?;
        UnparsedPublicKey::new(&ED25519, key)
            .verify(&payload, &signature)
            .map_err(|_| invalid("The capability manifest signature is invalid."))?;
        let relative = Path::new(&manifest.entrypoint);
        if relative.is_absolute()
            || relative
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(invalid(
                "The capability entrypoint escapes its immutable install directory.",
            ));
        }
        let canonical_root = fs::canonicalize(&root)
            .map_err(|_| invalid("The capability install directory cannot be resolved."))?;
        let entrypoint = root.join(relative);
        if !entrypoint.is_file() {
            return Err(invalid("The capability entrypoint is missing."));
        }
        let entrypoint = fs::canonicalize(entrypoint)
            .map_err(|_| invalid("The capability entrypoint cannot be resolved."))?;
        if !entrypoint.starts_with(&canonical_root) {
            return Err(invalid(
                "The capability entrypoint escapes its immutable install directory.",
            ));
        }
        let archive = root.join("pack.archive");
        let archive_or_entrypoint = if archive.is_file() {
            archive
        } else {
            entrypoint.clone()
        };
        let canonical_payload = fs::canonicalize(archive_or_entrypoint)
            .map_err(|_| invalid("The capability archive cannot be resolved."))?;
        if !canonical_payload.starts_with(&canonical_root) {
            return Err(invalid(
                "The capability archive escapes its immutable install directory.",
            ));
        }
        let bytes = fs::read(canonical_payload)
            .map_err(|_| invalid("The capability archive cannot be read."))?;
        let actual = format!("{:x}", Sha256::digest(bytes));
        if !actual.eq_ignore_ascii_case(&manifest.archive_sha256) {
            return Err(invalid(
                "The capability archive hash does not match its signed manifest.",
            ));
        }
        Ok(ResolvedCapabilityPack {
            manifest,
            root: canonical_root,
            entrypoint,
        })
    }
}

impl CapabilityPackManifest {
    pub fn signing_payload(&self) -> Result<Vec<u8>, BackendError> {
        let mut unsigned = self.clone();
        unsigned.signature.clear();
        serde_json::to_vec(&unsigned)
            .map_err(|_| invalid("The capability manifest cannot be canonicalized."))
    }
}

fn decode_hex(value: &str) -> Result<Vec<u8>, BackendError> {
    if value.len() % 2 != 0 {
        return Err(invalid("The capability signature encoding is invalid."));
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|_| invalid("The capability signature encoding is invalid."))
        })
        .collect()
}

fn invalid(message: &str) -> BackendError {
    BackendError::new(IMPORT_V2_CAPABILITY_INVALID, message, false, false)
}
fn unavailable(message: &str) -> BackendError {
    BackendError::new(IMPORT_V2_CAPABILITY_UNAVAILABLE, message, true, true)
}
