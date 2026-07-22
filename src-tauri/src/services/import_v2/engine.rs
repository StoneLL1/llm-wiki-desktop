use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use crate::errors::{BackendError, IMPORT_V2_ENGINE_OUTPUT_INVALID, IMPORT_V2_ENGINE_UNAVAILABLE};
use crate::models::import_v2::{ImportInput, MediaSaveMode};
use crate::models::paths::ProjectContext;
use crate::tasks::task_model::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EngineOperation {
    Inspect,
    Extract,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EngineRequest {
    pub protocol_version: String,
    pub request_id: String,
    #[serde(default)]
    pub project_id: String,
    pub session_id: String,
    pub item_id: String,
    pub task_id: String,
    pub operation: EngineOperation,
    pub input: ImportInput,
    #[serde(default)]
    pub project_root: String,
    pub staging_root: String,
    /// Staging-relative artifact produced by a preceding route (for example
    /// the validated OOXML emitted by the legacy Office converter).
    #[serde(default)]
    pub chained_input: Option<String>,
    #[serde(default)]
    pub local_asr_authorized: bool,
    #[serde(default)]
    pub local_ocr_authorized: bool,
    #[serde(default)]
    pub media_save_mode: MediaSaveMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EngineResult {
    pub source_snapshot_path: String,
    pub markdown_path: String,
    pub asset_paths: Vec<String>,
    #[serde(default)]
    pub metadata_path: Option<String>,
    pub title: String,
    pub text_coverage: Option<f64>,
    pub table_cell_accuracy: Option<f64>,
    #[serde(default)]
    pub sheet_count_exact: Option<f64>,
    #[serde(default)]
    pub slide_count_exact: Option<f64>,
    #[serde(default)]
    pub non_empty_cell_coverage: Option<f64>,
    #[serde(default)]
    pub formula_value_pairs: Option<f64>,
    #[serde(default)]
    pub meaningful_image_coverage: Option<f64>,
    #[serde(default)]
    pub continuation: Option<EngineContinuation>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EngineContinuation {
    LocalAsr {
        temporary_input_path: String,
        media_kind: String,
    },
    LocalOcr {
        temporary_input_paths: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EngineDescriptor {
    pub engine_id: String,
    pub engine_version: String,
    pub route: String,
}

pub trait ImportEngine: Send + Sync {
    fn descriptor(&self) -> EngineDescriptor;
    fn supports(&self, input: &ImportInput) -> bool;
    fn execute(
        &self,
        request: &EngineRequest,
        cancellation: &CancellationToken,
    ) -> Result<EngineResult, BackendError>;
}

#[derive(Default)]
pub struct EngineRegistry {
    engines: RwLock<Vec<Arc<dyn ImportEngine>>>,
}

impl EngineRegistry {
    pub fn registered_routes(&self) -> Result<Vec<String>, BackendError> {
        Ok(self
            .engines
            .read()
            .map_err(|_| registry_error())?
            .iter()
            .map(|engine| engine.descriptor().route)
            .collect())
    }
    pub fn register(&self, engine: Arc<dyn ImportEngine>) -> Result<(), BackendError> {
        self.register_inner(engine, false)
    }

    pub(crate) fn ensure_registered(
        &self,
        engine: Arc<dyn ImportEngine>,
    ) -> Result<(), BackendError> {
        self.register_inner(engine, true)
    }

    fn register_inner(
        &self,
        engine: Arc<dyn ImportEngine>,
        allow_exact_match: bool,
    ) -> Result<(), BackendError> {
        let descriptor = engine.descriptor();
        let mut engines = self.engines.write().map_err(|_| registry_error())?;
        if allow_exact_match
            && engines
                .iter()
                .any(|existing| existing.descriptor() == descriptor)
        {
            return Ok(());
        }
        if engines
            .iter()
            .any(|existing| existing.descriptor().engine_id == descriptor.engine_id)
        {
            return Err(BackendError::new(
                IMPORT_V2_ENGINE_UNAVAILABLE,
                "An import engine with this identifier is already registered.",
                true,
                false,
            ));
        }
        engines.push(engine);
        Ok(())
    }

    pub fn resolve(&self, input: &ImportInput) -> Result<Arc<dyn ImportEngine>, BackendError> {
        self.engines
            .read()
            .map_err(|_| registry_error())?
            .iter()
            .find(|engine| engine.supports(input))
            .cloned()
            .ok_or_else(|| {
                BackendError::new(
                    IMPORT_V2_ENGINE_UNAVAILABLE,
                    "No installed import engine supports this input.",
                    true,
                    true,
                )
            })
    }

    /// Resolve an explicitly planned route; registration order is never routing policy.
    pub fn resolve_route(
        &self,
        route: &str,
        input: &ImportInput,
    ) -> Result<Arc<dyn ImportEngine>, BackendError> {
        self.engines
            .read()
            .map_err(|_| registry_error())?
            .iter()
            .filter(|engine| {
                let descriptor = engine.descriptor();
                descriptor.route == route && engine.supports(input)
            })
            // Built-ins are safe fallbacks. Prefer an installed capability pack
            // when it provides the same planned route (notably browser/web).
            .min_by_key(|engine| engine.descriptor().engine_id.starts_with("builtin."))
            .cloned()
            .ok_or_else(|| {
                BackendError::new(
                    IMPORT_V2_ENGINE_UNAVAILABLE,
                    "The planned import route is not installed.",
                    true,
                    true,
                )
            })
    }
}

pub fn validate_engine_result(
    staging_root: &str,
    result: &EngineResult,
) -> Result<(), BackendError> {
    validate_staging_relative_path(staging_root, &result.source_snapshot_path)?;
    validate_staging_relative_path(staging_root, &result.markdown_path)?;
    for asset_path in &result.asset_paths {
        validate_staging_relative_path(staging_root, asset_path)?;
    }
    if let Some(EngineContinuation::LocalAsr {
        temporary_input_path,
        media_kind,
        ..
    }) = &result.continuation
    {
        validate_staging_relative_path(staging_root, temporary_input_path)?;
        if media_kind != "audio" && media_kind != "video" {
            return Err(output_error());
        }
    }
    if let Some(metadata_path) = &result.metadata_path {
        validate_staging_relative_path(staging_root, metadata_path)?;
    }
    Ok(())
}

fn validate_staging_relative_path(
    staging_root: &str,
    relative_path: &str,
) -> Result<(), BackendError> {
    let normalized = relative_path.replace('\\', "/");
    let invalid = normalized.trim().is_empty()
        || normalized.starts_with('/')
        || normalized.contains(':')
        || normalized
            .split('/')
            .any(|component| component == "." || component == "..");
    if invalid {
        return Err(output_error());
    }

    let project_context = ProjectContext::new("engine-output-validation", PathBuf::from("."));
    let staged_path = format!(
        "{}/{}",
        staging_root.trim_end_matches(['/', '\\']),
        normalized
    );
    project_context
        .resolve_project_path(&staged_path)
        .map(|_| ())
        .map_err(|_| output_error())
}

fn registry_error() -> BackendError {
    BackendError::new(
        IMPORT_V2_ENGINE_UNAVAILABLE,
        "The import engine registry is unavailable.",
        true,
        false,
    )
}

fn output_error() -> BackendError {
    BackendError::new(
        IMPORT_V2_ENGINE_OUTPUT_INVALID,
        "An import engine returned a path outside its item staging directory.",
        false,
        false,
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::errors::{BackendError, IMPORT_V2_ENGINE_OUTPUT_INVALID};
    use crate::models::import_v2::{ImportInput, ImportInputKind};
    use crate::tasks::task_model::CancellationToken;

    struct FixtureEngine {
        descriptor: EngineDescriptor,
        supported: bool,
    }

    impl FixtureEngine {
        fn new(engine_id: &str, supported: bool) -> Self {
            Self::with_route(engine_id, "fixture", supported)
        }

        fn with_route(engine_id: &str, route: &str, supported: bool) -> Self {
            Self {
                descriptor: EngineDescriptor {
                    engine_id: engine_id.to_string(),
                    engine_version: "1.0.0".into(),
                    route: route.into(),
                },
                supported,
            }
        }
    }

    impl ImportEngine for FixtureEngine {
        fn descriptor(&self) -> EngineDescriptor {
            self.descriptor.clone()
        }

        fn supports(&self, _input: &ImportInput) -> bool {
            self.supported
        }

        fn execute(
            &self,
            _request: &EngineRequest,
            _cancellation: &CancellationToken,
        ) -> Result<EngineResult, BackendError> {
            Ok(valid_result())
        }
    }

    fn valid_result() -> EngineResult {
        EngineResult {
            source_snapshot_path: "source.bin".into(),
            markdown_path: "candidate.md".into(),
            asset_paths: vec!["assets/image.png".into()],
            metadata_path: None,
            title: "Fixture".into(),
            text_coverage: Some(1.0),
            table_cell_accuracy: None,
            sheet_count_exact: None,
            slide_count_exact: None,
            non_empty_cell_coverage: None,
            formula_value_pairs: None,
            meaningful_image_coverage: None,
            continuation: None,
            warnings: Vec::new(),
        }
    }

    #[test]
    fn registry_selects_only_a_supporting_engine() {
        let registry = EngineRegistry::default();
        registry
            .register(Arc::new(FixtureEngine::new("unsupported", false)))
            .unwrap();
        registry
            .register(Arc::new(FixtureEngine::new("fixture", true)))
            .unwrap();
        let input = ImportInput {
            source_identity: None,
            kind: ImportInputKind::File,
            display_name: "a.pdf".into(),
            locator: "D:/a.pdf".into(),
            normalized_locator: None,
            media_save_mode: Default::default(),
        };

        assert_eq!(
            registry.resolve(&input).unwrap().descriptor().engine_id,
            "fixture"
        );
    }

    #[test]
    fn registry_rejects_duplicate_engine_identifiers() {
        let registry = EngineRegistry::default();
        registry
            .register(Arc::new(FixtureEngine::new("fixture", true)))
            .unwrap();

        let error = registry
            .register(Arc::new(FixtureEngine::new("fixture", false)))
            .unwrap_err();

        assert_eq!(error.code, crate::errors::IMPORT_V2_ENGINE_UNAVAILABLE);
    }

    #[test]
    fn planned_route_prefers_installed_pack_over_builtin_fallback() {
        let registry = EngineRegistry::default();
        registry
            .register(Arc::new(FixtureEngine::with_route(
                "builtin.web-http-browser",
                "web.generic.browser",
                true,
            )))
            .unwrap();
        registry
            .register(Arc::new(FixtureEngine::with_route(
                "pack.browser",
                "web.generic.browser",
                true,
            )))
            .unwrap();
        let input = ImportInput {
            source_identity: None,
            kind: ImportInputKind::Url,
            display_name: "page".into(),
            locator: "https://example.com".into(),
            normalized_locator: Some("https://example.com".into()),
            media_save_mode: Default::default(),
        };

        assert_eq!(
            registry
                .resolve_route("web.generic.browser", &input)
                .unwrap()
                .descriptor()
                .engine_id,
            "pack.browser"
        );
    }

    #[test]
    fn engine_result_cannot_escape_item_staging() {
        let mut result = valid_result();
        result.markdown_path = "../outside.md".into();

        let error =
            validate_engine_result(".app/import-sessions/s/items/i/staging", &result).unwrap_err();

        assert_eq!(error.code, IMPORT_V2_ENGINE_OUTPUT_INVALID);
    }

    #[test]
    fn every_engine_result_path_must_be_staging_relative() {
        let invalid_paths = [
            "",
            ".",
            "./candidate.md",
            "assets/../outside.md",
            "/absolute.md",
            r"C:\absolute.md",
            r"\\server\share\file.md",
        ];

        for invalid_path in invalid_paths {
            let mut result = valid_result();
            result.asset_paths = vec![invalid_path.into()];
            let error = validate_engine_result(".app/import-sessions/s/items/i/staging", &result)
                .unwrap_err();
            assert_eq!(
                error.code, IMPORT_V2_ENGINE_OUTPUT_INVALID,
                "path should be rejected: {invalid_path:?}"
            );
        }
    }

    #[test]
    fn chained_input_is_an_explicit_protocol_field() {
        let value = serde_json::to_value(EngineRequest {
            protocol_version: "2".into(),
            request_id: "r".into(),
            project_id: "p".into(),
            session_id: "s".into(),
            item_id: "i".into(),
            task_id: "t".into(),
            operation: EngineOperation::Extract,
            input: ImportInput {
                source_identity: None,
                kind: ImportInputKind::File,
                display_name: "legacy.doc".into(),
                locator: "legacy.doc".into(),
                normalized_locator: None,
                media_save_mode: Default::default(),
            },
            project_root: "root".into(),
            staging_root: "staging".into(),
            chained_input: Some("converted/legacy.docx".into()),
            local_asr_authorized: false,
            local_ocr_authorized: false,
            media_save_mode: Default::default(),
        })
        .unwrap();
        assert_eq!(value["chainedInput"], "converted/legacy.docx");
    }
}
