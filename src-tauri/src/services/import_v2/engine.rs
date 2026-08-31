use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use crate::errors::{
    BackendError, IMPORT_V2_ENGINE_OUTPUT_INVALID, IMPORT_V2_ENGINE_PANICKED,
    IMPORT_V2_ENGINE_UNAVAILABLE,
};
use crate::models::import_v2::{ImportAsrProfile, ImportInput, MediaSaveMode};
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
    /// Allows a signed media capability to inspect only embedded subtitle
    /// tracks before the user grants ASR. It must never run speech recognition.
    #[serde(default)]
    pub asr_probe_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asr_profile: Option<ImportAsrProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recognition_language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_subtitle: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineProgress {
    pub current: u64,
    pub total: Option<u64>,
    pub label: String,
}

pub type EngineProgressReporter<'a> = dyn Fn(EngineProgress) -> Result<(), BackendError> + 'a;

pub trait ImportEngine: Send + Sync {
    fn descriptor(&self) -> EngineDescriptor;
    fn supports(&self, input: &ImportInput) -> bool;
    fn execute(
        &self,
        request: &EngineRequest,
        cancellation: &CancellationToken,
    ) -> Result<EngineResult, BackendError>;

    fn execute_with_progress(
        &self,
        request: &EngineRequest,
        cancellation: &CancellationToken,
        _report_progress: &EngineProgressReporter<'_>,
    ) -> Result<EngineResult, BackendError> {
        self.execute(request, cancellation)
    }
}

pub(crate) fn execute_engine(
    engine: &dyn ImportEngine,
    request: &EngineRequest,
    cancellation: &CancellationToken,
) -> Result<EngineResult, BackendError> {
    catch_engine_panic(|| engine.execute(request, cancellation))
}

pub(crate) fn execute_engine_with_progress(
    engine: &dyn ImportEngine,
    request: &EngineRequest,
    cancellation: &CancellationToken,
    report_progress: &EngineProgressReporter<'_>,
) -> Result<EngineResult, BackendError> {
    catch_engine_panic(|| engine.execute_with_progress(request, cancellation, report_progress))
}

pub(crate) fn describe_engine(engine: &dyn ImportEngine) -> Result<EngineDescriptor, BackendError> {
    catch_engine_panic(|| Ok(engine.descriptor()))
}

pub(crate) fn engine_supports(
    engine: &dyn ImportEngine,
    input: &ImportInput,
) -> Result<bool, BackendError> {
    catch_engine_panic(|| Ok(engine.supports(input)))
}

fn catch_engine_panic<T>(
    execute: impl FnOnce() -> Result<T, BackendError>,
) -> Result<T, BackendError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(execute))
        .unwrap_or_else(|_| Err(engine_panicked_error()))
}

pub(crate) fn engine_panicked_error() -> BackendError {
    BackendError::new(
        IMPORT_V2_ENGINE_PANICKED,
        "The import engine stopped unexpectedly.",
        true,
        false,
    )
}

#[derive(Default)]
pub struct EngineRegistry {
    engines: RwLock<Vec<Arc<dyn ImportEngine>>>,
}

