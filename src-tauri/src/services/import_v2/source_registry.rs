use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::Digest;
use unicode_normalization::UnicodeNormalization;

use crate::errors::{BackendError, IMPORT_V2_SOURCE_INDEX_INVALID};
use crate::models::compile::{CompileConsumptionRecord, SourceVersionRef};
use crate::models::import_v2::{ImportInputKind, QualityReport};
use crate::models::paths::ProjectContext;
use crate::services::FileStore;
use crate::utils::path_utils::normalize_project_path;

use super::source_finalization::{parse_final_source, validate_source_version_binding};
use super::transaction::{is_project_reparse_point, read_project_file_nofollow, FileTransaction};

const SOURCE_INDEX_PATH: &str = ".app/source-index-v2.json";
pub const SOURCE_REGISTRY_SCHEMA_VERSION: u32 = 3;
const LEGACY_SOURCE_REGISTRY_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceIndex {
    pub schema_version: u32,
    pub by_content_hash: BTreeMap<String, SourcePointer>,
    pub by_locator: BTreeMap<String, SourcePointer>,
}

impl<'de> Deserialize<'de> for SourceIndex {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SourceIndexVisitor;

        impl<'de> Visitor<'de> for SourceIndexVisitor {
            type Value = SourceIndex;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an import v2 source index")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut schema_version = None;
                let mut by_content_hash = None;
                let mut by_locator = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "schemaVersion" if schema_version.is_none() => {
                            schema_version = Some(map.next_value()?);
                        }
                        "byContentHash" if by_content_hash.is_none() => {
                            by_content_hash = Some(map.next_value::<UniquePointerMap>()?.0);
                        }
                        "byLocator" if by_locator.is_none() => {
                            by_locator = Some(map.next_value::<UniquePointerMap>()?.0);
                        }
                        "schemaVersion" | "byContentHash" | "byLocator" => {
                            let field = match key.as_str() {
                                "schemaVersion" => "schemaVersion",
                                "byContentHash" => "byContentHash",
                                _ => "byLocator",
                            };
                            return Err(de::Error::duplicate_field(field));
                        }
                        _ => {
                            let _: de::IgnoredAny = map.next_value()?;
                        }
                    }
                }
                Ok(SourceIndex {
                    schema_version: schema_version
                        .ok_or_else(|| de::Error::missing_field("schemaVersion"))?,
                    by_content_hash: by_content_hash
                        .ok_or_else(|| de::Error::missing_field("byContentHash"))?,
                    by_locator: by_locator.ok_or_else(|| de::Error::missing_field("byLocator"))?,
                })
            }
        }

        deserializer.deserialize_map(SourceIndexVisitor)
    }
}

struct UniquePointerMap(BTreeMap<String, SourcePointer>);

impl<'de> Deserialize<'de> for UniquePointerMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct UniqueMapVisitor;

        impl<'de> Visitor<'de> for UniqueMapVisitor {
            type Value = UniquePointerMap;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a map with unique source keys")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut values = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<String, SourcePointer>()? {
                    if values.insert(key.clone(), value).is_some() {
                        return Err(de::Error::custom(format!("duplicate source key: {key}")));
                    }
                }
                Ok(UniquePointerMap(values))
            }
        }

        deserializer.deserialize_map(UniqueMapVisitor)
    }
}

