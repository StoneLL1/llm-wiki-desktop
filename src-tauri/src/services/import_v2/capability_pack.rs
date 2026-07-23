use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::RwLock;

use ring::signature::{UnparsedPublicKey, ED25519};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::errors::{BackendError, IMPORT_V2_CAPABILITY_INVALID, IMPORT_V2_CAPABILITY_UNAVAILABLE};
use crate::models::import_v2_file::CapabilityRequirement;

const MAX_RUNTIME_DIRECTORY_DEPTH: usize = 64;
const MAX_RUNTIME_ENTRIES: usize = 40_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityPackManifest {
    pub schema_version: u32,
    pub pack_id: String,
    pub version: String,
    pub protocol_version: String,
    pub target_triples: Vec<String>,
    pub archive_sha256: String,
    pub license_expression: String,
    pub entrypoint: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entrypoint_args: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub executable_files: Vec<String>,
    pub compressed_bytes: u64,
    pub installed_bytes: u64,
    pub signing_key_id: String,
    pub signature: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<CapabilityPackFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityPackFile {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
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
    pub entrypoint_sha256: String,
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

    /// Resolve one catalog-selected version without allowing a newer side-by-side
    /// installation to satisfy the check. Install verification must bind to the
    /// exact artifact named by the catalog entry.
    pub fn resolve_version(
        &self,
        requirement: &CapabilityRequirement,
        version: &str,
    ) -> Result<ResolvedCapabilityPack, BackendError> {
        Version::parse(version)
            .map_err(|_| invalid("The capability manifest version is invalid."))?;
        let root = self
            .install_root
            .join(&requirement.capability_id)
            .join(version);
        let bytes = fs::read(root.join("manifest.json"))
            .map_err(|_| unavailable("The requested capability pack is not installed."))?;
        let manifest: CapabilityPackManifest = serde_json::from_slice(&bytes)
            .map_err(|_| invalid("The capability manifest is invalid."))?;
        if manifest.pack_id != requirement.capability_id || manifest.version != version {
            return Err(invalid(
                "The capability manifest does not match the requested version.",
            ));
        }
        self.validate_candidate(root, manifest, requirement)
    }

    fn validate_candidate(
        &self,
        root: PathBuf,
        manifest: CapabilityPackManifest,
        requirement: &CapabilityRequirement,
    ) -> Result<ResolvedCapabilityPack, BackendError> {
        if !matches!(manifest.schema_version, 1 | 2)
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
        if manifest.entrypoint_args.len() > 32
            || manifest
                .entrypoint_args
                .iter()
                .any(|argument| argument.len() > 4_096 || argument.contains('\0'))
        {
            return Err(invalid("The capability entrypoint arguments are invalid."));
        }
        validate_executable_files(&manifest)?;
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
        if manifest.schema_version == 1 {
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
            let actual = hash_file(&canonical_payload)?;
            if !actual.eq_ignore_ascii_case(&manifest.archive_sha256) {
                return Err(invalid(
                    "The capability archive hash does not match its signed manifest.",
                ));
            }
        } else if !manifest.archive_sha256.is_empty()
            || manifest.compressed_bytes != 0
            || manifest.installed_bytes != 0
            || manifest.files.is_empty()
        {
            return Err(invalid(
                "Schema v2 capability manifests must delegate archive measurements to the signed application catalog and include a complete file inventory.",
            ));
        }
        let entrypoint_sha256 = hash_file(&entrypoint)?;
        let pack = ResolvedCapabilityPack {
            manifest,
            root: canonical_root,
            entrypoint,
            entrypoint_sha256,
        };
        verify_runtime_integrity(&pack)?;
        Ok(pack)
    }
}

/// Revalidates every installed runtime file immediately before a capability is launched.
/// Resolution verifies the signature; this function closes the mutation window between
/// resolution and process creation without needing the signing key again.
pub fn verify_runtime_integrity(pack: &ResolvedCapabilityPack) -> Result<(), BackendError> {
    let canonical_root = fs::canonicalize(&pack.root)
        .map_err(|_| invalid("The capability install directory cannot be resolved."))?;
    if canonical_root != pack.root {
        return Err(invalid(
            "The capability install directory changed after resolution.",
        ));
    }
    let actual_files = collect_runtime_files(&canonical_root)?;
    if pack.manifest.files.is_empty() {
        let entrypoint_relative = pack.entrypoint.strip_prefix(&canonical_root).map_err(|_| {
            invalid("The capability entrypoint escapes its immutable install directory.")
        })?;
        let expected = portable_relative_path(entrypoint_relative)?;
        if actual_files.len() != 1 || actual_files[0].0 != expected {
            return Err(invalid(
                "Capability packs with runtime siblings require a signed file inventory.",
            ));
        }
        let actual = hash_file(&pack.entrypoint)?;
        if !actual.eq_ignore_ascii_case(&pack.entrypoint_sha256) {
            return Err(invalid(
                "The capability entrypoint changed after resolution.",
            ));
        }
        return Ok(());
    }

    validate_inventory_shape(&pack.manifest.files)?;
    if actual_files.len() != pack.manifest.files.len() {
        return Err(invalid(
            "The capability runtime contains an unexpected or missing file.",
        ));
    }
    let actual_paths: HashSet<&str> = actual_files.iter().map(|(path, _)| path.as_str()).collect();
    for expected in &pack.manifest.files {
        if !actual_paths.contains(expected.path.as_str()) {
            return Err(invalid(
                "The capability runtime contains an unexpected or missing file.",
            ));
        }
        let file = canonical_root.join(Path::new(&expected.path));
        let metadata = fs::metadata(&file)
            .map_err(|_| invalid("A capability runtime file cannot be read."))?;
        if metadata.len() != expected.bytes
            || !hash_file(&file)?.eq_ignore_ascii_case(&expected.sha256)
        {
            return Err(invalid(
                "A capability runtime file does not match its signed inventory.",
            ));
        }
    }
    Ok(())
}

fn validate_inventory_shape(files: &[CapabilityPackFile]) -> Result<(), BackendError> {
    let mut previous: Option<&str> = None;
    for file in files {
        let relative = Path::new(&file.path);
        if file.path.contains('\\')
            || relative.is_absolute()
            || relative
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
            || file.path == "manifest.json"
            || file.path == "pack.archive"
            || file.sha256.len() != 64
            || !file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(invalid("The signed capability file inventory is invalid."));
        }
        if previous.is_some_and(|prior| prior >= file.path.as_str()) {
            return Err(invalid(
                "The signed capability file inventory is not deterministic.",
            ));
        }
        previous = Some(&file.path);
    }
    Ok(())
}