impl EngineRegistry {
    pub fn registered_routes(&self) -> Result<Vec<String>, BackendError> {
        let engines = self.engines.read().map_err(|_| registry_error())?;
        let mut routes = Vec::with_capacity(engines.len());
        for engine in engines.iter() {
            routes.push(describe_engine(engine.as_ref())?.route);
        }
        Ok(routes)
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

    pub(crate) fn replace_registered(
        &self,
        engine: Arc<dyn ImportEngine>,
    ) -> Result<(), BackendError> {
        self.replace_registered_batch_transaction(vec![engine], || Ok(()))
    }

    pub(crate) fn replace_registered_batch_transaction<F>(
        &self,
        replacements: Vec<Arc<dyn ImportEngine>>,
        commit: F,
    ) -> Result<(), BackendError>
    where
        F: FnOnce() -> Result<(), BackendError>,
    {
        let descriptors = replacements
            .iter()
            .map(|engine| describe_engine(engine.as_ref()))
            .collect::<Result<Vec<_>, _>>()?;
        let mut replacement_ids = HashSet::with_capacity(descriptors.len());
        if descriptors
            .iter()
            .any(|descriptor| !replacement_ids.insert(descriptor.engine_id.clone()))
        {
            return Err(BackendError::new(
                IMPORT_V2_ENGINE_UNAVAILABLE,
                "A capability activation snapshot contains duplicate engine identifiers.",
                true,
                false,
            ));
        }

        let mut engines = self.engines.write().map_err(|_| registry_error())?;
        let previous = engines.clone();
        let mut next = Vec::with_capacity(previous.len() + replacements.len());
        for existing in previous.iter() {
            let descriptor = describe_engine(existing.as_ref())?;
            if !replacement_ids.contains(&descriptor.engine_id) {
                next.push(existing.clone());
            }
        }
        next.extend(replacements);
        *engines = next;
        if let Err(error) = commit() {
            *engines = previous;
            return Err(error);
        }
        Ok(())
    }

    fn register_inner(
        &self,
        engine: Arc<dyn ImportEngine>,
        allow_exact_match: bool,
    ) -> Result<(), BackendError> {
        let descriptor = describe_engine(engine.as_ref())?;
        let mut engines = self.engines.write().map_err(|_| registry_error())?;
        for existing in engines.iter() {
            let existing_descriptor = describe_engine(existing.as_ref())?;
            if allow_exact_match && existing_descriptor == descriptor {
                return Ok(());
            }
            if existing_descriptor.engine_id == descriptor.engine_id {
                return Err(BackendError::new(
                    IMPORT_V2_ENGINE_UNAVAILABLE,
                    "An import engine with this identifier is already registered.",
                    true,
                    false,
                ));
            }
        }
        engines.push(engine);
        Ok(())
    }

    pub fn resolve(&self, input: &ImportInput) -> Result<Arc<dyn ImportEngine>, BackendError> {
        let engines = self.engines.read().map_err(|_| registry_error())?;
        for engine in engines.iter() {
            if engine_supports(engine.as_ref(), input)? {
                return Ok(engine.clone());
            }
        }
        Err(BackendError::new(
            IMPORT_V2_ENGINE_UNAVAILABLE,
            "No installed import engine supports this input.",
            true,
            true,
        ))
    }

    /// Resolve an explicitly planned route; registration order is never routing policy.
    pub fn resolve_route(
        &self,
        route: &str,
        input: &ImportInput,
    ) -> Result<Arc<dyn ImportEngine>, BackendError> {
        let engines = self.engines.read().map_err(|_| registry_error())?;
        let mut selected: Option<(bool, Arc<dyn ImportEngine>)> = None;
        for engine in engines.iter() {
            let descriptor = describe_engine(engine.as_ref())?;
            if descriptor.route != route || !engine_supports(engine.as_ref(), input)? {
                continue;
            }
            // Built-ins are safe fallbacks. Prefer an installed capability pack
            // when it provides the same planned route (notably browser/web).
            let is_builtin = descriptor.engine_id.starts_with("builtin.");
            if selected
                .as_ref()
                .is_none_or(|(selected_is_builtin, _)| *selected_is_builtin && !is_builtin)
            {
                selected = Some((is_builtin, engine.clone()));
            }
        }
        selected.map(|(_, engine)| engine).ok_or_else(|| {
            BackendError::new(
                IMPORT_V2_ENGINE_UNAVAILABLE,
                "The planned import route is not installed.",
                true,
                true,
            )
        })
    }

    pub fn resolve_media_asr(
        &self,
        input: &ImportInput,
        profile: Option<&ImportAsrProfile>,
    ) -> Result<Arc<dyn ImportEngine>, BackendError> {
        let engines = self.engines.read().map_err(|_| registry_error())?;
        let mut selected: Option<((bool, String), Arc<dyn ImportEngine>)> = None;
        for engine in engines.iter() {
            let descriptor = describe_engine(engine.as_ref())?;
            if descriptor.route != "media.asr" || !engine_supports(engine.as_ref(), input)? {
                continue;
            }
            let id = descriptor.engine_id;
            let key = match profile.unwrap_or(&ImportAsrProfile::Balanced) {
                ImportAsrProfile::Accurate => (!id.contains("whisper"), id),
                ImportAsrProfile::Fast | ImportAsrProfile::Balanced => {
                    (!id.contains("sensevoice"), id)
                }
            };
            if selected
                .as_ref()
                .is_none_or(|(selected_key, _)| key < *selected_key)
            {
                selected = Some((key, engine.clone()));
            }
        }
        selected.map(|(_, engine)| engine).ok_or_else(|| {
            BackendError::new(
                IMPORT_V2_ENGINE_UNAVAILABLE,
                "No installed local ASR engine supports this media.",
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
    use std::sync::atomic::{AtomicUsize, Ordering};
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

    struct PanickingEngine;

    impl ImportEngine for PanickingEngine {
        fn descriptor(&self) -> EngineDescriptor {
            EngineDescriptor {
                engine_id: "panicking.fixture".into(),
                engine_version: "1.0.0".into(),
                route: "fixture".into(),
            }
        }

        fn supports(&self, _input: &ImportInput) -> bool {
            true
        }

        fn execute(
            &self,
            _request: &EngineRequest,
            _cancellation: &CancellationToken,
        ) -> Result<EngineResult, BackendError> {
            panic!("injected engine panic");
        }
    }

    struct PanickingSupportsEngine;

    impl ImportEngine for PanickingSupportsEngine {
        fn descriptor(&self) -> EngineDescriptor {
            EngineDescriptor {
                engine_id: "panicking-supports.fixture".into(),
                engine_version: "1.0.0".into(),
                route: "fixture".into(),
            }
        }

        fn supports(&self, _input: &ImportInput) -> bool {
            panic!("injected supports panic");
        }

        fn execute(
            &self,
            _request: &EngineRequest,
            _cancellation: &CancellationToken,
        ) -> Result<EngineResult, BackendError> {
            unreachable!("the registry must reject the engine before execution")
        }
    }

    struct DescriptorPanicsAfterRegistration {
        calls: AtomicUsize,
    }

    impl ImportEngine for DescriptorPanicsAfterRegistration {
        fn descriptor(&self) -> EngineDescriptor {
            if self.calls.fetch_add(1, Ordering::SeqCst) > 0 {
                panic!("injected descriptor panic");
            }
            EngineDescriptor {
                engine_id: "panicking-descriptor.fixture".into(),
                engine_version: "1.0.0".into(),
                route: "fixture".into(),
            }
        }

        fn supports(&self, _input: &ImportInput) -> bool {
            true
        }

        fn execute(
            &self,
            _request: &EngineRequest,
            _cancellation: &CancellationToken,
        ) -> Result<EngineResult, BackendError> {
            unreachable!("the registry must reject the engine before execution")
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

    fn fixture_request() -> EngineRequest {
        EngineRequest {
            protocol_version: "2".into(),
            request_id: "request".into(),
            project_id: "project".into(),
            session_id: "session".into(),
            item_id: "item".into(),
            task_id: "task".into(),
            operation: EngineOperation::Extract,
            input: ImportInput {
                source_identity: None,
                kind: ImportInputKind::File,
                display_name: "fixture.txt".into(),
                locator: "fixture.txt".into(),
                normalized_locator: None,
                media_save_mode: Default::default(),
            },
            project_root: "root".into(),
            staging_root: "staging".into(),
            chained_input: None,
            local_asr_authorized: false,
            asr_probe_only: false,
            asr_profile: None,
            recognition_language: None,
            selected_subtitle: None,
            local_ocr_authorized: false,
            media_save_mode: Default::default(),
        }
    }

    #[test]
    fn engine_panics_become_recoverable_backend_errors() {
        let engine = PanickingEngine;
        let request = fixture_request();
        let cancellation = CancellationToken::new();

        for error in [
            execute_engine(&engine, &request, &cancellation).unwrap_err(),
            execute_engine_with_progress(&engine, &request, &cancellation, &|_| Ok(()))
                .unwrap_err(),
        ] {
            assert_eq!(error.code, crate::errors::IMPORT_V2_ENGINE_PANICKED);
            assert!(error.recoverable);
            assert!(!error.user_action_required);
        }
    }

    #[test]
    fn registry_metadata_panics_become_recoverable_backend_errors() {
        let input = fixture_request().input;
        let supports_registry = EngineRegistry::default();
        supports_registry
            .register(Arc::new(PanickingSupportsEngine))
            .unwrap();
        let supports_error = match supports_registry.resolve(&input) {
            Err(error) => error,
            Ok(_) => panic!("a panicking supports call must not resolve an engine"),
        };
        assert_eq!(
            supports_error.code,
            crate::errors::IMPORT_V2_ENGINE_PANICKED
        );

        let descriptor_registry = EngineRegistry::default();
        descriptor_registry
            .register(Arc::new(DescriptorPanicsAfterRegistration {
                calls: AtomicUsize::new(0),
            }))
            .unwrap();
        let descriptor_error = descriptor_registry.registered_routes().unwrap_err();
        assert_eq!(
            descriptor_error.code,
            crate::errors::IMPORT_V2_ENGINE_PANICKED
        );
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
    fn registry_can_explicitly_replace_a_capability_engine() {
        let registry = EngineRegistry::default();
        let mut production = FixtureEngine::with_route("pack.media", "media.asr", true);
        production.descriptor.engine_version = "1.0.0".into();
        registry.ensure_registered(Arc::new(production)).unwrap();
        let mut development = FixtureEngine::with_route("pack.media", "media.asr", true);
        development.descriptor.engine_version = "2.0.0-dev".into();

        registry.replace_registered(Arc::new(development)).unwrap();

        let input = ImportInput {
            source_identity: None,
            kind: ImportInputKind::File,
            display_name: "recording.wav".into(),
            locator: "recording.wav".into(),
            normalized_locator: None,
            media_save_mode: Default::default(),
        };
        assert_eq!(
            registry
                .resolve_route("media.asr", &input)
                .unwrap()
                .descriptor()
                .engine_version,
            "2.0.0-dev"
        );
    }

    #[test]
    fn registry_publishes_all_capability_routes_or_restores_the_previous_snapshot() {
        let registry = EngineRegistry::default();
        let old = |route: &str| {
            let mut engine =
                FixtureEngine::with_route(&format!("pack.browser-runtime.{route}"), route, true);
            engine.descriptor.engine_version = "1.0.0".into();
            Arc::new(engine) as Arc<dyn ImportEngine>
        };
        let new = |route: &str| {
            let mut engine =
                FixtureEngine::with_route(&format!("pack.browser-runtime.{route}"), route, true);
            engine.descriptor.engine_version = "2.0.0".into();
            Arc::new(engine) as Arc<dyn ImportEngine>
        };
        let routes = ["web.generic.browser", "web.wechat.article", "web.x.post"];
        for route in routes {
            registry.ensure_registered(old(route)).unwrap();
        }
        let input = fixture_request().input;

        let error = registry
            .replace_registered_batch_transaction(routes.into_iter().map(new).collect(), || {
                Err(BackendError::new(
                    "ACTIVATION_JOURNAL_COMMIT_FAILED",
                    "fixture",
                    true,
                    false,
                ))
            })
            .unwrap_err();
        assert_eq!(error.code, "ACTIVATION_JOURNAL_COMMIT_FAILED");
        for route in routes {
            assert_eq!(
                registry
                    .resolve_route(route, &input)
                    .unwrap()
                    .descriptor()
                    .engine_version,
                "1.0.0"
            );
        }

        registry
            .replace_registered_batch_transaction(routes.into_iter().map(new).collect(), || Ok(()))
            .unwrap();
        for route in routes {
            assert_eq!(
                registry
                    .resolve_route(route, &input)
                    .unwrap()
                    .descriptor()
                    .engine_version,
                "2.0.0"
            );
        }
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
            asr_probe_only: false,
            asr_profile: None,
            recognition_language: None,
            selected_subtitle: None,
            local_ocr_authorized: false,
            media_save_mode: Default::default(),
        })
        .unwrap();
        assert_eq!(value["chainedInput"], "converted/legacy.docx");
    }
}