impl SourceIndex {
    pub fn default_v2() -> Self {
        Self {
            schema_version: SOURCE_REGISTRY_SCHEMA_VERSION,
            by_content_hash: BTreeMap::new(),
            by_locator: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourcePointer {
    pub source_id: String,
    pub version_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceManifest {
    pub schema_version: u32,
    pub source_id: String,
    pub source_kind: String,
    pub current_version_id: String,
    pub wiki_path: String,
    #[serde(default)]
    pub aliases: Vec<SourceAlias>,
    pub origins: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform_content_id: Option<String>,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    pub imported_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub versions: Vec<SourceVersion>,
    #[serde(default)]
    pub compiled_consumptions: Vec<CompiledConsumption>,
    #[serde(default)]
    pub restricted_content: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restricted_identity_summary: Option<String>,
    #[serde(default)]
    pub timeline: Vec<SourceTimelineEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceVersion {
    pub version_id: String,
    pub content_hash: String,
    #[serde(default)]
    pub raw_evidence: Vec<SourceArtifactRecord>,
    #[serde(default)]
    pub assets: Vec<SourceArtifactRecord>,
    pub baseline_path: String,
    pub candidate: SourceCandidateRecord,
    pub provenance: SourceProvenance,
    pub quality: QualityReport,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_edit_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceAlias {
    pub kind: String,
    pub value: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceArtifactRecord {
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceCandidateRecord {
    pub markdown_hash: String,
    pub title: String,
    pub source_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform_content_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceProvenance {
    pub locator: String,
    pub route: String,
    pub engine_id: String,
    pub engine_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompiledConsumption {
    pub version_id: String,
    pub content_hash: String,
    pub compile_task_id: String,
    pub consumed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceTimelineEvent {
    pub event_id: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_id: Option<String>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceResolution {
    New,
    ExactDuplicate {
        source_id: String,
        version_id: String,
    },
    UpdatedOrigin {
        source_id: String,
        previous_version_id: String,
    },
    SameContentNewOrigin {
        source_id: String,
        version_id: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceCommitPlan {
    pub resolution: SourceResolution,
    pub source_id: String,
    pub version_id: String,
    pub previous_current_version_id: Option<String>,
    pub raw_path: String,
    pub evidence_root_path: String,
    pub asset_root_path: String,
    pub baseline_path: String,
    pub wiki_path: String,
    pub manifest_path: String,
    pub next_manifest: SourceManifest,
    pub next_index: SourceIndex,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceCommitInput {
    pub normalized_locator: String,
    pub content_hash: String,
    pub display_name: String,
    pub input_kind: ImportInputKind,
    pub source_extension: String,
    pub source_kind: String,
    pub canonical_url: Option<String>,
    pub platform: Option<String>,
    pub platform_content_id: Option<String>,
    pub title: String,
    pub author: Option<String>,
    pub published_at: Option<String>,
    pub imported_at: String,
    pub language: Option<String>,
    pub candidate_markdown_hash: String,
    pub route: String,
    pub engine_id: String,
    pub engine_version: String,
    pub quality: QualityReport,
}

#[derive(Debug, Clone)]
pub struct ValidatedCompileSourceVersion {
    pub manifest_path: String,
    pub project_path: String,
    pub manifest: SourceManifest,
    pub version: SourceVersion,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacySourceManifest {
    schema_version: u32,
    source_id: String,
    origins: Vec<String>,
    versions: Vec<LegacySourceVersion>,
    current_version_id: String,
    wiki_path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacySourceVersion {
    version_id: String,
    content_hash: String,
    raw_path: String,
    #[serde(default, rename = "extractedPath")]
    _extracted_path: String,
    baseline_path: String,
    created_at: String,
    route: String,
    engine_id: String,
    engine_version: String,
    quality: QualityReport,
}

#[derive(Default)]
pub struct SourceRegistry;

impl SourceRegistry {
    pub fn validate_manifest_contract(manifest: &SourceManifest) -> Result<(), BackendError> {
        validate_manifest(manifest)
    }

    pub fn resolve_compile_source_version(
        context: &ProjectContext,
        files: &FileStore,
        index: &SourceIndex,
        reference: &SourceVersionRef,
    ) -> Result<ValidatedCompileSourceVersion, BackendError> {
        validate_identity(&reference.source_id)?;
        validate_identity(&reference.version_id)?;
        if !is_sha256(&reference.content_hash) {
            return Err(compile_source_version_changed());
        }
        let expected_pointer = SourcePointer {
            source_id: reference.source_id.clone(),
            version_id: reference.version_id.clone(),
        };
        if index.by_content_hash.get(&reference.content_hash) != Some(&expected_pointer) {
            return Err(compile_source_version_changed());
        }

        let manifest_path = format!(".app/sources/{}.json", reference.source_id);
        let manifest = Self::read_manifest(context, files, &manifest_path)
            .map_err(|_| compile_source_version_changed())?;
        if manifest.source_id != reference.source_id {
            return Err(compile_source_version_changed());
        }
        let version = manifest
            .versions
            .iter()
            .find(|version| {
                version.version_id == reference.version_id
                    && version.content_hash == reference.content_hash
            })
            .cloned()
            .ok_or_else(compile_source_version_changed)?;
        let project_path = if manifest.current_version_id == version.version_id {
            manifest.wiki_path.clone()
        } else {
            version.baseline_path.clone()
        };
        let absolute = context.resolve_project_path(&project_path)?;
        let markdown = read_project_file_nofollow(&context.root, &absolute)
            .map_err(|_| compile_source_version_changed())?;
        let expected_markdown_hash = version
            .human_edit_hash
            .as_deref()
            .ok_or_else(compile_source_version_changed)?;
        if format!("{:x}", sha2::Sha256::digest(&markdown)) != expected_markdown_hash
            || validate_source_version_binding(&markdown, &manifest, &version).is_err()
        {
            return Err(compile_source_version_changed());
        }

        Ok(ValidatedCompileSourceVersion {
            manifest_path,
            project_path,
            manifest,
            version,
        })
    }

    pub fn record_compile_consumption(
        context: &ProjectContext,
        files: &FileStore,
        record: &CompileConsumptionRecord,
    ) -> Result<Vec<SourceVersionRef>, BackendError> {
        let record_path = format!(".app/compile/{}.json", record.compile_task_id);
        let record_absolute = context.resolve_project_path(&record_path)?;
        if record_absolute.exists() {
            return Err(BackendError::new(
                "COMPILE_CONSUMPTION_EXISTS",
                "This Compile task already has a consumption record.",
                false,
                true,
            ));
        }
        let index = record
            .source_versions
            .iter()
            .any(|reference| !reference.source_id.starts_with("legacy-"))
            .then(|| Self::read_index(context, files))
            .transpose()?;
        let mut manifests = Vec::new();
        let mut consumed = Vec::new();
        for reference in &record.source_versions {
            validate_identity(&reference.source_id)?;
            validate_identity(&reference.version_id)?;
            // Legacy refs are recorded in the task record only. They never
            // cause the legacy index to be rewritten.
            if reference.source_id.starts_with("legacy-") {
                consumed.push(reference.clone());
                continue;
            }
            let validated = Self::resolve_compile_source_version(
                context,
                files,
                index.as_ref().ok_or_else(compile_source_version_changed)?,
                reference,
            )?;
            let relative = validated.manifest_path;
            let absolute = context.resolve_project_path(&relative)?;
            let expected_hash = files.file_hash(context, &relative)?;
            let mut manifest = validated.manifest;
            if manifest.compiled_consumptions.iter().any(|entry| {
                entry.version_id == reference.version_id
                    && entry.content_hash == reference.content_hash
            }) {
                return Err(compile_source_version_changed());
            }
            manifest.compiled_consumptions.push(CompiledConsumption {
                version_id: reference.version_id.clone(),
                content_hash: reference.content_hash.clone(),
                compile_task_id: record.compile_task_id.clone(),
                consumed_at: record.consumed_at.clone(),
            });
            validate_manifest(&manifest)?;
            manifests.push((absolute, expected_hash, manifest));
            consumed.push(reference.clone());
        }
        let persisted_record = CompileConsumptionRecord {
            source_versions: consumed.clone(),
            ..record.clone()
        };
        let mut transaction = FileTransaction::new_for_project(&context.root);
        for (absolute, expected_hash, manifest) in manifests {
            transaction.write_if_hash_matches(
                &absolute,
                &serde_json::to_vec_pretty(&manifest).map_err(|error| {
                    BackendError::new(
                        "COMPILE_CONSUMPTION_WRITE_FAILED",
                        error.to_string(),
                        true,
                        false,
                    )
                })?,
                &expected_hash,
            )?;
        }
        transaction.write_new(
            &record_absolute,
            &serde_json::to_vec_pretty(&persisted_record).map_err(|error| {
                BackendError::new(
                    "COMPILE_CONSUMPTION_WRITE_FAILED",
                    error.to_string(),
                    true,
                    false,
                )
            })?,
        )?;
        transaction.commit()?;
        Ok(consumed)
    }

    pub fn read_index(
        context: &ProjectContext,
        files: &FileStore,
    ) -> Result<SourceIndex, BackendError> {
        if !files.exists(context, SOURCE_INDEX_PATH) {
            return Ok(SourceIndex::default_v2());
        }

        let mut index: SourceIndex = files
            .read_json(context, SOURCE_INDEX_PATH)
            .map_err(|_| invalid_index())?;
        if index.schema_version == LEGACY_SOURCE_REGISTRY_SCHEMA_VERSION {
            index.schema_version = SOURCE_REGISTRY_SCHEMA_VERSION;
        }
        validate_index(&index)?;
        Ok(index)
    }

    pub fn read_manifest(
        context: &ProjectContext,
        files: &FileStore,
        manifest_path: &str,
    ) -> Result<SourceManifest, BackendError> {
        let value: serde_json::Value = files
            .read_json(context, manifest_path)
            .map_err(|_| invalid_index())?;
        match value
            .get("schemaVersion")
            .and_then(serde_json::Value::as_u64)
        {
            Some(version) if version == u64::from(SOURCE_REGISTRY_SCHEMA_VERSION) => {
                let manifest: SourceManifest =
                    serde_json::from_value(value).map_err(|_| invalid_index())?;
                validate_manifest(&manifest)?;
                Ok(manifest)
            }
            Some(version) if version == u64::from(LEGACY_SOURCE_REGISTRY_SCHEMA_VERSION) => {
                let legacy: LegacySourceManifest =
                    serde_json::from_value(value).map_err(|_| invalid_index())?;
                migrate_legacy_manifest(context, files, legacy)
            }
            _ => Err(invalid_index()),
        }
    }

    /// Upgrade the complete project registry as one hash-guarded transaction.
    /// Callers must hold the Import V2 mutation lock. Parsing and validation
    /// finish before the first write, and a failed installation rolls every
    /// manifest and the index back to their exact prior bytes.
    pub fn migrate_project_v3(
        context: &ProjectContext,
        files: &FileStore,
    ) -> Result<bool, BackendError> {
        FileTransaction::reconcile_project(&context.root)?;
        let mut writes = Vec::<(PathBuf, Vec<u8>, String)>::new();
        let index_path = context.resolve_project_path(SOURCE_INDEX_PATH)?;
        if index_path.exists() {
            let bytes = read_project_file_nofollow(&context.root, &index_path)
                .map_err(|_| invalid_index())?;
            let value: serde_json::Value =
                serde_json::from_slice(&bytes).map_err(|_| invalid_index())?;
            let schema = value
                .get("schemaVersion")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(invalid_index)?;
            if schema == u64::from(LEGACY_SOURCE_REGISTRY_SCHEMA_VERSION) {
                let index = Self::read_index(context, files)?;
                writes.push((
                    index_path,
                    pretty_json_bytes(&index)?,
                    format!("{:x}", sha2::Sha256::digest(&bytes)),
                ));
            } else if schema != u64::from(SOURCE_REGISTRY_SCHEMA_VERSION) {
                return Err(invalid_index());
            } else {
                let index: SourceIndex =
                    serde_json::from_value(value).map_err(|_| invalid_index())?;
                validate_index(&index)?;
            }
        }

        for manifest_path in manifest_paths_for_migration(context)? {
            let bytes = read_project_file_nofollow(&context.root, &manifest_path)
                .map_err(|_| invalid_index())?;
            let value: serde_json::Value =
                serde_json::from_slice(&bytes).map_err(|_| invalid_index())?;
            let schema = value
                .get("schemaVersion")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(invalid_index)?;
            if schema == u64::from(LEGACY_SOURCE_REGISTRY_SCHEMA_VERSION) {
                let legacy: LegacySourceManifest =
                    serde_json::from_value(value).map_err(|_| invalid_index())?;
                let manifest = migrate_legacy_manifest(context, files, legacy)?;
                writes.push((
                    manifest_path,
                    pretty_json_bytes(&manifest)?,
                    format!("{:x}", sha2::Sha256::digest(&bytes)),
                ));
            } else if schema == u64::from(SOURCE_REGISTRY_SCHEMA_VERSION) {
                let manifest: SourceManifest =
                    serde_json::from_value(value).map_err(|_| invalid_index())?;
                validate_manifest(&manifest)?;
            } else {
                return Err(invalid_index());
            }
        }
        if writes.is_empty() {
            return Ok(false);
        }
        let mut transaction = FileTransaction::new_for_project(&context.root);
        for (path, bytes, expected_hash) in writes {
            transaction.write_if_hash_matches(&path, &bytes, &expected_hash)?;
        }
        transaction.commit()?;
        Ok(true)
    }

    /// Resolve an imported source asset for a Wiki page without exposing the
    /// raw filesystem layout to the renderer. Markdown keeps the portable
    /// `assets/...` reference; the source manifest supplies the immutable
    /// source/version directory that owns that asset.
    pub fn resolve_wiki_asset_path(
        context: &ProjectContext,
        files: &FileStore,
        wiki_path: &str,
        asset_path: &str,
    ) -> Result<PathBuf, BackendError> {
        let wiki_path = normalize_project_path(wiki_path.trim());
        let wiki_absolute = context.resolve_project_path(&wiki_path)?;
        if !wiki_path.starts_with("wiki/")
            || !wiki_path.ends_with(".md")
            || wiki_absolute.strip_prefix(&context.wiki_dir).is_err()
            || !wiki_absolute.is_file()
        {
            return Err(wiki_asset_not_found(&wiki_path, asset_path));
        }

        let asset_path = asset_path
            .split(['?', '#'])
            .next()
            .unwrap_or("")
            .trim()
            .replace('\\', "/");
        let asset_path = asset_path.trim_start_matches("./");
        let asset_parts: Vec<&str> = asset_path.split('/').collect();
        if asset_parts.len() < 2
            || asset_parts[0] != "assets"
            || asset_parts
                .iter()
                .any(|part| part.is_empty() || *part == "." || *part == ".." || part.contains(':'))
        {
            return Err(wiki_asset_not_found(&wiki_path, asset_path));
        }
        let asset_relative = asset_parts[1..].join("/");

        let index = Self::read_index(context, files)?;
        let wiki_bytes = read_project_file_nofollow(&context.root, &wiki_absolute)
            .map_err(|_| wiki_asset_not_found(&wiki_path, asset_path))?;
        if let Ok(wiki_markdown) = std::str::from_utf8(&wiki_bytes) {
            if let Ok((frontmatter, _)) = parse_final_source(wiki_markdown) {
                if !is_safe_id(&frontmatter.source_id)
                    || !is_safe_id(&frontmatter.version_id)
                    || !index
                        .by_content_hash
                        .get(&frontmatter.content_hash)
                        .is_some_and(|pointer| {
                            pointer.source_id == frontmatter.source_id
                                && pointer.version_id == frontmatter.version_id
                        })
                {
                    return Err(wiki_asset_not_found(&wiki_path, asset_path));
                }

                let manifest_path = format!(".app/sources/{}.json", frontmatter.source_id);
                let manifest = Self::read_manifest(context, files, &manifest_path)
                    .map_err(|_| wiki_asset_not_found(&wiki_path, asset_path))?;
                let version = manifest
                    .versions
                    .iter()
                    .find(|version| version.version_id == manifest.current_version_id);
                if manifest.source_id != frontmatter.source_id
                    || manifest.current_version_id != frontmatter.version_id
                    || normalize_project_path(&manifest.wiki_path) != wiki_path
                    || version.is_none_or(|version| {
                        validate_source_version_binding(&wiki_bytes, &manifest, version).is_err()
                    })
                {
                    return Err(wiki_asset_not_found(&wiki_path, asset_path));
                }

                return resolve_manifest_asset_path(context, &manifest, &asset_relative)?
                    .ok_or_else(|| wiki_asset_not_found(&wiki_path, asset_path));
            }
        }

        // V2 projects did not put a Source identity in Wiki frontmatter. Keep
        // their read-only compatibility path after the transactional V3 JSON
        // migration, but accept only an exact Wiki path whose current version
        // carries the migration marker. Modern malformed Source pages never
        // gain that marker and therefore still fail closed.
        let mut source_ids = std::collections::BTreeSet::new();
        source_ids.extend(
            index
                .by_content_hash
                .values()
                .map(|pointer| pointer.source_id.clone()),
        );
        source_ids.extend(
            index
                .by_locator
                .values()
                .map(|pointer| pointer.source_id.clone()),
        );

        for source_id in source_ids {
            let manifest_path = format!(".app/sources/{source_id}.json");
            let persisted_schema = match persisted_registry_schema(context, &manifest_path) {
                Ok(schema) => schema,
                Err(_) => continue,
            };
            let manifest = match Self::read_manifest(context, files, &manifest_path) {
                Ok(manifest) => manifest,
                Err(_) => continue,
            };
            if normalize_project_path(&manifest.wiki_path) != wiki_path
                || (persisted_schema != Some(LEGACY_SOURCE_REGISTRY_SCHEMA_VERSION)
                    && !current_version_has_legacy_migration_marker(&manifest))
            {
                continue;
            }

            return resolve_manifest_asset_path(context, &manifest, &asset_relative)?
                .ok_or_else(|| wiki_asset_not_found(&wiki_path, asset_path));
        }

        Err(wiki_asset_not_found(&wiki_path, asset_path))
    }

    pub fn resolve(
        index: &SourceIndex,
        normalized_locator: &str,
        content_hash: &str,
    ) -> SourceResolution {
        match (
            index.by_locator.get(normalized_locator),
            index.by_content_hash.get(content_hash),
        ) {
            (Some(locator_pointer), Some(hash_pointer))
                if locator_pointer.source_id == hash_pointer.source_id =>
            {
                SourceResolution::ExactDuplicate {
                    source_id: hash_pointer.source_id.clone(),
                    version_id: hash_pointer.version_id.clone(),
                }
            }
            (Some(locator_pointer), _) => SourceResolution::UpdatedOrigin {
                source_id: locator_pointer.source_id.clone(),
                previous_version_id: locator_pointer.version_id.clone(),
            },
            (None, Some(hash_pointer)) => SourceResolution::SameContentNewOrigin {
                source_id: hash_pointer.source_id.clone(),
                version_id: hash_pointer.version_id.clone(),
            },
            (None, None) => SourceResolution::New,
        }
    }

    pub fn build_commit_plan(
        &self,
        index: &SourceIndex,
        existing_manifest: Option<&SourceManifest>,
        input: &SourceCommitInput,
    ) -> Result<SourceCommitPlan, BackendError> {
        validate_index(index)?;
        if let Some(manifest) = existing_manifest {
            validate_manifest(manifest)?;
        }
        let locator = normalize_locator(&input.normalized_locator);
        if locator.is_empty() || input.content_hash.trim().is_empty() {
            return Err(invalid_index());
        }
        if matches!(
            (
                index.by_locator.get(&locator),
                index.by_content_hash.get(&input.content_hash),
            ),
            (Some(locator_pointer), Some(hash_pointer))
                if locator_pointer.source_id != hash_pointer.source_id
        ) {
            return Err(invalid_index());
        }

        let resolution = match (
            Self::resolve(index, &locator, &input.content_hash),
            existing_manifest,
        ) {
            (SourceResolution::New, Some(manifest))
                if manifest.origins.iter().any(|origin| origin == &locator) =>
            {
                SourceResolution::UpdatedOrigin {
                    source_id: manifest.source_id.clone(),
                    previous_version_id: manifest.current_version_id.clone(),
                }
            }
            (resolution, _) => resolution,
        };
        let source_id = match &resolution {
            SourceResolution::New => uuid::Uuid::new_v4().to_string(),
            SourceResolution::ExactDuplicate { source_id, .. }
            | SourceResolution::UpdatedOrigin { source_id, .. }
            | SourceResolution::SameContentNewOrigin { source_id, .. } => source_id.clone(),
        };
        let existing = existing_manifest.filter(|manifest| manifest.source_id == source_id);

        let reused_version_id = match &resolution {
            SourceResolution::ExactDuplicate { version_id, .. }
            | SourceResolution::SameContentNewOrigin { version_id, .. } => Some(version_id.clone()),
            SourceResolution::New | SourceResolution::UpdatedOrigin { .. } => None,
        };
        if !matches!(resolution, SourceResolution::New) && existing.is_none() {
            return Err(invalid_index());
        }

        let version_id = reused_version_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let extension = safe_extension(&input.source_extension);
        let evidence_root_path = match input.input_kind {
            ImportInputKind::Url => format!("raw/web/{source_id}/{version_id}"),
            ImportInputKind::File | ImportInputKind::Folder | ImportInputKind::ClipboardText => {
                format!("raw/sources/{source_id}/{version_id}")
            }
        };
        let raw_name = if input.input_kind == ImportInputKind::Url {
            "snapshot"
        } else {
            "original"
        };
        let raw_path = format!("{evidence_root_path}/{raw_name}.{extension}");
        let baseline_path = format!(".app/source-artifacts/{source_id}/{version_id}/baseline.md");
        let wiki_path = existing
            .map(|manifest| manifest.wiki_path.clone())
            .unwrap_or_else(|| derive_wiki_path(input));

        let created_at = input.imported_at.clone();
        let previous_current_version_id =
            existing.map(|manifest| manifest.current_version_id.clone());
        let mut next_manifest = existing.cloned().unwrap_or_else(|| SourceManifest {
            schema_version: SOURCE_REGISTRY_SCHEMA_VERSION,
            source_id: source_id.clone(),
            source_kind: input.source_kind.clone(),
            current_version_id: version_id.clone(),
            wiki_path: wiki_path.clone(),
            aliases: Vec::new(),
            origins: Vec::new(),
            canonical_url: input.canonical_url.clone(),
            platform: input.platform.clone(),
            platform_content_id: input.platform_content_id.clone(),
            title: input.title.clone(),
            author: input.author.clone(),
            published_at: input.published_at.clone(),
            imported_at: input.imported_at.clone(),
            language: input.language.clone(),
            versions: Vec::new(),
            compiled_consumptions: Vec::new(),
            restricted_content: false,
            restricted_identity_summary: None,
            timeline: Vec::new(),
        });
        next_manifest.schema_version = SOURCE_REGISTRY_SCHEMA_VERSION;
        if next_manifest.canonical_url.is_none() {
            next_manifest.canonical_url.clone_from(&input.canonical_url);
        }
        if next_manifest.platform.is_none() {
            next_manifest.platform.clone_from(&input.platform);
        }
        if next_manifest.platform_content_id.is_none() {
            next_manifest
                .platform_content_id
                .clone_from(&input.platform_content_id);
        }
        if next_manifest.author.is_none() {
            next_manifest.author.clone_from(&input.author);
        }
        if next_manifest.published_at.is_none() {
            next_manifest.published_at.clone_from(&input.published_at);
        }
        if next_manifest.language.is_none() {
            next_manifest.language.clone_from(&input.language);
        }
        if !next_manifest.origins.contains(&locator) {
            next_manifest.origins.push(locator.clone());
            next_manifest.origins.sort();
        }
        if !next_manifest
            .aliases
            .iter()
            .any(|alias| alias.value == locator)
        {
            next_manifest.aliases.push(SourceAlias {
                kind: if input.input_kind == ImportInputKind::Url {
                    "url"
                } else {
                    "locator"
                }
                .into(),
                value: locator.clone(),
                created_at: created_at.clone(),
            });
            next_manifest
                .aliases
                .sort_by(|left, right| left.value.cmp(&right.value));
        }
        if let Some(canonical_url) = input
            .canonical_url
            .as_deref()
            .map(normalize_locator)
            .filter(|canonical_url| !canonical_url.is_empty() && canonical_url != &locator)
        {
            if !next_manifest.origins.contains(&canonical_url) {
                next_manifest.origins.push(canonical_url.clone());
                next_manifest.origins.sort();
            }
            if !next_manifest
                .aliases
                .iter()
                .any(|alias| alias.value == canonical_url)
            {
                next_manifest.aliases.push(SourceAlias {
                    kind: "canonical_url".into(),
                    value: canonical_url,
                    created_at: input.imported_at.clone(),
                });
                next_manifest
                    .aliases
                    .sort_by(|left, right| left.value.cmp(&right.value));
            }
        }
        if reused_version_id.is_none() {
            next_manifest.versions.push(SourceVersion {
                version_id: version_id.clone(),
                content_hash: input.content_hash.clone(),
                raw_evidence: vec![SourceArtifactRecord {
                    path: raw_path.clone(),
                    sha256: input.content_hash.clone(),
                    size_bytes: 0,
                    kind: "source_snapshot".into(),
                }],
                assets: Vec::new(),
                baseline_path: baseline_path.clone(),
                candidate: SourceCandidateRecord {
                    markdown_hash: input.candidate_markdown_hash.clone(),
                    title: input.title.clone(),
                    source_kind: input.source_kind.clone(),
                    canonical_url: input.canonical_url.clone(),
                    platform: input.platform.clone(),
                    platform_content_id: input.platform_content_id.clone(),
                    author: input.author.clone(),
                    published_at: input.published_at.clone(),
                    language: input.language.clone(),
                },
                provenance: SourceProvenance {
                    locator: locator.clone(),
                    route: input.route.clone(),
                    engine_id: input.engine_id.clone(),
                    engine_version: input.engine_version.clone(),
                },
                quality: input.quality.clone(),
                created_at: created_at.clone(),
                human_edit_hash: None,
                checkpoint: None,
            });
            next_manifest.current_version_id = version_id.clone();
            next_manifest.timeline.push(SourceTimelineEvent {
                event_id: uuid::Uuid::new_v4().to_string(),
                kind: if existing.is_some() {
                    "version_added"
                } else {
                    "imported"
                }
                .into(),
                version_id: Some(version_id.clone()),
                created_at,
                checkpoint: None,
            });
        }
        let (raw_path, evidence_root_path, baseline_path) = if reused_version_id.is_some() {
            let version = next_manifest
                .versions
                .iter()
                .find(|version| version.version_id == version_id)
                .ok_or_else(invalid_index)?;
            let raw_path = version
                .raw_evidence
                .first()
                .map(|artifact| artifact.path.clone())
                .ok_or_else(invalid_index)?;
            let evidence_root = Path::new(&raw_path)
                .parent()
                .and_then(Path::to_str)
                .map(|path| path.replace('\\', "/"))
                .ok_or_else(invalid_index)?;
            (raw_path, evidence_root, version.baseline_path.clone())
        } else {
            (raw_path, evidence_root_path, baseline_path)
        };

        let pointer = SourcePointer {
            source_id: source_id.clone(),
            version_id: version_id.clone(),
        };
        let mut next_index = index.clone();
        next_index.schema_version = SOURCE_REGISTRY_SCHEMA_VERSION;
        next_index
            .by_content_hash
            .insert(input.content_hash.clone(), pointer.clone());
        next_index.by_locator.insert(locator, pointer.clone());
        if let Some(canonical_url) = input
            .canonical_url
            .as_deref()
            .map(normalize_locator)
            .filter(|canonical_url| !canonical_url.is_empty())
        {
            next_index.by_locator.insert(canonical_url, pointer);
        }

        let asset_root_path = format!("raw/assets/{source_id}/{version_id}");
        let plan = SourceCommitPlan {
            resolution,
            source_id: source_id.clone(),
            version_id,
            previous_current_version_id,
            raw_path,
            evidence_root_path,
            asset_root_path,
            baseline_path,
            wiki_path,
            manifest_path: format!(".app/sources/{source_id}.json"),
            next_manifest,
            next_index,
        };
        validate_commit_plan(&plan)?;
        Ok(plan)
    }
}

fn manifest_paths_for_migration(context: &ProjectContext) -> Result<Vec<PathBuf>, BackendError> {
    let root = context.resolve_project_path(".app/sources")?;
    let metadata = match std::fs::symlink_metadata(&root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(invalid_index()),
    };
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || is_project_reparse_point(&metadata)
    {
        return Err(invalid_index());
    }
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(&root).map_err(|_| invalid_index())? {
        let entry = entry.map_err(|_| invalid_index())?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path).map_err(|_| invalid_index())?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || is_project_reparse_point(&metadata)
            || path.extension().and_then(|value| value.to_str()) != Some("json")
        {
            return Err(invalid_index());
        }
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(invalid_index)?;
        if !is_safe_id(stem) {
            return Err(invalid_index());
        }
        paths.push(path);
    }
    paths.sort();
    Ok(paths)
}

fn pretty_json_bytes(value: &impl Serialize) -> Result<Vec<u8>, BackendError> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|_| invalid_index())?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn migrate_legacy_manifest(
    context: &ProjectContext,
    files: &FileStore,
    legacy: LegacySourceManifest,
) -> Result<SourceManifest, BackendError> {
    if legacy.schema_version != LEGACY_SOURCE_REGISTRY_SCHEMA_VERSION {
        return Err(invalid_index());
    }
    let imported_at = legacy
        .versions
        .first()
        .map(|version| version.created_at.clone())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    let canonical_url = legacy
        .origins
        .iter()
        .find(|origin| url::Url::parse(origin).is_ok_and(|url| url.has_host()))
        .cloned();
    let source_kind = if canonical_url.is_some()
        || legacy.wiki_path.starts_with("wiki/sources/web/")
        || legacy.wiki_path.starts_with("wiki/sources/video/")
    {
        "web_page"
    } else {
        "local_document"
    }
    .to_string();
    let title = Path::new(&legacy.wiki_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Source")
        .to_string();
    let aliases = legacy
        .origins
        .iter()
        .map(|origin| SourceAlias {
            kind: if url::Url::parse(origin).is_ok_and(|url| url.has_host()) {
                "url"
            } else {
                "locator"
            }
            .into(),
            value: origin.clone(),
            created_at: imported_at.clone(),
        })
        .collect();
    let mut versions = Vec::with_capacity(legacy.versions.len());
    for version in legacy.versions {
        let raw_evidence = vec![artifact_record_or_fallback(
            context,
            files,
            &version.raw_path,
            &version.content_hash,
            "source_snapshot",
        )?];
        let assets = artifact_records_under(
            context,
            &format!("raw/assets/{}/{}", legacy.source_id, version.version_id),
            "asset",
        )?;
        let human_edit_hash = files
            .exists(context, &legacy.wiki_path)
            .then(|| files.file_hash(context, &legacy.wiki_path))
            .transpose()?;
        versions.push(SourceVersion {
            version_id: version.version_id,
            content_hash: version.content_hash.clone(),
            raw_evidence,
            assets,
            baseline_path: version.baseline_path,
            candidate: SourceCandidateRecord {
                markdown_hash: version.content_hash,
                title: title.clone(),
                source_kind: source_kind.clone(),
                canonical_url: canonical_url.clone(),
                platform: None,
                platform_content_id: None,
                author: None,
                published_at: None,
                language: None,
            },
            provenance: SourceProvenance {
                locator: legacy.origins.first().cloned().unwrap_or_default(),
                route: version.route,
                engine_id: version.engine_id,
                engine_version: version.engine_version,
            },
            quality: version.quality,
            created_at: version.created_at,
            human_edit_hash,
            checkpoint: None,
        });
    }
    let timeline = versions
        .iter()
        .map(|version| SourceTimelineEvent {
            event_id: format!("legacy-import-{}", version.version_id),
            kind: "imported".into(),
            version_id: Some(version.version_id.clone()),
            created_at: version.created_at.clone(),
            checkpoint: None,
        })
        .collect();
    let manifest = SourceManifest {
        schema_version: SOURCE_REGISTRY_SCHEMA_VERSION,
        source_id: legacy.source_id,
        source_kind,
        current_version_id: legacy.current_version_id,
        wiki_path: legacy.wiki_path,
        aliases,
        origins: legacy.origins,
        canonical_url,
        platform: None,
        platform_content_id: None,
        title,
        author: None,
        published_at: None,
        imported_at,
        language: None,
        versions,
        compiled_consumptions: Vec::new(),
        restricted_content: false,
        restricted_identity_summary: None,
        timeline,
    };
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn artifact_record_or_fallback(
    context: &ProjectContext,
    files: &FileStore,
    path: &str,
    fallback_hash: &str,
    kind: &str,
) -> Result<SourceArtifactRecord, BackendError> {
    let absolute = context.resolve_project_path(path)?;
    let (sha256, size_bytes) = if absolute.is_file() {
        (
            files.file_hash(context, path)?,
            std::fs::metadata(&absolute)
                .map_err(|_| invalid_index())?
                .len(),
        )
    } else {
        (fallback_hash.to_string(), 0)
    };
    Ok(SourceArtifactRecord {
        path: normalize_project_path(path),
        sha256,
        size_bytes,
        kind: kind.into(),
    })
}

fn artifact_records_under(
    context: &ProjectContext,
    root: &str,
    kind: &str,
) -> Result<Vec<SourceArtifactRecord>, BackendError> {
    let absolute = context.resolve_project_path(root)?;
    if !absolute.is_dir() {
        return Ok(Vec::new());
    }
    let mut pending = vec![absolute];
    let mut records = Vec::new();
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(&directory).map_err(|_| invalid_index())?;
        for entry in entries {
            let entry = entry.map_err(|_| invalid_index())?;
            let path = entry.path();
            let metadata = entry.metadata().map_err(|_| invalid_index())?;
            if metadata.is_dir() {
                pending.push(path);
            } else if metadata.is_file() {
                let relative = path
                    .strip_prefix(&context.root)
                    .map_err(|_| invalid_index())?
                    .to_string_lossy()
                    .replace('\\', "/");
                let bytes = std::fs::read(&path).map_err(|_| invalid_index())?;
                records.push(SourceArtifactRecord {
                    path: relative,
                    sha256: format!("{:x}", sha2::Sha256::digest(&bytes)),
                    size_bytes: metadata.len(),
                    kind: kind.into(),
                });
            }
        }
    }
    records.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(records)
}

fn resolve_manifest_asset_path(
    context: &ProjectContext,
    manifest: &SourceManifest,
    asset_relative: &str,
) -> Result<Option<PathBuf>, BackendError> {
    let raw_asset_path = format!(
        "raw/assets/{}/{}/{asset_relative}",
        manifest.source_id, manifest.current_version_id
    );
    let resolved = context.resolve_project_path(&raw_asset_path)?;
    if resolved.is_file() {
        return Ok(Some(resolved));
    }

    // Import V2 originally nested assets below raw/sources. Keep this
    // read-only fallback so existing projects remain renderable after new
    // imports move to the canonical raw/assets tree.
    let legacy_asset_path = format!(
        "raw/sources/{}/{}/assets/{asset_relative}",
        manifest.source_id, manifest.current_version_id
    );
    let legacy_resolved = context.resolve_project_path(&legacy_asset_path)?;
    Ok(legacy_resolved.is_file().then_some(legacy_resolved))
}

fn persisted_registry_schema(
    context: &ProjectContext,
    relative_path: &str,
) -> Result<Option<u32>, BackendError> {
    let absolute = context.resolve_project_path(relative_path)?;
    if !absolute.is_file() {
        return Ok(None);
    }
    let bytes =
        read_project_file_nofollow(&context.root, &absolute).map_err(|_| invalid_index())?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| invalid_index())?;
    let version = value
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
        .and_then(|version| u32::try_from(version).ok())
        .ok_or_else(invalid_index)?;
    Ok(Some(version))
}

fn current_version_has_legacy_migration_marker(manifest: &SourceManifest) -> bool {
    let current_version_id = manifest.current_version_id.as_str();
    let expected_event_id = format!("legacy-import-{current_version_id}");
    manifest.timeline.iter().any(|event| {
        event.event_id == expected_event_id
            && event.kind == "imported"
            && event.version_id.as_deref() == Some(current_version_id)
    })
}

fn wiki_asset_not_found(wiki_path: &str, asset_path: &str) -> BackendError {
    BackendError::new(
        "WIKI_ASSET_NOT_FOUND",
        "The Wiki image asset could not be resolved for this page.",
        true,
        false,
    )
    .with_details(serde_json::json!({
        "wikiPath": wiki_path,
        "assetPath": asset_path,
    }))
}

fn validate_index(index: &SourceIndex) -> Result<(), BackendError> {
    let known_pointers: HashSet<(&str, &str)> = index
        .by_content_hash
        .values()
        .map(|pointer| (pointer.source_id.as_str(), pointer.version_id.as_str()))
        .collect();
    if index.schema_version != SOURCE_REGISTRY_SCHEMA_VERSION
        || index
            .by_content_hash
            .iter()
            .any(|(hash, pointer)| hash.trim().is_empty() || invalid_pointer(pointer))
        || index.by_locator.iter().any(|(locator, pointer)| {
            locator.trim().is_empty()
                || normalize_locator(locator) != *locator
                || invalid_pointer(pointer)
                || !known_pointers
                    .contains(&(pointer.source_id.as_str(), pointer.version_id.as_str()))
        })
    {
        return Err(invalid_index());
    }
    Ok(())
}

pub(crate) fn validate_for_migration(index: &SourceIndex) -> Result<(), BackendError> {
    validate_index(index)
}

fn invalid_pointer(pointer: &SourcePointer) -> bool {
    !is_safe_id(&pointer.source_id) || !is_safe_id(&pointer.version_id)
}

fn invalid_index() -> BackendError {
    BackendError::new(
        IMPORT_V2_SOURCE_INDEX_INVALID,
        "The import v2 source index is missing required data or is inconsistent.",
        false,
        true,
    )
}

fn validate_identity(value: &str) -> Result<(), BackendError> {
    if is_safe_id(value) {
        Ok(())
    } else {
        Err(compile_source_version_changed())
    }
}

fn compile_source_version_changed() -> BackendError {
    BackendError::new(
        "COMPILE_SOURCE_VERSION_INVALID",
        "A selected Source version is missing or its content hash no longer matches.",
        true,
        true,
    )
}

fn normalize_locator(locator: &str) -> String {
    locator.trim().replace('\\', "/")
}

fn validate_manifest(manifest: &SourceManifest) -> Result<(), BackendError> {
    let paths = ProjectContext::new("source-manifest-validation", std::path::PathBuf::from("."));
    let current_count = manifest
        .versions
        .iter()
        .filter(|version| version.version_id == manifest.current_version_id)
        .count();
    let mut version_ids = std::collections::BTreeSet::new();
    let invalid_version = manifest.versions.iter().any(|version| {
        !is_safe_id(&version.version_id)
            || !version_ids.insert(&version.version_id)
            || !is_sha256(&version.content_hash)
            || version.raw_evidence.is_empty()
            || version.raw_evidence.iter().any(|artifact| {
                !valid_evidence_path(&artifact.path, &manifest.source_id, &version.version_id)
                    || !is_sha256(&artifact.sha256)
                    || artifact.kind.trim().is_empty()
                    || paths.resolve_project_path(&artifact.path).is_err()
            })
            || version.assets.iter().any(|artifact| {
                !valid_asset_path(&artifact.path, &manifest.source_id, &version.version_id)
                    || !is_sha256(&artifact.sha256)
                    || artifact.kind.trim().is_empty()
                    || paths.resolve_project_path(&artifact.path).is_err()
            })
            || version.baseline_path
                != format!(
                    ".app/source-artifacts/{}/{}/baseline.md",
                    manifest.source_id, version.version_id
                )
            || paths.resolve_project_path(&version.baseline_path).is_err()
            || !is_sha256(&version.candidate.markdown_hash)
            || version.candidate.title.trim().is_empty()
            || version.candidate.source_kind.trim().is_empty()
            || version.provenance.locator.trim().is_empty()
            || version.provenance.route.trim().is_empty()
            || version.provenance.engine_id.trim().is_empty()
            || version.provenance.engine_version.trim().is_empty()
            || version.created_at.trim().is_empty()
            || version
                .human_edit_hash
                .as_deref()
                .is_some_and(|hash| !is_sha256(hash))
    });
    let aliases_invalid = manifest.aliases.iter().any(|alias| {
        alias.kind.trim().is_empty()
            || alias.value.trim().is_empty()
            || alias.created_at.trim().is_empty()
    });
    let consumptions_invalid = manifest.compiled_consumptions.iter().any(|consumption| {
        !is_safe_id(&consumption.version_id)
            || !is_sha256(&consumption.content_hash)
            || consumption.compile_task_id.trim().is_empty()
            || consumption.consumed_at.trim().is_empty()
    });
    let timeline_invalid = manifest.timeline.iter().any(|event| {
        !is_safe_id(&event.event_id)
            || event.kind.trim().is_empty()
            || event.created_at.trim().is_empty()
            || event
                .version_id
                .as_deref()
                .is_some_and(|version_id| !is_safe_id(version_id))
    });
    if manifest.schema_version != SOURCE_REGISTRY_SCHEMA_VERSION
        || !is_safe_id(&manifest.source_id)
        || manifest.source_kind.trim().is_empty()
        || manifest.title.trim().is_empty()
        || manifest.imported_at.trim().is_empty()
        || current_count != 1
        || invalid_version
        || aliases_invalid
        || manifest
            .origins
            .iter()
            .any(|origin| origin.trim().is_empty())
        || consumptions_invalid
        || timeline_invalid
        || !valid_wiki_path(&manifest.wiki_path)
        || paths.resolve_project_path(&manifest.wiki_path).is_err()
    {
        return Err(invalid_index());
    }
    Ok(())
}

fn validate_commit_plan(plan: &SourceCommitPlan) -> Result<(), BackendError> {
    let paths = ProjectContext::new("source-plan-validation", std::path::PathBuf::from("."));
    let manifest_path = format!(".app/sources/{}.json", plan.source_id);
    if !is_safe_id(&plan.source_id)
        || !is_safe_id(&plan.version_id)
        || !valid_raw_path(&plan.raw_path, &plan.source_id, &plan.version_id)
        || !valid_evidence_root(&plan.evidence_root_path, &plan.source_id, &plan.version_id)
        || plan.asset_root_path != format!("raw/assets/{}/{}", plan.source_id, plan.version_id)
        || plan.baseline_path
            != format!(
                ".app/source-artifacts/{}/{}/baseline.md",
                plan.source_id, plan.version_id
            )
        || !valid_wiki_path(&plan.wiki_path)
        || plan.manifest_path != manifest_path
        || [
            &plan.raw_path,
            &plan.evidence_root_path,
            &plan.asset_root_path,
            &plan.baseline_path,
            &plan.wiki_path,
            &plan.manifest_path,
        ]
        .iter()
        .any(|path| paths.resolve_project_path(path).is_err())
    {
        return Err(invalid_index());
    }
    Ok(())
}

fn is_safe_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

fn valid_raw_path(path: &str, source_id: &str, version_id: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    parts.len() == 5
        && parts[0] == "raw"
        && matches!(parts[1], "sources" | "web")
        && parts[2] == source_id
        && parts[3] == version_id
        && ((parts[1] == "sources" && parts[4].starts_with("original."))
            || (parts[1] == "web" && parts[4].starts_with("snapshot.")))
        && safe_extension(
            parts[4]
                .trim_start_matches("original.")
                .trim_start_matches("snapshot."),
        ) == parts[4]
            .trim_start_matches("original.")
            .trim_start_matches("snapshot.")
}

fn valid_evidence_root(path: &str, source_id: &str, version_id: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    parts.len() == 4
        && parts[0] == "raw"
        && matches!(parts[1], "sources" | "web")
        && parts[2] == source_id
        && parts[3] == version_id
}

fn valid_evidence_path(path: &str, source_id: &str, version_id: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    parts.len() >= 5
        && parts[0] == "raw"
        && matches!(parts[1], "sources" | "web")
        && parts[2] == source_id
        && parts[3] == version_id
        && safe_path_tail(&parts[4..])
}

fn valid_asset_path(path: &str, source_id: &str, version_id: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    parts.len() >= 5
        && parts[0] == "raw"
        && parts[1] == "assets"
        && parts[2] == source_id
        && parts[3] == version_id
        && safe_path_tail(&parts[4..])
}

fn safe_path_tail(parts: &[&str]) -> bool {
    !parts.is_empty()
        && parts.iter().all(|part| {
            !part.is_empty()
                && *part != "."
                && *part != ".."
                && !part.contains(['\\', ':'])
                && !part.chars().any(char::is_control)
        })
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_wiki_path(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    let filename = parts.last().copied().unwrap_or_default();
    let stem = filename.strip_suffix(".md").unwrap_or_default();
    let canonical_local = parts.len() == 4 && parts.get(2) == Some(&"local");
    let canonical_local_package = parts.len() == 5
        && parts.get(2) == Some(&"local")
        && parts.get(4) == Some(&"index.md")
        && parts
            .get(3)
            .is_some_and(|directory| portable_wiki_stem(directory) == *directory);
    let canonical_web = parts.len() == 5
        && parts.get(2) == Some(&"web")
        && parts.get(3).is_some_and(|host| valid_normalized_host(host));
    let legacy = parts.len() == 4
        && parts
            .get(2)
            .is_some_and(|category| matches!(*category, "files" | "web" | "video"));
    (canonical_local || canonical_local_package || canonical_web || legacy)
        && parts[0] == "wiki"
        && parts[1] == "sources"
        && !stem.is_empty()
        && portable_wiki_stem(stem) == stem
}

fn valid_normalized_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && !host.starts_with('.')
        && !host.ends_with('.')
        && host
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-'))
}

fn safe_extension(extension: &str) -> String {
    let extension = extension.trim().trim_start_matches('.').to_lowercase();
    if !extension.is_empty()
        && extension.len() <= 16
        && extension
            .split('.')
            .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_alphanumeric()))
    {
        extension
    } else {
        "bin".into()
    }
}

pub(crate) fn derive_wiki_path_for_input(
    input_kind: &ImportInputKind,
    display_name: &str,
    normalized_locator: &str,
    canonical_url: Option<&str>,
) -> String {
    let stem = display_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("source")
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(display_name);
    let slug = portable_wiki_stem(stem);
    match input_kind {
        ImportInputKind::File | ImportInputKind::Folder | ImportInputKind::ClipboardText => {
            format!("wiki/sources/local/{slug}.md")
        }
        ImportInputKind::Url => {
            let host = canonical_url
                .and_then(normalized_web_host)
                .or_else(|| normalized_web_host(normalized_locator))
                .unwrap_or_else(|| "unknown-host".into());
            format!("wiki/sources/web/{host}/{slug}.md")
        }
    }
}

fn derive_wiki_path(input: &SourceCommitInput) -> String {
    derive_wiki_path_for_input(
        &input.input_kind,
        &input.display_name,
        &input.normalized_locator,
        input.canonical_url.as_deref(),
    )
}

pub fn normalized_web_host(value: &str) -> Option<String> {
    let parsed = url::Url::parse(value).ok()?;
    let mut host = parsed
        .host_str()?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    if let Some(port) = parsed.port() {
        host.push_str(&format!("-port-{port}"));
    }
    valid_normalized_host(&host).then_some(host)
}

fn portable_wiki_stem(stem: &str) -> String {
    let slug: String = stem
        .trim()
        .nfc()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            ch if ch.is_control() => '-',
            ch => ch,
        })
        .collect();
    let slug = slug.trim_matches([' ', '.', '-']);
    let slug = if slug.is_empty() { "source" } else { slug };
    let upper = slug.to_ascii_uppercase();
    let device_basename = upper.split('.').next().unwrap_or_default();
    let reserved = matches!(device_basename, "CON" | "PRN" | "AUX" | "NUL")
        || (device_basename.len() == 4
            && (device_basename.starts_with("COM") || device_basename.starts_with("LPT"))
            && device_basename.as_bytes()[3].is_ascii_digit()
            && device_basename.as_bytes()[3] != b'0');
    let portable = if reserved {
        format!("source-{slug}")
    } else {
        slug.to_string()
    };
    let mut bounded = String::new();
    for ch in portable.chars() {
        if bounded.len() + ch.len_utf8() > 120 {
            break;
        }
        bounded.push(ch);
    }
    if bounded.is_empty() {
        "source"
    } else {
        &bounded
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::IMPORT_V2_SOURCE_INDEX_INVALID;
    use crate::models::compile::{CompileConsumptionRecord, CompileRoute, SourceVersionRef};
    use crate::models::import_v2::{
        ImportInputKind, QualityLevel, QualityReport, SourceFrontmatter, SourcePageType,
    };
    use crate::services::import_v2::source_finalization::{
        finalize_source, render_source_markdown, CandidateMetadata, FinalizationInput,
    };
    use crate::services::FileStore;
    use std::collections::BTreeMap;

    fn pass_quality() -> QualityReport {
        QualityReport {
            level: QualityLevel::Pass,
            metrics: Vec::new(),
            warnings: Vec::new(),
            sheet_count_exact: None,
            slide_count_exact: None,
            non_empty_cell_coverage: None,
            formula_value_pairs: None,
            meaningful_image_coverage: None,
        }
    }

    #[test]
    fn preview_and_commit_share_portable_targets_for_new_local_and_web_sources() {
        assert_eq!(
            derive_wiki_path_for_input(
                &ImportInputKind::File,
                "研究笔记.docx",
                r"file:d:\资料\研究笔记.docx",
                None,
            ),
            "wiki/sources/local/研究笔记.md"
        );
        assert_eq!(
            derive_wiki_path_for_input(
                &ImportInputKind::Url,
                "文章标题",
                "https://Example.COM./article/1",
                Some("https://Example.COM./article/1"),
            ),
            "wiki/sources/web/example.com/文章标题.md"
        );
    }

    fn fixture_version(version_id: &str, content_hash: &str) -> SourceVersion {
        let content_hash = format!("{:x}", sha2::Sha256::digest(content_hash.as_bytes()));
        SourceVersion {
            version_id: version_id.into(),
            content_hash: content_hash.clone(),
            raw_evidence: vec![SourceArtifactRecord {
                path: format!("raw/sources/source-1/{version_id}/original.docx"),
                sha256: content_hash.clone(),
                size_bytes: 1,
                kind: "source_snapshot".into(),
            }],
            assets: Vec::new(),
            baseline_path: format!(".app/source-artifacts/source-1/{version_id}/baseline.md"),
            candidate: SourceCandidateRecord {
                markdown_hash: content_hash,
                title: "Fixture".into(),
                source_kind: "local_document".into(),
                canonical_url: None,
                platform: None,
                platform_content_id: None,
                author: None,
                published_at: None,
                language: None,
            },
            provenance: SourceProvenance {
                locator: "file:/fixture.docx".into(),
                route: "fixture".into(),
                engine_id: "fixture".into(),
                engine_version: "1.0.0".into(),
            },
            quality: pass_quality(),
            created_at: "2026-07-11T00:00:00Z".into(),
            human_edit_hash: None,
            checkpoint: None,
        }
    }

    fn fixture_manifest() -> SourceManifest {
        SourceManifest {
            schema_version: SOURCE_REGISTRY_SCHEMA_VERSION,
            source_id: "source-1".into(),
            source_kind: "local_document".into(),
            current_version_id: "version-1".into(),
            wiki_path: "wiki/sources/local/a.md".into(),
            aliases: Vec::new(),
            origins: vec!["file:/a.docx".into()],
            canonical_url: None,
            platform: None,
            platform_content_id: None,
            title: "Fixture".into(),
            author: None,
            published_at: None,
            imported_at: "2026-07-11T00:00:00Z".into(),
            language: None,
            versions: vec![fixture_version("version-1", "hash-a")],
            compiled_consumptions: Vec::new(),
            restricted_content: false,
            restricted_identity_summary: None,
            timeline: Vec::new(),
        }
    }

    fn persist_compile_fixture(
        context: &ProjectContext,
        files: &FileStore,
    ) -> (SourceManifest, SourceVersionRef) {
        let mut manifest = fixture_manifest();
        let version = manifest.versions.first().unwrap().clone();
        let candidate = CandidateMetadata {
            source_kind: version.candidate.source_kind.clone(),
            title: version.candidate.title.clone(),
            canonical_url: version.candidate.canonical_url.clone(),
            platform: version.candidate.platform.clone(),
            platform_content_id: version.candidate.platform_content_id.clone(),
            author: version.candidate.author.clone(),
            published_at: version.candidate.published_at.clone(),
            language: version.candidate.language.clone(),
        };
        let finalized = finalize_source(FinalizationInput {
            candidate_markdown: b"# Fixture\n\nBound source body.",
            candidate: &candidate,
            source_id: &manifest.source_id,
            version_id: &version.version_id,
            content_hash: &version.content_hash,
            imported_at: &version.created_at,
            quality: &version.quality,
            restricted: manifest.restricted_content,
        })
        .unwrap();
        manifest.versions[0].human_edit_hash = Some(finalized.human_edit_hash);
        for relative in [&manifest.wiki_path, &version.baseline_path] {
            let absolute = context.resolve_project_path(relative).unwrap();
            std::fs::create_dir_all(absolute.parent().unwrap()).unwrap();
            std::fs::write(absolute, &finalized.bytes).unwrap();
        }
        files
            .write_json_atomic(context, ".app/sources/source-1.json", &manifest)
            .unwrap();
        let reference = SourceVersionRef {
            source_id: manifest.source_id.clone(),
            version_id: version.version_id,
            content_hash: version.content_hash,
        };
        let pointer = SourcePointer {
            source_id: reference.source_id.clone(),
            version_id: reference.version_id.clone(),
        };
        files
            .write_json_atomic(
                context,
                SOURCE_INDEX_PATH,
                &SourceIndex {
                    schema_version: SOURCE_REGISTRY_SCHEMA_VERSION,
                    by_content_hash: BTreeMap::from([(
                        reference.content_hash.clone(),
                        pointer.clone(),
                    )]),
                    by_locator: BTreeMap::from([("file:/a.docx".into(), pointer)]),
                },
            )
            .unwrap();
        (manifest, reference)
    }

    fn fixture_input(locator: &str, hash: &str, name: &str) -> SourceCommitInput {
        SourceCommitInput {
            normalized_locator: locator.into(),
            content_hash: hash.into(),
            display_name: name.into(),
            input_kind: ImportInputKind::File,
            source_extension: "docx".into(),
            source_kind: "local_document".into(),
            canonical_url: None,
            platform: None,
            platform_content_id: None,
            title: name.into(),
            author: None,
            published_at: None,
            imported_at: "2026-07-25T00:00:00Z".into(),
            language: Some("zh-CN".into()),
            candidate_markdown_hash: format!("{:x}", sha2::Sha256::digest(name.as_bytes())),
            route: "fixture".into(),
            engine_id: "fixture".into(),
            engine_version: "1.0.0".into(),
            quality: pass_quality(),
        }
    }

    #[test]
    fn shared_manifest_v3_fixture_freezes_the_complete_schema() {
        let manifest: SourceManifest = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/import-v2/source-manifest-v3.json"
        ))
        .unwrap();

        assert_eq!(manifest.schema_version, SOURCE_REGISTRY_SCHEMA_VERSION);
        SourceRegistry::validate_manifest_contract(&manifest).unwrap();
        let round_trip: SourceManifest =
            serde_json::from_value(serde_json::to_value(&manifest).unwrap()).unwrap();
        assert_eq!(round_trip, manifest);
    }

    #[test]
    fn legacy_registry_migrates_atomically_once_and_adapter_remains_compatible() {
        let (context, root) = super::super::test_support::test_context("legacy-manifest-v2");
        let files = FileStore;
        files
            .write_markdown(
                &context,
                ".app/sources/source-legacy.json",
                include_str!("../../../../tests/fixtures/import-v2/legacy-source-manifest-v2.json"),
            )
            .unwrap();
        files
            .write_json_atomic(
                &context,
                SOURCE_INDEX_PATH,
                &SourceIndex {
                    schema_version: LEGACY_SOURCE_REGISTRY_SCHEMA_VERSION,
                    by_content_hash: BTreeMap::from([(
                        "a".repeat(64),
                        SourcePointer {
                            source_id: "source-legacy".into(),
                            version_id: "version-legacy".into(),
                        },
                    )]),
                    by_locator: BTreeMap::from([(
                        "file:/fixtures/legacy.md".into(),
                        SourcePointer {
                            source_id: "source-legacy".into(),
                            version_id: "version-legacy".into(),
                        },
                    )]),
                },
            )
            .unwrap();
        files
            .write_markdown(
                &context,
                "raw/sources/source-legacy/version-legacy/original.md",
                "# Legacy raw\n",
            )
            .unwrap();
        files
            .write_markdown(
                &context,
                ".app/source-artifacts/source-legacy/version-legacy/baseline.md",
                "# Legacy Source\n",
            )
            .unwrap();
        files
            .write_markdown(
                &context,
                "wiki/sources/files/legacy.md",
                "# Legacy Source\n",
            )
            .unwrap();

        let manifest =
            SourceRegistry::read_manifest(&context, &files, ".app/sources/source-legacy.json")
                .unwrap();
        assert_eq!(manifest.schema_version, SOURCE_REGISTRY_SCHEMA_VERSION);
        assert_eq!(manifest.source_id, "source-legacy");
        assert_eq!(manifest.versions.len(), 1);
        assert!(!manifest.versions[0].raw_evidence.is_empty());
        SourceRegistry::validate_manifest_contract(&manifest).unwrap();
        let stored: serde_json::Value = files
            .read_json(&context, ".app/sources/source-legacy.json")
            .unwrap();
        assert_eq!(stored["schemaVersion"], 2);
        assert!(SourceRegistry::migrate_project_v3(&context, &files).unwrap());
        let manifest_bytes = std::fs::read(root.join(".app/sources/source-legacy.json")).unwrap();
        let index_bytes = std::fs::read(root.join(SOURCE_INDEX_PATH)).unwrap();
        let stored: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
        let stored_index: serde_json::Value = serde_json::from_slice(&index_bytes).unwrap();
        assert_eq!(stored["schemaVersion"], 3);
        assert_eq!(stored_index["schemaVersion"], 3);
        assert!(!SourceRegistry::migrate_project_v3(&context, &files).unwrap());
        assert_eq!(
            std::fs::read(root.join(".app/sources/source-legacy.json")).unwrap(),
            manifest_bytes
        );
        assert_eq!(
            std::fs::read(root.join(SOURCE_INDEX_PATH)).unwrap(),
            index_bytes
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_registry_concurrent_change_rolls_back_without_half_v3_state() {
        let (context, root) = super::super::test_support::test_context("legacy-v3-rollback");
        let files = FileStore;
        let manifest_path = ".app/sources/source-legacy.json";
        files
            .write_markdown(
                &context,
                manifest_path,
                include_str!("../../../../tests/fixtures/import-v2/legacy-source-manifest-v2.json"),
            )
            .unwrap();
        for (path, body) in [
            (
                "raw/sources/source-legacy/version-legacy/original.md",
                "# Legacy raw\n",
            ),
            (
                ".app/source-artifacts/source-legacy/version-legacy/baseline.md",
                "# Legacy Source\n",
            ),
            ("wiki/sources/files/legacy.md", "# Legacy Source\n"),
        ] {
            files.write_markdown(&context, path, body).unwrap();
        }
        files
            .write_json_atomic(
                &context,
                SOURCE_INDEX_PATH,
                &SourceIndex {
                    schema_version: LEGACY_SOURCE_REGISTRY_SCHEMA_VERSION,
                    by_content_hash: BTreeMap::from([(
                        "a".repeat(64),
                        SourcePointer {
                            source_id: "source-legacy".into(),
                            version_id: "version-legacy".into(),
                        },
                    )]),
                    by_locator: BTreeMap::from([(
                        "file:/fixtures/legacy.md".into(),
                        SourcePointer {
                            source_id: "source-legacy".into(),
                            version_id: "version-legacy".into(),
                        },
                    )]),
                },
            )
            .unwrap();
        let legacy_index = std::fs::read(root.join(SOURCE_INDEX_PATH)).unwrap();
        let manifest_absolute =
            root.join(manifest_path.replace('/', std::path::MAIN_SEPARATOR_STR));
        let manifest_for_hook = manifest_absolute.clone();
        super::super::transaction::set_before_checked_displace_hook(move |path| {
            if path == manifest_for_hook {
                let mut bytes = std::fs::read(path).unwrap();
                bytes.extend_from_slice(b"\n");
                std::fs::write(path, bytes).unwrap();
                true
            } else {
                false
            }
        });
        assert!(SourceRegistry::migrate_project_v3(&context, &files).is_err());
        let stored_index: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join(SOURCE_INDEX_PATH)).unwrap()).unwrap();
        let stored_manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&manifest_absolute).unwrap()).unwrap();
        assert_eq!(stored_index["schemaVersion"], 2);
        assert_eq!(stored_manifest["schemaVersion"], 2);
        assert_eq!(
            std::fs::read(root.join(SOURCE_INDEX_PATH)).unwrap(),
            legacy_index
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn source_resolution_distinguishes_new_duplicate_update_and_alias() {
        let pointer = SourcePointer {
            source_id: "source-1".into(),
            version_id: "version-1".into(),
        };
        let index = SourceIndex {
            schema_version: 2,
            by_content_hash: BTreeMap::from([("hash-a".into(), pointer.clone())]),
            by_locator: BTreeMap::from([("file:d:/a.docx".into(), pointer)]),
        };
        assert_eq!(
            SourceRegistry::resolve(&index, "file:d:/new.docx", "hash-new"),
            SourceResolution::New
        );
        assert!(matches!(
            SourceRegistry::resolve(&index, "file:d:/a.docx", "hash-a"),
            SourceResolution::ExactDuplicate { .. }
        ));
        assert!(matches!(
            SourceRegistry::resolve(&index, "file:d:/a.docx", "hash-b"),
            SourceResolution::UpdatedOrigin { .. }
        ));
        assert!(matches!(
            SourceRegistry::resolve(&index, "file:d:/copy.docx", "hash-a"),
            SourceResolution::SameContentNewOrigin { .. }
        ));
    }

    #[test]
    fn case_sensitive_locators_remain_distinct_after_registry_reload() {
        let pointer = SourcePointer {
            source_id: "source-uppercase".into(),
            version_id: "version-uppercase".into(),
        };
        let index = SourceIndex {
            schema_version: SOURCE_REGISTRY_SCHEMA_VERSION,
            by_content_hash: BTreeMap::from([("hash-uppercase".into(), pointer.clone())]),
            by_locator: BTreeMap::from([("file:/workspace/资料/A.md".into(), pointer)]),
        };
        let reloaded: SourceIndex =
            serde_json::from_str(&serde_json::to_string(&index).unwrap()).unwrap();

        assert!(matches!(
            SourceRegistry::resolve(&reloaded, "file:/workspace/资料/A.md", "hash-changed"),
            SourceResolution::UpdatedOrigin { .. }
        ));
        assert_eq!(
            SourceRegistry::resolve(&reloaded, "file:/workspace/资料/a.md", "hash-lowercase"),
            SourceResolution::New
        );
    }

    #[test]
    fn historical_hash_for_same_source_is_an_exact_duplicate() {
        let index = SourceIndex {
            schema_version: 2,
            by_content_hash: BTreeMap::from([(
                "hash-old".into(),
                SourcePointer {
                    source_id: "source-1".into(),
                    version_id: "version-1".into(),
                },
            )]),
            by_locator: BTreeMap::from([(
                "file:/Docs/A.docx".into(),
                SourcePointer {
                    source_id: "source-1".into(),
                    version_id: "version-2".into(),
                },
            )]),
        };
        assert_eq!(
            SourceRegistry::resolve(&index, "file:/Docs/A.docx", "hash-old"),
            SourceResolution::ExactDuplicate {
                source_id: "source-1".into(),
                version_id: "version-1".into(),
            }
        );
    }

    #[test]
    fn normalized_locator_case_is_preserved() {
        let lower = fixture_input("https://example.test/Report", "hash-a", "Report");
        let plan = SourceRegistry
            .build_commit_plan(&SourceIndex::default_v2(), None, &lower)
            .unwrap();
        assert!(plan
            .next_index
            .by_locator
            .contains_key("https://example.test/Report"));
    }

    #[test]
    fn locator_update_cannot_steal_content_owned_by_another_source() {
        let index = SourceIndex {
            schema_version: 2,
            by_content_hash: BTreeMap::from([
                (
                    "hash-a".into(),
                    SourcePointer {
                        source_id: "source-1".into(),
                        version_id: "version-1".into(),
                    },
                ),
                (
                    "hash-b".into(),
                    SourcePointer {
                        source_id: "source-2".into(),
                        version_id: "version-2".into(),
                    },
                ),
            ]),
            by_locator: BTreeMap::from([
                (
                    "file:/a.docx".into(),
                    SourcePointer {
                        source_id: "source-1".into(),
                        version_id: "version-1".into(),
                    },
                ),
                (
                    "file:/b.docx".into(),
                    SourcePointer {
                        source_id: "source-2".into(),
                        version_id: "version-2".into(),
                    },
                ),
            ]),
        };
        let existing = SourceManifest {
            source_id: "source-1".into(),
            origins: vec!["file:/a.docx".into()],
            versions: vec![fixture_version("version-1", "hash-a")],
            current_version_id: "version-1".into(),
            wiki_path: "wiki/sources/local/a.md".into(),
            ..fixture_manifest()
        };
        let error = SourceRegistry
            .build_commit_plan(
                &index,
                Some(&existing),
                &fixture_input("file:/a.docx", "hash-b", "a.docx"),
            )
            .unwrap_err();
        assert_eq!(error.code, IMPORT_V2_SOURCE_INDEX_INVALID);
    }

    #[test]
    fn commit_plan_never_reuses_an_existing_raw_version_path() {
        let existing = SourceManifest {
            source_id: "source-1".into(),
            origins: vec!["file:d:/a.docx".into()],
            versions: vec![fixture_version("version-1", "hash-a")],
            current_version_id: "version-1".into(),
            wiki_path: "wiki/sources/local/a.md".into(),
            ..fixture_manifest()
        };
        let plan = SourceRegistry
            .build_commit_plan(
                &SourceIndex::default_v2(),
                Some(&existing),
                &fixture_input("file:d:/a.docx", "hash-b", "a.docx"),
            )
            .unwrap();
        assert!(plan.raw_path.starts_with("raw/sources/source-1/"));
        assert!(!plan.raw_path.contains("version-1/"));
        assert_eq!(plan.next_manifest.versions.len(), 2);
    }

    #[test]
    fn same_content_new_origin_adds_a_normalized_alias_without_a_new_version() {
        let content_hash = fixture_version("version-1", "hash-a").content_hash;
        let pointer = SourcePointer {
            source_id: "source-1".into(),
            version_id: "version-1".into(),
        };
        let index = SourceIndex {
            schema_version: SOURCE_REGISTRY_SCHEMA_VERSION,
            by_content_hash: BTreeMap::from([(content_hash.clone(), pointer.clone())]),
            by_locator: BTreeMap::from([("file:d:/a.docx".into(), pointer)]),
        };
        let existing = SourceManifest {
            source_id: "source-1".into(),
            origins: vec!["file:d:/a.docx".into()],
            versions: vec![fixture_version("version-1", "hash-a")],
            current_version_id: "version-1".into(),
            wiki_path: "wiki/sources/local/a.md".into(),
            ..fixture_manifest()
        };
        let plan = SourceRegistry
            .build_commit_plan(
                &index,
                Some(&existing),
                &fixture_input("file:d:/副本.docx", &content_hash, "副本.docx"),
            )
            .unwrap();
        assert_eq!(plan.version_id, "version-1");
        assert_eq!(plan.next_manifest.versions.len(), 1);
        assert!(plan
            .next_manifest
            .origins
            .contains(&"file:d:/副本.docx".to_string()));
        assert_eq!(
            plan.next_index.by_locator["file:d:/副本.docx"],
            SourcePointer {
                source_id: "source-1".into(),
                version_id: "version-1".into()
            }
        );
    }

    #[test]
    fn read_index_defaults_when_v2_is_missing_and_rejects_invalid_content() {
        let (context, root) = super::super::test_support::test_context("source-registry-index");
        let files = FileStore;
        assert_eq!(
            SourceRegistry::read_index(&context, &files).unwrap(),
            SourceIndex::default_v2()
        );

        files
            .write_markdown(&context, ".app/source-index-v2.json", "not-json")
            .unwrap();
        assert_eq!(
            SourceRegistry::read_index(&context, &files)
                .unwrap_err()
                .code,
            IMPORT_V2_SOURCE_INDEX_INVALID
        );
        files
            .write_markdown(
                &context,
                ".app/source-index-v2.json",
                r#"{"schemaVersion":1,"byContentHash":{},"byLocator":{}}"#,
            )
            .unwrap();
        assert_eq!(
            SourceRegistry::read_index(&context, &files)
                .unwrap_err()
                .code,
            IMPORT_V2_SOURCE_INDEX_INVALID
        );
        files
            .write_markdown(
                &context,
                ".app/source-index-v2.json",
                r#"{"schemaVersion":2,"byContentHash":{"hash-a":{"sourceId":"s1","versionId":"v1"}},"byLocator":{"file:/a":{"sourceId":"s1","versionId":"v1"},"file:/a":{"sourceId":"s1","versionId":"v1"}}}"#,
            )
            .unwrap();
        assert_eq!(
            SourceRegistry::read_index(&context, &files)
                .unwrap_err()
                .code,
            IMPORT_V2_SOURCE_INDEX_INVALID
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn index_validation_joins_many_locators_to_content_pointers_in_one_lookup_set() {
        let pointers: BTreeMap<String, SourcePointer> = (0..256)
            .map(|index| {
                (
                    format!("hash-{index}"),
                    SourcePointer {
                        source_id: format!("source-{index}"),
                        version_id: format!("version-{index}"),
                    },
                )
            })
            .collect();
        let locators: BTreeMap<String, SourcePointer> = (0..1_024)
            .map(|index| {
                (
                    format!("file:/source-{index}.md"),
                    pointers[&format!("hash-{}", index % pointers.len())].clone(),
                )
            })
            .collect();
        let mut index = SourceIndex {
            schema_version: SOURCE_REGISTRY_SCHEMA_VERSION,
            by_content_hash: pointers,
            by_locator: locators,
        };

        validate_index(&index).unwrap();

        index.by_locator.insert(
            "file:/orphan.md".into(),
            SourcePointer {
                source_id: "source-orphan".into(),
                version_id: "version-orphan".into(),
            },
        );
        assert_eq!(
            validate_index(&index).unwrap_err().code,
            IMPORT_V2_SOURCE_INDEX_INVALID
        );
    }

    #[test]
    fn corrupt_existing_manifest_is_rejected_before_paths_are_returned() {
        let existing = SourceManifest {
            schema_version: 1,
            source_id: "source-1".into(),
            origins: vec!["file:/a.docx".into()],
            versions: vec![fixture_version("version-1", "hash-a")],
            current_version_id: "version-1".into(),
            wiki_path: "../escape.md".into(),
            ..fixture_manifest()
        };
        let error = SourceRegistry
            .build_commit_plan(
                &SourceIndex::default_v2(),
                Some(&existing),
                &fixture_input("file:/a.docx", "hash-b", "a.docx"),
            )
            .unwrap_err();
        assert_eq!(error.code, IMPORT_V2_SOURCE_INDEX_INVALID);
    }

    #[test]
    fn source_id_must_be_one_safe_component_for_every_planned_target() {
        let pointer = SourcePointer {
            source_id: "nested/source".into(),
            version_id: "version-1".into(),
        };
        let index = SourceIndex {
            schema_version: 2,
            by_content_hash: BTreeMap::from([("hash-a".into(), pointer.clone())]),
            by_locator: BTreeMap::from([("file:/a.docx".into(), pointer)]),
        };
        let existing = SourceManifest {
            source_id: "nested/source".into(),
            origins: vec!["file:/a.docx".into()],
            versions: vec![SourceVersion {
                version_id: "version-1".into(),
                raw_evidence: vec![SourceArtifactRecord {
                    path: "raw/sources/nested/source/version-1/original.docx".into(),
                    ..fixture_version("version-1", "hash-a").raw_evidence[0].clone()
                }],
                baseline_path: ".app/source-artifacts/nested/source/version-1/baseline.md".into(),
                ..fixture_version("version-1", "hash-a")
            }],
            current_version_id: "version-1".into(),
            wiki_path: "wiki/sources/local/a.md".into(),
            ..fixture_manifest()
        };
        let error = SourceRegistry
            .build_commit_plan(
                &index,
                Some(&existing),
                &fixture_input("file:/a.docx", "hash-a", "a.docx"),
            )
            .unwrap_err();
        assert_eq!(error.code, IMPORT_V2_SOURCE_INDEX_INVALID);
    }

    #[test]
    fn version_id_and_artifact_paths_must_describe_the_same_version_directory() {
        let pointer = SourcePointer {
            source_id: "source-1".into(),
            version_id: "../escape".into(),
        };
        let index = SourceIndex {
            schema_version: 2,
            by_content_hash: BTreeMap::from([("hash-a".into(), pointer.clone())]),
            by_locator: BTreeMap::from([("file:/a.docx".into(), pointer)]),
        };
        let existing = SourceManifest {
            source_id: "source-1".into(),
            origins: vec!["file:/a.docx".into()],
            versions: vec![SourceVersion {
                version_id: "../escape".into(),
                raw_evidence: vec![SourceArtifactRecord {
                    path: "raw/sources/source-1/safe-version/original.docx".into(),
                    ..fixture_version("version-1", "hash-a").raw_evidence[0].clone()
                }],
                baseline_path: ".app/source-artifacts/source-1/safe-version/baseline.md".into(),
                ..fixture_version("version-1", "hash-a")
            }],
            current_version_id: "../escape".into(),
            wiki_path: "wiki/sources/local/a.md".into(),
            ..fixture_manifest()
        };
        let error = SourceRegistry
            .build_commit_plan(
                &index,
                Some(&existing),
                &fixture_input("file:/a.docx", "hash-a", "a.docx"),
            )
            .unwrap_err();
        assert_eq!(error.code, IMPORT_V2_SOURCE_INDEX_INVALID);
    }

    #[test]
    fn commit_plan_derives_safe_unicode_paths_through_project_context() {
        let input = fixture_input("file:d:/资料/研究报告.docx", "hash-cjk", "研究报告.docx");
        let plan = SourceRegistry
            .build_commit_plan(&SourceIndex::default_v2(), None, &input)
            .unwrap();
        assert_eq!(plan.wiki_path, "wiki/sources/local/研究报告.md");
        assert_eq!(
            plan.manifest_path,
            format!(".app/sources/{}.json", plan.source_id)
        );
        let (context, root) = super::super::test_support::test_context("source-registry-paths");
        assert!(context.resolve_project_path(&plan.raw_path).is_ok());
        assert_eq!(
            plan.evidence_root_path,
            format!("raw/sources/{}/{}", plan.source_id, plan.version_id)
        );
        assert!(context.resolve_project_path(&plan.baseline_path).is_ok());
        assert!(context.resolve_project_path(&plan.wiki_path).is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wiki_slug_avoids_windows_device_names_and_excessive_length() {
        let reserved = SourceRegistry
            .build_commit_plan(
                &SourceIndex::default_v2(),
                None,
                &fixture_input("file:/CON.docx", "hash-con", "CON.docx"),
            )
            .unwrap();
        assert_ne!(reserved.wiki_path, "wiki/sources/local/CON.md");

        let long_name = format!("{}.docx", "研".repeat(200));
        let long = SourceRegistry
            .build_commit_plan(
                &SourceIndex::default_v2(),
                None,
                &fixture_input("file:/long.docx", "hash-long", &long_name),
            )
            .unwrap();
        assert!(long.wiki_path.len() < 240);
    }

    #[test]
    fn web_host_and_wiki_stem_are_portable_across_supported_filesystems() {
        let host_cases = [
            ("https://EXAMPLE.COM./article", "example.com"),
            ("https://例子.测试/article", "xn--fsqu00a.xn--0zwm56d"),
            ("https://Example.com:8443/article", "example.com-port-8443"),
        ];
        for (url, expected) in host_cases {
            assert_eq!(normalized_web_host(url).as_deref(), Some(expected));
        }

        let nfc = portable_wiki_stem("Café");
        let nfd = portable_wiki_stem("Cafe\u{301}");
        assert_eq!(nfc, nfd, "macOS NFD and NFC names must share one slug");
        assert_ne!(
            portable_wiki_stem("Report"),
            portable_wiki_stem("report"),
            "Linux-visible title casing is retained in the chosen filename"
        );
    }

    #[test]
    fn existing_manifest_wiki_filename_must_be_portable() {
        let invalid_names = [
            "CON.md".to_string(),
            "bad?.md".to_string(),
            "bad\u{0007}.md".to_string(),
            "trailing..md".to_string(),
            "trailing .md".to_string(),
            format!("{}.md", "a".repeat(121)),
        ];

        for name in invalid_names {
            let existing = SourceManifest {
                source_id: "source-1".into(),
                origins: vec!["file:/a.docx".into()],
                versions: vec![fixture_version("version-1", "hash-a")],
                current_version_id: "version-1".into(),
                wiki_path: format!("wiki/sources/local/{name}"),
                ..fixture_manifest()
            };
            let error = SourceRegistry
                .build_commit_plan(
                    &SourceIndex::default_v2(),
                    Some(&existing),
                    &fixture_input("file:/a.docx", "hash-b", "a.docx"),
                )
                .unwrap_err();
            assert_eq!(
                error.code, IMPORT_V2_SOURCE_INDEX_INVALID,
                "portable filename should be rejected: {name:?}"
            );
        }
    }

    #[test]
    fn reserved_device_basename_with_extra_extension_is_never_a_wiki_target() {
        for (name, hash) in [
            ("CON.foo.docx", "hash-con-dot"),
            ("COM1.backup.docx", "hash-com-dot"),
        ] {
            let generated = SourceRegistry
                .build_commit_plan(
                    &SourceIndex::default_v2(),
                    None,
                    &fixture_input(&format!("file:/{name}"), hash, name),
                )
                .unwrap();
            assert!(
                generated.wiki_path.contains("/source-"),
                "generated target must prefix reserved device basename: {name:?}"
            );

            let wiki_name = name.trim_end_matches(".docx");
            let existing = SourceManifest {
                source_id: "source-1".into(),
                origins: vec!["file:/a.docx".into()],
                versions: vec![fixture_version("version-1", "hash-a")],
                current_version_id: "version-1".into(),
                wiki_path: format!("wiki/sources/local/{wiki_name}.md"),
                ..fixture_manifest()
            };
            let error = SourceRegistry
                .build_commit_plan(
                    &SourceIndex::default_v2(),
                    Some(&existing),
                    &fixture_input("file:/a.docx", "hash-b", "a.docx"),
                )
                .unwrap_err();
            assert_eq!(error.code, IMPORT_V2_SOURCE_INDEX_INVALID);
        }
    }

    #[test]
    fn wiki_asset_resolution_keeps_legacy_fallback_before_and_after_v3_migration() {
        let (context, root) = super::super::test_support::test_context("wiki-asset");
        let files = FileStore;
        let wiki_path = "wiki/sources/files/legacy.md";
        let manifest_path = ".app/sources/source-legacy.json";
        let pointer = SourcePointer {
            source_id: "source-legacy".into(),
            version_id: "version-legacy".into(),
        };
        let index = SourceIndex {
            schema_version: LEGACY_SOURCE_REGISTRY_SCHEMA_VERSION,
            by_content_hash: BTreeMap::from([("a".repeat(64), pointer.clone())]),
            by_locator: BTreeMap::from([("file:/fixtures/legacy.md".into(), pointer)]),
        };
        files
            .write_json_atomic(&context, SOURCE_INDEX_PATH, &index)
            .unwrap();
        files
            .write_markdown(
                &context,
                manifest_path,
                include_str!("../../../../tests/fixtures/import-v2/legacy-source-manifest-v2.json"),
            )
            .unwrap();
        files
            .write_markdown(&context, wiki_path, "![cover](assets/cover.jpg)")
            .unwrap();
        let legacy_asset = root.join("raw/sources/source-legacy/version-legacy/assets/cover.jpg");
        std::fs::create_dir_all(legacy_asset.parent().unwrap()).unwrap();
        std::fs::write(&legacy_asset, b"legacy-image").unwrap();
        let legacy_before = std::fs::read(&legacy_asset).unwrap();
        let legacy_modified_before = std::fs::metadata(&legacy_asset)
            .unwrap()
            .modified()
            .unwrap();
        let index_before = std::fs::read(root.join(SOURCE_INDEX_PATH)).unwrap();
        let manifest_before = std::fs::read(root.join(manifest_path)).unwrap();

        let resolved = SourceRegistry::resolve_wiki_asset_path(
            &context,
            &files,
            wiki_path,
            "assets/cover.jpg",
        )
        .unwrap();
        assert_eq!(resolved, legacy_asset);
        assert_eq!(std::fs::read(&legacy_asset).unwrap(), legacy_before);
        assert_eq!(
            std::fs::metadata(&legacy_asset)
                .unwrap()
                .modified()
                .unwrap(),
            legacy_modified_before
        );
        assert_eq!(
            std::fs::read(root.join(SOURCE_INDEX_PATH)).unwrap(),
            index_before
        );
        assert_eq!(
            std::fs::read(root.join(manifest_path)).unwrap(),
            manifest_before
        );

        let canonical_asset = root.join("raw/assets/source-legacy/version-legacy/cover.jpg");
        std::fs::create_dir_all(canonical_asset.parent().unwrap()).unwrap();
        std::fs::write(&canonical_asset, b"canonical-image").unwrap();
        let canonical_resolved = SourceRegistry::resolve_wiki_asset_path(
            &context,
            &files,
            wiki_path,
            "assets/cover.jpg?download=1#preview",
        )
        .unwrap();
        assert_eq!(canonical_resolved, canonical_asset);

        let traversal = SourceRegistry::resolve_wiki_asset_path(
            &context,
            &files,
            wiki_path,
            "assets/../secret.jpg",
        )
        .unwrap_err();
        assert_eq!(traversal.code, "WIKI_ASSET_NOT_FOUND");

        std::fs::remove_file(&canonical_asset).unwrap();
        assert!(SourceRegistry::migrate_project_v3(&context, &files).unwrap());
        let migrated_index: serde_json::Value =
            files.read_json(&context, SOURCE_INDEX_PATH).unwrap();
        let migrated_manifest: serde_json::Value =
            files.read_json(&context, manifest_path).unwrap();
        assert_eq!(
            migrated_index["schemaVersion"],
            SOURCE_REGISTRY_SCHEMA_VERSION
        );
        assert_eq!(
            migrated_manifest["schemaVersion"],
            SOURCE_REGISTRY_SCHEMA_VERSION
        );
        let migrated_resolved = SourceRegistry::resolve_wiki_asset_path(
            &context,
            &files,
            wiki_path,
            "assets/cover.jpg",
        )
        .unwrap();
        assert_eq!(migrated_resolved, legacy_asset);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wiki_asset_resolution_uses_exact_frontmatter_manifest_and_rejects_forgery() {
        let (context, root) =
            super::super::test_support::test_context("wiki-asset-direct-manifest");
        let files = FileStore;
        let manifest = fixture_manifest();
        let version = manifest.versions.first().unwrap();
        let frontmatter = SourceFrontmatter {
            page_type: SourcePageType::Source,
            source_id: manifest.source_id.clone(),
            version_id: version.version_id.clone(),
            source_kind: version.candidate.source_kind.clone(),
            title: version.candidate.title.clone(),
            imported_at: version.created_at.clone(),
            content_hash: version.content_hash.clone(),
            platform: version.candidate.platform.clone(),
            canonical_url: version.candidate.canonical_url.clone(),
            platform_content_id: version.candidate.platform_content_id.clone(),
            author: version.candidate.author.clone(),
            published_at: version.candidate.published_at.clone(),
            language: version.candidate.language.clone(),
            quality: version.quality.clone(),
            restricted: manifest.restricted_content,
        };
        let markdown =
            render_source_markdown(&frontmatter, "# Fixture\n\n![cover](assets/cover.jpg)\n")
                .unwrap();
        let target_pointer = SourcePointer {
            source_id: manifest.source_id.clone(),
            version_id: version.version_id.clone(),
        };
        let unrelated_pointer = SourcePointer {
            source_id: "source-0".into(),
            version_id: "version-0".into(),
        };
        let index = SourceIndex {
            schema_version: SOURCE_REGISTRY_SCHEMA_VERSION,
            by_content_hash: BTreeMap::from([
                ("unrelated-hash".into(), unrelated_pointer.clone()),
                (version.content_hash.clone(), target_pointer.clone()),
            ]),
            by_locator: BTreeMap::from([
                ("file:/unrelated.docx".into(), unrelated_pointer),
                ("file:/fixture.docx".into(), target_pointer),
            ]),
        };
        files
            .write_json_atomic(&context, SOURCE_INDEX_PATH, &index)
            .unwrap();
        files
            .write_json_atomic(&context, ".app/sources/source-1.json", &manifest)
            .unwrap();
        files
            .write_markdown(&context, ".app/sources/source-0.json", "{")
            .unwrap();
        files
            .write_markdown(&context, &manifest.wiki_path, &markdown)
            .unwrap();
        let canonical_asset = root.join("raw/assets/source-1/version-1/cover.jpg");
        std::fs::create_dir_all(canonical_asset.parent().unwrap()).unwrap();
        std::fs::write(&canonical_asset, b"canonical-image").unwrap();

        let resolved = SourceRegistry::resolve_wiki_asset_path(
            &context,
            &files,
            &manifest.wiki_path,
            "assets/cover.jpg",
        )
        .unwrap();
        assert_eq!(resolved, canonical_asset);

        let forged = markdown.replacen("sourceId: \"source-1\"", "sourceId: \"source-0\"", 1);
        files
            .write_markdown(&context, &manifest.wiki_path, &forged)
            .unwrap();
        assert_eq!(
            SourceRegistry::resolve_wiki_asset_path(
                &context,
                &files,
                &manifest.wiki_path,
                "assets/cover.jpg",
            )
            .unwrap_err()
            .code,
            "WIKI_ASSET_NOT_FOUND"
        );

        let invalid_pages: [(&str, &[u8]); 3] = [
            ("missing", b"# Fixture\n\n![cover](assets/cover.jpg)\n"),
            (
                "malformed",
                b"---\ntype: source\nsourceId: [\n---\n\n![cover](assets/cover.jpg)\n",
            ),
            ("non-utf8", b"\xff\xfe![cover](assets/cover.jpg)\n"),
        ];
        let wiki_absolute = context.resolve_project_path(&manifest.wiki_path).unwrap();
        for (case, bytes) in invalid_pages {
            std::fs::write(&wiki_absolute, bytes).unwrap();
            let error = SourceRegistry::resolve_wiki_asset_path(
                &context,
                &files,
                &manifest.wiki_path,
                "assets/cover.jpg",
            )
            .unwrap_err();
            assert_eq!(
                error.code, "WIKI_ASSET_NOT_FOUND",
                "current schema must not downgrade {case} frontmatter into legacy scanning"
            );
        }

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn compile_consumption_updates_manifest_and_writes_one_task_record() {
        let (context, root) = super::super::test_support::test_context("compile-consumption");
        let files = FileStore;
        let (_manifest, reference) = persist_compile_fixture(&context, &files);
        let consumed = SourceRegistry::record_compile_consumption(
            &context,
            &files,
            &CompileConsumptionRecord {
                schema_version: 1,
                compile_task_id: "task-1".into(),
                route: CompileRoute::Byok,
                consumed_at: "2026-07-26T00:00:00Z".into(),
                source_versions: vec![reference.clone()],
                affected_paths: vec!["wiki/index.md".into()],
                checkpoint: Some("checkpoint-1".into()),
            },
        )
        .unwrap();
        assert_eq!(consumed, vec![reference]);
        let updated =
            SourceRegistry::read_manifest(&context, &files, ".app/sources/source-1.json").unwrap();
        assert_eq!(updated.compiled_consumptions.len(), 1);
        let record: CompileConsumptionRecord = files
            .read_json(&context, ".app/compile/task-1.json")
            .unwrap();
        assert_eq!(record.schema_version, 1);
        assert_eq!(record.source_versions, consumed);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn compile_source_resolution_rejects_misbound_or_tampered_registry_state() {
        for case in ["index_pointer", "manifest_identity", "source_bytes"] {
            let (context, root) =
                super::super::test_support::test_context(&format!("compile-binding-{case}"));
            let files = FileStore;
            let (manifest, reference) = persist_compile_fixture(&context, &files);
            let mut index = SourceRegistry::read_index(&context, &files).unwrap();
            match case {
                "index_pointer" => {
                    index.by_content_hash.insert(
                        reference.content_hash.clone(),
                        SourcePointer {
                            source_id: "source-other".into(),
                            version_id: reference.version_id.clone(),
                        },
                    );
                }
                "manifest_identity" => {
                    let mut misplaced = manifest.clone();
                    misplaced.source_id = "source-other".into();
                    misplaced.wiki_path = "wiki/sources/local/other.md".into();
                    for version in &mut misplaced.versions {
                        version.baseline_path = format!(
                            ".app/source-artifacts/source-other/{}/baseline.md",
                            version.version_id
                        );
                        for evidence in &mut version.raw_evidence {
                            evidence.path = format!(
                                "raw/sources/source-other/{}/original.docx",
                                version.version_id
                            );
                        }
                    }
                    files
                        .write_json_atomic(&context, ".app/sources/source-1.json", &misplaced)
                        .unwrap();
                }
                "source_bytes" => {
                    std::fs::write(
                        context.resolve_project_path(&manifest.wiki_path).unwrap(),
                        b"externally replaced",
                    )
                    .unwrap();
                }
                _ => unreachable!(),
            }

            let error = SourceRegistry::resolve_compile_source_version(
                &context, &files, &index, &reference,
            )
            .unwrap_err();
            assert_eq!(
                error.code, "COMPILE_SOURCE_VERSION_INVALID",
                "case {case} must fail closed"
            );
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn compile_consumption_preflights_every_reference_before_any_write() {
        let (context, root) =
            super::super::test_support::test_context("compile-consumption-preflight");
        let files = FileStore;
        let (_manifest, valid) = persist_compile_fixture(&context, &files);
        let invalid = SourceVersionRef {
            source_id: "source-missing".into(),
            version_id: "version-missing".into(),
            content_hash: "f".repeat(64),
        };
        let error = SourceRegistry::record_compile_consumption(
            &context,
            &files,
            &CompileConsumptionRecord {
                schema_version: 1,
                compile_task_id: "task-preflight".into(),
                route: CompileRoute::Byok,
                consumed_at: "2026-07-26T00:00:00Z".into(),
                source_versions: vec![valid, invalid],
                affected_paths: vec!["wiki/index.md".into()],
                checkpoint: None,
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "COMPILE_SOURCE_VERSION_INVALID");
        let unchanged =
            SourceRegistry::read_manifest(&context, &files, ".app/sources/source-1.json").unwrap();
        assert!(unchanged.compiled_consumptions.is_empty());
        assert!(!context.app_dir.join("compile/task-preflight.json").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_compile_consumption_never_rewrites_the_legacy_index() {
        let (context, root) = super::super::test_support::test_context("legacy-consumption");
        let files = FileStore;
        let legacy = br#"{"sources":{"raw/source.txt":["raw/extracted/source.md"]}}"#;
        let legacy_path = context
            .resolve_project_path(".app/source-index.json")
            .unwrap();
        std::fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
        std::fs::write(&legacy_path, legacy).unwrap();
        SourceRegistry::record_compile_consumption(
            &context,
            &files,
            &CompileConsumptionRecord {
                schema_version: 1,
                compile_task_id: "task-legacy".into(),
                route: CompileRoute::Byok,
                consumed_at: "2026-07-26T00:00:00Z".into(),
                source_versions: vec![SourceVersionRef {
                    source_id: "legacy-0123456789abcdef".into(),
                    version_id: "legacy-fedcba9876543210".into(),
                    content_hash: "a".repeat(64),
                }],
                affected_paths: Vec::new(),
                checkpoint: None,
            },
        )
        .unwrap();
        assert_eq!(std::fs::read(&legacy_path).unwrap(), legacy);
        std::fs::remove_dir_all(root).unwrap();
    }
}
