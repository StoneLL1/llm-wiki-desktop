use std::collections::BTreeMap;

use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use crate::errors::{BackendError, IMPORT_V2_SOURCE_INDEX_INVALID};
use crate::models::import_v2::{ImportInputKind, QualityReport, IMPORT_V2_SCHEMA_VERSION};
use crate::models::paths::ProjectContext;
use crate::services::FileStore;

const SOURCE_INDEX_PATH: &str = ".app/source-index-v2.json";

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
            schema_version: IMPORT_V2_SCHEMA_VERSION,
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
#[serde(rename_all = "camelCase")]
pub struct SourceManifest {
    pub schema_version: u32,
    pub source_id: String,
    pub origins: Vec<String>,
    pub versions: Vec<SourceVersion>,
    pub current_version_id: String,
    pub wiki_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceVersion {
    pub version_id: String,
    pub content_hash: String,
    pub raw_path: String,
    pub baseline_path: String,
    pub created_at: String,
    pub route: String,
    pub engine_id: String,
    pub engine_version: String,
    pub quality: QualityReport,
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
    pub source_id: String,
    pub version_id: String,
    pub raw_path: String,
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
    pub route: String,
    pub engine_id: String,
    pub engine_version: String,
    pub quality: QualityReport,
}

#[derive(Default)]
pub struct SourceRegistry;

impl SourceRegistry {
    pub fn read_index(
        context: &ProjectContext,
        files: &FileStore,
    ) -> Result<SourceIndex, BackendError> {
        if !files.exists(context, SOURCE_INDEX_PATH) {
            return Ok(SourceIndex::default_v2());
        }

        let index: SourceIndex = files
            .read_json(context, SOURCE_INDEX_PATH)
            .map_err(|_| invalid_index())?;
        validate_index(&index)?;
        Ok(index)
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
        let raw_path = format!("raw/sources/{source_id}/{version_id}/original.{extension}");
        let baseline_path = format!(".app/source-artifacts/{source_id}/{version_id}/baseline.md");
        let wiki_path = existing
            .map(|manifest| manifest.wiki_path.clone())
            .unwrap_or_else(|| derive_wiki_path(input));

        let mut next_manifest = existing.cloned().unwrap_or_else(|| SourceManifest {
            schema_version: IMPORT_V2_SCHEMA_VERSION,
            source_id: source_id.clone(),
            origins: Vec::new(),
            versions: Vec::new(),
            current_version_id: version_id.clone(),
            wiki_path: wiki_path.clone(),
        });
        if !next_manifest.origins.contains(&locator) {
            next_manifest.origins.push(locator.clone());
            next_manifest.origins.sort();
        }
        if reused_version_id.is_none() {
            next_manifest.versions.push(SourceVersion {
                version_id: version_id.clone(),
                content_hash: input.content_hash.clone(),
                raw_path: raw_path.clone(),
                baseline_path: baseline_path.clone(),
                created_at: chrono::Utc::now().to_rfc3339(),
                route: input.route.clone(),
                engine_id: input.engine_id.clone(),
                engine_version: input.engine_version.clone(),
                quality: input.quality.clone(),
            });
            next_manifest.current_version_id = version_id.clone();
        }

        let (raw_path, baseline_path) = if reused_version_id.is_some() {
            let version = next_manifest
                .versions
                .iter()
                .find(|version| version.version_id == version_id)
                .ok_or_else(invalid_index)?;
            (version.raw_path.clone(), version.baseline_path.clone())
        } else {
            (raw_path, baseline_path)
        };

        let pointer = SourcePointer {
            source_id: source_id.clone(),
            version_id: version_id.clone(),
        };
        let mut next_index = index.clone();
        next_index.schema_version = IMPORT_V2_SCHEMA_VERSION;
        next_index
            .by_content_hash
            .insert(input.content_hash.clone(), pointer.clone());
        next_index.by_locator.insert(locator, pointer);

        let asset_root_path = format!("raw/sources/{source_id}/{version_id}/assets");
        let plan = SourceCommitPlan {
            source_id: source_id.clone(),
            version_id,
            raw_path,
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

fn validate_index(index: &SourceIndex) -> Result<(), BackendError> {
    if index.schema_version != IMPORT_V2_SCHEMA_VERSION
        || index
            .by_content_hash
            .iter()
            .any(|(hash, pointer)| hash.trim().is_empty() || invalid_pointer(pointer))
        || index.by_locator.iter().any(|(locator, pointer)| {
            locator.trim().is_empty()
                || normalize_locator(locator) != *locator
                || invalid_pointer(pointer)
                || !index.by_content_hash.values().any(|known| known == pointer)
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
            || !valid_raw_path(&version.raw_path, &manifest.source_id, &version.version_id)
            || version.baseline_path
                != format!(
                    ".app/source-artifacts/{}/{}/baseline.md",
                    manifest.source_id, version.version_id
                )
            || paths.resolve_project_path(&version.raw_path).is_err()
            || paths.resolve_project_path(&version.baseline_path).is_err()
    });
    if manifest.schema_version != IMPORT_V2_SCHEMA_VERSION
        || !is_safe_id(&manifest.source_id)
        || current_count != 1
        || invalid_version
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
        || plan.asset_root_path
            != format!("raw/sources/{}/{}/assets", plan.source_id, plan.version_id)
        || plan.baseline_path
            != format!(
                ".app/source-artifacts/{}/{}/baseline.md",
                plan.source_id, plan.version_id
            )
        || !valid_wiki_path(&plan.wiki_path)
        || plan.manifest_path != manifest_path
        || [
            &plan.raw_path,
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
        && parts[1] == "sources"
        && parts[2] == source_id
        && parts[3] == version_id
        && parts[4].starts_with("original.")
        && safe_extension(parts[4].trim_start_matches("original."))
            == parts[4].trim_start_matches("original.")
}

fn valid_wiki_path(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    let filename = parts.get(3).copied().unwrap_or_default();
    let stem = filename.strip_suffix(".md").unwrap_or_default();
    parts.len() == 4
        && parts[0] == "wiki"
        && parts[1] == "sources"
        && matches!(parts[2], "files" | "web" | "video")
        && !stem.is_empty()
        && portable_wiki_stem(stem) == stem
}

fn safe_extension(extension: &str) -> String {
    let extension = extension.trim().trim_start_matches('.').to_lowercase();
    if !extension.is_empty()
        && extension.len() <= 16
        && extension.chars().all(|ch| ch.is_ascii_alphanumeric())
    {
        extension
    } else {
        "bin".into()
    }
}

fn derive_wiki_path(input: &SourceCommitInput) -> String {
    let category = match input.input_kind {
        ImportInputKind::File | ImportInputKind::Folder => "files",
        ImportInputKind::Url => "web",
    };
    let stem = input
        .display_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("source")
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(&input.display_name);
    let slug = portable_wiki_stem(stem);
    format!("wiki/sources/{category}/{slug}.md")
}

fn portable_wiki_stem(stem: &str) -> String {
    let slug: String = stem
        .trim()
        .chars()
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
    use crate::models::import_v2::{ImportInputKind, QualityLevel, QualityReport};
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

    fn fixture_version(version_id: &str, content_hash: &str) -> SourceVersion {
        SourceVersion {
            version_id: version_id.into(),
            content_hash: content_hash.into(),
            raw_path: format!("raw/sources/source-1/{version_id}/original.docx"),
            baseline_path: format!(".app/source-artifacts/source-1/{version_id}/baseline.md"),
            created_at: "2026-07-11T00:00:00Z".into(),
            route: "fixture".into(),
            engine_id: "fixture".into(),
            engine_version: "1.0.0".into(),
            quality: pass_quality(),
        }
    }

    fn fixture_input(locator: &str, hash: &str, name: &str) -> SourceCommitInput {
        SourceCommitInput {
            normalized_locator: locator.into(),
            content_hash: hash.into(),
            display_name: name.into(),
            input_kind: ImportInputKind::File,
            source_extension: "docx".into(),
            route: "fixture".into(),
            engine_id: "fixture".into(),
            engine_version: "1.0.0".into(),
            quality: pass_quality(),
        }
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
            schema_version: 2,
            source_id: "source-1".into(),
            origins: vec!["file:/a.docx".into()],
            versions: vec![fixture_version("version-1", "hash-a")],
            current_version_id: "version-1".into(),
            wiki_path: "wiki/sources/files/a.md".into(),
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
            schema_version: 2,
            source_id: "source-1".into(),
            origins: vec!["file:d:/a.docx".into()],
            versions: vec![fixture_version("version-1", "hash-a")],
            current_version_id: "version-1".into(),
            wiki_path: "wiki/sources/files/a.md".into(),
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
        let pointer = SourcePointer {
            source_id: "source-1".into(),
            version_id: "version-1".into(),
        };
        let index = SourceIndex {
            schema_version: 2,
            by_content_hash: BTreeMap::from([("hash-a".into(), pointer.clone())]),
            by_locator: BTreeMap::from([("file:d:/a.docx".into(), pointer)]),
        };
        let existing = SourceManifest {
            schema_version: 2,
            source_id: "source-1".into(),
            origins: vec!["file:d:/a.docx".into()],
            versions: vec![fixture_version("version-1", "hash-a")],
            current_version_id: "version-1".into(),
            wiki_path: "wiki/sources/files/a.md".into(),
        };
        let plan = SourceRegistry
            .build_commit_plan(
                &index,
                Some(&existing),
                &fixture_input("file:d:/副本.docx", "hash-a", "副本.docx"),
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
    fn corrupt_existing_manifest_is_rejected_before_paths_are_returned() {
        let existing = SourceManifest {
            schema_version: 1,
            source_id: "source-1".into(),
            origins: vec!["file:/a.docx".into()],
            versions: vec![fixture_version("version-1", "hash-a")],
            current_version_id: "version-1".into(),
            wiki_path: "../escape.md".into(),
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
            schema_version: 2,
            source_id: "nested/source".into(),
            origins: vec!["file:/a.docx".into()],
            versions: vec![SourceVersion {
                version_id: "version-1".into(),
                content_hash: "hash-a".into(),
                raw_path: "raw/sources/nested/source/version-1/original.docx".into(),
                baseline_path: ".app/source-artifacts/nested/source/version-1/baseline.md".into(),
                created_at: "2026-07-11T00:00:00Z".into(),
                route: "fixture".into(),
                engine_id: "fixture".into(),
                engine_version: "1.0.0".into(),
                quality: pass_quality(),
            }],
            current_version_id: "version-1".into(),
            wiki_path: "wiki/sources/files/a.md".into(),
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
            schema_version: 2,
            source_id: "source-1".into(),
            origins: vec!["file:/a.docx".into()],
            versions: vec![SourceVersion {
                version_id: "../escape".into(),
                content_hash: "hash-a".into(),
                raw_path: "raw/sources/source-1/safe-version/original.docx".into(),
                baseline_path: ".app/source-artifacts/source-1/safe-version/baseline.md".into(),
                created_at: "2026-07-11T00:00:00Z".into(),
                route: "fixture".into(),
                engine_id: "fixture".into(),
                engine_version: "1.0.0".into(),
                quality: pass_quality(),
            }],
            current_version_id: "../escape".into(),
            wiki_path: "wiki/sources/files/a.md".into(),
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
        assert_eq!(plan.wiki_path, "wiki/sources/files/研究报告.md");
        assert_eq!(
            plan.manifest_path,
            format!(".app/sources/{}.json", plan.source_id)
        );
        let (context, root) = super::super::test_support::test_context("source-registry-paths");
        assert!(context.resolve_project_path(&plan.raw_path).is_ok());
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
        assert_ne!(reserved.wiki_path, "wiki/sources/files/CON.md");

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
                schema_version: 2,
                source_id: "source-1".into(),
                origins: vec!["file:/a.docx".into()],
                versions: vec![fixture_version("version-1", "hash-a")],
                current_version_id: "version-1".into(),
                wiki_path: format!("wiki/sources/files/{name}"),
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
                schema_version: 2,
                source_id: "source-1".into(),
                origins: vec!["file:/a.docx".into()],
                versions: vec![fixture_version("version-1", "hash-a")],
                current_version_id: "version-1".into(),
                wiki_path: format!("wiki/sources/files/{wiki_name}.md"),
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
}