fn validate_executable_files(manifest: &CapabilityPackManifest) -> Result<(), BackendError> {
    let mut previous: Option<&str> = None;
    for path in &manifest.executable_files {
        if previous.is_some_and(|prior| prior >= path.as_str())
            || !manifest.files.iter().any(|file| file.path == *path)
        {
            return Err(invalid(
                "The signed capability executable inventory is invalid.",
            ));
        }
        previous = Some(path);
    }
    Ok(())
}

fn collect_runtime_files(root: &Path) -> Result<Vec<(String, PathBuf)>, BackendError> {
    let mut output = Vec::new();
    let mut pending = vec![(root.to_path_buf(), 0_usize)];
    let mut visited_entries = 0_usize;
    while let Some((directory, depth)) = pending.pop() {
        let entries = fs::read_dir(directory)
            .map_err(|_| invalid("The capability runtime directory cannot be read."))?;
        for entry in entries {
            let entry =
                entry.map_err(|_| invalid("The capability runtime directory cannot be read."))?;
            visited_entries = visited_entries
                .checked_add(1)
                .ok_or_else(|| invalid("The capability runtime contains too many entries."))?;
            if visited_entries > MAX_RUNTIME_ENTRIES {
                return Err(invalid("The capability runtime contains too many entries."));
            }
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|_| invalid("A capability runtime file cannot be inspected."))?;
            if metadata.file_type().is_symlink() {
                return Err(invalid(
                    "Capability runtime symbolic links are not allowed.",
                ));
            }
            if metadata.is_dir() {
                if depth >= MAX_RUNTIME_DIRECTORY_DEPTH {
                    return Err(invalid(
                        "The capability runtime directory nesting is too deep.",
                    ));
                }
                pending.push((path, depth + 1));
            } else if metadata.is_file() {
                let relative = path.strip_prefix(root).map_err(|_| {
                    invalid("A capability runtime file escapes its install directory.")
                })?;
                let portable = portable_relative_path(relative)?;
                if portable != "manifest.json" && portable != "pack.archive" {
                    output.push((portable, path));
                }
            } else {
                return Err(invalid("Capability runtime special files are not allowed."));
            }
        }
    }
    output.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(output)
}

fn portable_relative_path(path: &Path) -> Result<String, BackendError> {
    let mut parts = Vec::new();
    for component in path.components() {
        let Component::Normal(part) = component else {
            return Err(invalid("A capability runtime path is invalid."));
        };
        parts.push(
            part.to_str()
                .ok_or_else(|| invalid("A capability runtime path is not valid UTF-8."))?,
        );
    }
    if parts.is_empty() {
        return Err(invalid("A capability runtime path is empty."));
    }
    Ok(parts.join("/"))
}

pub(super) fn hash_file(path: &Path) -> Result<String, BackendError> {
    let mut file =
        fs::File::open(path).map_err(|_| invalid("A capability runtime file cannot be read."))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| invalid("A capability runtime file cannot be read."))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_inventory_walks_deep_directories_without_recursion() {
        let root = std::env::temp_dir().join(format!("cap-pack-depth-{}", uuid::Uuid::new_v4()));
        let mut directory = root.clone();
        for _ in 0..MAX_RUNTIME_DIRECTORY_DEPTH {
            directory = directory.join("d");
        }
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("runner.bin"), b"runtime").unwrap();

        let files = collect_runtime_files(&root).unwrap();

        assert_eq!(files.len(), 1);
        assert!(files[0].0.ends_with("runner.bin"));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn runtime_inventory_rejects_excessive_directory_depth() {
        let root = std::env::temp_dir().join(format!("cap-pack-limit-{}", uuid::Uuid::new_v4()));
        let mut directory = root.clone();
        for _ in 0..=MAX_RUNTIME_DIRECTORY_DEPTH {
            directory = directory.join("d");
        }
        fs::create_dir_all(&directory).unwrap();

        let error = collect_runtime_files(&root).unwrap_err();

        assert_eq!(error.code, IMPORT_V2_CAPABILITY_INVALID);
        assert!(error.message.contains("nesting is too deep"));
        fs::remove_dir_all(root).ok();
    }
}
