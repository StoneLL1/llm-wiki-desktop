use std::collections::{HashMap, HashSet};

use serde::Deserialize;

pub const PRODUCT_MANIFEST_JSON: &str =
    include_str!("../../../../capabilities/product-manifest.json");

const REQUIRED_TARGETS: &[&str] = &[
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-linux-gnu",
];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductCapabilityManifest {
    #[serde(rename = "$schema")]
    pub schema: Option<String>,
    pub schema_version: u32,
    pub supported_targets: Vec<String>,
    pub surface: ProductSurface,
    pub definitions: Vec<ProductCapabilityDefinition>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductSurface {
    pub routes: Vec<String>,
    pub formats: Vec<String>,
    pub platform_content_types: Vec<String>,
    pub recovery_actions: Vec<String>,
    pub asr_profiles: Vec<String>,
    pub ocr_profiles: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductCapabilityDefinition {
    pub capability_id: String,
    pub name_key: String,
    pub category: String,
    pub purpose_key: String,
    pub routes: Vec<String>,
    pub formats: ProductCapabilityFormats,
    pub protocol_version: String,
    pub distribution_tier: String,
    pub supported_targets: Vec<String>,
    pub license_policy: ProductLicensePolicy,
    pub size_sources: ProductSizeSources,
    pub recovery_actions: Vec<String>,
    pub profiles: ProductProfiles,
    pub installation: ProductInstallation,
    pub runtime: ProductRuntimePermissions,
    pub qualification: ProductQualification,
    pub release: ProductReleasePlan,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductCapabilityFormats {
    pub extensions: Vec<String>,
    pub platform_content_types: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductLicensePolicy {
    pub expression: String,
    pub third_party_notices: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductSizeSources {
    pub compressed_bytes: Option<String>,
    pub installed_bytes: Option<String>,
    pub model_bytes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductProfiles {
    pub asr: Vec<String>,
    pub ocr: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductInstallation {
    pub proactive: bool,
    pub updates: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductRuntimePermissions {
    pub network: bool,
    pub subprocess: bool,
    pub filesystem: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductQualification {
    pub status: String,
    pub entrypoint: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductReleasePlan {
    pub staging_status: String,
    pub staging_script: Option<String>,
    pub owner: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProductInstallCatalog {
    schema_version: u32,
    entries: Vec<ProductCatalogEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProductCatalogEntry {
    capability_id: String,
    version: String,
    target_triple: String,
    url: String,
    archive_sha256: String,
    manifest_sha256: String,
    compressed_bytes: u64,
    installed_bytes: u64,
    model_bytes: Option<u64>,
    license: String,
}

impl ProductCapabilityManifest {
    pub fn embedded() -> Result<Self, String> {
        Self::parse(PRODUCT_MANIFEST_JSON)
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let manifest: Self = serde_json::from_str(text)
            .map_err(|error| format!("product capability manifest is invalid JSON: {error}"))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("product capability manifest schemaVersion must be 1".into());
        }
        if self.supported_targets.len() != REQUIRED_TARGETS.len()
            || !all_unique(self.supported_targets.iter().map(String::as_str))
        {
            return Err("product capability manifest targets must be unique".into());
        }
        if !same_set(&self.supported_targets, REQUIRED_TARGETS.iter().copied()) {
            return Err(
                "product capability manifest must use the exact four desktop targets".into(),
            );
        }
        let mut ids = HashSet::new();
        for definition in &self.definitions {
            if !is_stable_id(&definition.capability_id)
                || !ids.insert(definition.capability_id.as_str())
            {
                return Err("product capability ids must be stable and unique".into());
            }
            if !definition.name_key.starts_with("importV2.capabilityName.")
                || !definition
                    .purpose_key
                    .starts_with("importV2.capabilityPurpose.")
                || definition.category.trim().is_empty()
                || definition.license_policy.expression.trim().is_empty()
            {
                return Err(format!(
                    "product capability {} is missing required product metadata",
                    definition.capability_id
                ));
            }
            if !matches!(
                definition.distribution_tier.as_str(),
                "built_in" | "published" | "experimental" | "unsupported"
            ) {
                return Err(format!(
                    "product capability {} has an invalid distribution tier",
                    definition.capability_id
                ));
            }
            if definition.protocol_version != "2" {
                return Err(format!(
                    "product capability {} must use protocol version 2",
                    definition.capability_id
                ));
            }
            if !matches!(
                definition.qualification.status.as_str(),
                "implemented" | "planned_batch_8" | "not_applicable"
            ) || !matches!(
                definition.release.staging_status.as_str(),
                "implemented" | "planned_batch_8" | "not_applicable"
            ) {
                return Err(format!(
                    "product capability {} has an invalid release status",
                    definition.capability_id
                ));
            }
            if definition.distribution_tier == "published" {
                if !same_set(
                    &definition.supported_targets,
                    self.supported_targets.iter().map(String::as_str),
                ) {
                    return Err(format!(
                        "published product capability {} must support every product target",
                        definition.capability_id
                    ));
                }
                if !definition.installation.proactive || !definition.installation.updates {
                    return Err(format!(
                        "published product capability {} must allow proactive install and updates",
                        definition.capability_id
                    ));
                }
                if definition.release.staging_script.is_none()
                    || definition.qualification.entrypoint.is_none()
                    || definition.release.owner.trim().is_empty()
                {
                    return Err(format!(
                        "published product capability {} is missing its asset plan",
                        definition.capability_id
                    ));
                }
            }
            if definition.distribution_tier == "built_in" && definition.installation.proactive {
                return Err(format!(
                    "built-in product capability {} cannot expose installation",
                    definition.capability_id
                ));
            }
        }
        self.validate_surface_coverage()
    }

    fn validate_surface_coverage(&self) -> Result<(), String> {
        let providers = self.definitions.iter().filter(|definition| {
            matches!(
                definition.distribution_tier.as_str(),
                "built_in" | "published"
            )
        });
        let mut routes = HashSet::new();
        let mut formats = HashSet::new();
        let mut platform_content_types = HashSet::new();
        let mut recovery_actions = HashSet::new();
        let mut asr_profiles = HashSet::new();
        let mut ocr_profiles = HashSet::new();
        for definition in providers {
            routes.extend(definition.routes.iter().map(String::as_str));
            formats.extend(definition.formats.extensions.iter().map(String::as_str));
            platform_content_types.extend(
                definition
                    .formats
                    .platform_content_types
                    .iter()
                    .map(String::as_str),
            );
            recovery_actions.extend(definition.recovery_actions.iter().map(String::as_str));
            asr_profiles.extend(definition.profiles.asr.iter().map(String::as_str));
            ocr_profiles.extend(definition.profiles.ocr.iter().map(String::as_str));
        }
        require_coverage("route", &self.surface.routes, &routes)?;
        require_coverage("format", &self.surface.formats, &formats)?;
        require_coverage(
            "platform content type",
            &self.surface.platform_content_types,
            &platform_content_types,
        )?;
        require_coverage(
            "recovery action",
            &self.surface.recovery_actions,
            &recovery_actions,
        )?;
        require_coverage("ASR profile", &self.surface.asr_profiles, &asr_profiles)?;
        require_coverage("OCR profile", &self.surface.ocr_profiles, &ocr_profiles)
    }

    pub fn definition(&self, capability_id: &str) -> Option<&ProductCapabilityDefinition> {
        self.definitions
            .iter()
            .find(|definition| definition.capability_id == capability_id)
    }

    pub fn published_definitions(&self) -> impl Iterator<Item = &ProductCapabilityDefinition> {
        self.definitions
            .iter()
            .filter(|definition| definition.distribution_tier == "published")
    }

    pub fn expected_release_entry_count(&self) -> usize {
        self.published_definitions().count() * self.supported_targets.len()
    }

    pub fn validate_catalog(
        &self,
        catalog_text: &str,
        distributable: bool,
    ) -> Result<usize, String> {
        self.validate_catalog_for_tag(catalog_text, distributable, None)
    }

    pub fn validate_catalog_for_tag(
        &self,
        catalog_text: &str,
        distributable: bool,
        expected_release_tag: Option<&str>,
    ) -> Result<usize, String> {
        let catalog: ProductInstallCatalog = serde_json::from_str(catalog_text)
            .map_err(|error| format!("install catalog is not valid JSON: {error}"))?;
        if catalog.schema_version != 1 {
            return Err("install catalog schemaVersion must be 1".into());
        }
        let published = self
            .published_definitions()
            .map(|definition| (definition.capability_id.as_str(), definition))
            .collect::<HashMap<_, _>>();
        let mut pairs = HashSet::new();
        for entry in &catalog.entries {
            let definition = published.get(entry.capability_id.as_str()).ok_or_else(|| {
                format!(
                    "catalog capability {} is not published by the product manifest",
                    entry.capability_id
                )
            })?;
            if !definition
                .supported_targets
                .iter()
                .any(|value| value == &entry.target_triple)
            {
                return Err(format!(
                    "catalog capability {} does not support target {}",
                    entry.capability_id, entry.target_triple
                ));
            }
            if entry.license != definition.license_policy.expression {
                return Err(format!(
                    "catalog capability {} license does not match the product manifest",
                    entry.capability_id
                ));
            }
            validate_catalog_release_identity(entry, definition, expected_release_tag)?;
            if !pairs.insert((entry.capability_id.as_str(), entry.target_triple.as_str())) {
                return Err("catalog capability and target pairs must be unique".into());
            }
        }
        if distributable {
            let expected = self.expected_release_entry_count();
            if catalog.entries.len() != expected {
                return Err(format!(
                    "distributable builds require the manifest-derived exact catalog matrix of {expected} entries"
                ));
            }
            for definition in self.published_definitions() {
                for target in &self.supported_targets {
                    if !pairs.contains(&(definition.capability_id.as_str(), target.as_str())) {
                        return Err(format!(
                            "distributable catalog is missing {} for {}",
                            definition.capability_id, target
                        ));
                    }
                }
            }
        }
        Ok(catalog.entries.len())
    }
}

fn validate_catalog_release_identity(
    entry: &ProductCatalogEntry,
    definition: &ProductCapabilityDefinition,
    expected_release_tag: Option<&str>,
) -> Result<(), String> {
    semver::Version::parse(&entry.version).map_err(|_| {
        format!(
            "catalog capability {} has an invalid semantic version",
            entry.capability_id
        )
    })?;
    if entry.compressed_bytes == 0 || entry.installed_bytes == 0 {
        return Err(format!(
            "catalog capability {} must provide positive size facts",
            entry.capability_id
        ));
    }
    if definition.size_sources.model_bytes.is_some()
        && entry.model_bytes.is_none_or(|size| size == 0)
    {
        return Err(format!(
            "catalog capability {} must provide positive model bytes",
            entry.capability_id
        ));
    }
    if !valid_sha256(&entry.archive_sha256) || !valid_sha256(&entry.manifest_sha256) {
        return Err(format!(
            "catalog capability {} must provide non-zero lowercase SHA-256 identities",
            entry.capability_id
        ));
    }
    let prefix = "https://github.com/StoneLL1/llm-wiki-desktop/releases/download/";
    let (release_tag, file_name) = entry
        .url
        .strip_prefix(prefix)
        .and_then(|value| value.split_once('/'))
        .ok_or_else(|| {
            format!(
                "catalog capability {} must use the canonical immutable release URL",
                entry.capability_id
            )
        })?;
    let expected_name = format!(
        "{}-{}-{}.zip",
        entry.capability_id, entry.version, entry.target_triple
    );
    if file_name != expected_name
        || file_name.contains(['?', '#'])
        || !release_tag.starts_with("app-v")
        || expected_release_tag.is_some_and(|expected| release_tag != expected)
    {
        return Err(format!(
            "catalog capability {} release URL does not match its exact identity",
            entry.capability_id
        ));
    }
    Ok(())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        && value.bytes().any(|byte| byte != b'0')
}

fn same_set<'a>(left: &[String], right: impl Iterator<Item = &'a str>) -> bool {
    left.iter().map(String::as_str).collect::<HashSet<_>>() == right.collect::<HashSet<_>>()
}

fn all_unique<'a>(values: impl Iterator<Item = &'a str>) -> bool {
    let mut seen = HashSet::new();
    values.into_iter().all(|value| seen.insert(value))
}

fn is_stable_id(value: &str) -> bool {
    value.bytes().enumerate().all(|(index, byte)| {
        byte.is_ascii_alphanumeric() || (index > 0 && matches!(byte, b'.' | b'_' | b'-'))
    }) && !value.is_empty()
}

fn require_coverage(
    label: &str,
    expected: &[String],
    provided: &HashSet<&str>,
) -> Result<(), String> {
    if let Some(missing) = expected
        .iter()
        .find(|value| !provided.contains(value.as_str()))
    {
        return Err(format!(
            "user-visible {label} {missing} has no built-in or published provider"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_manifest_closes_release_blockers_and_derives_the_matrix() {
        let manifest = ProductCapabilityManifest::embedded().unwrap();
        assert!(manifest
            .definition("browser-runtime")
            .is_some_and(|definition| definition.routes.iter().any(|route| route == "web.x.post")));
        for id in ["document-standard", "office-legacy", "asr-whisper"] {
            assert_eq!(
                manifest
                    .definition(id)
                    .map(|definition| definition.distribution_tier.as_str()),
                Some("published")
            );
        }
        assert_eq!(
            manifest.expected_release_entry_count(),
            manifest.published_definitions().count() * REQUIRED_TARGETS.len()
        );
    }
}
