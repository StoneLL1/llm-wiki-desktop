use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::errors::{BackendError, IMPORT_V2_CAPABILITY_INVALID};

pub const OCR_PREPROCESS_VERSION: &str = "ocr-page-300dpi-v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OcrLayoutHint {
    Auto,
    SingleColumn,
    MultiColumn,
    Table,
    SparseText,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OcrQualityThresholds {
    pub minimum_character_accuracy: f64,
    pub minimum_table_cell_accuracy: f64,
    pub minimum_block_confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OcrRequest {
    /// Paths are supplied by Import Core and must point to authorized page
    /// images. Capability processes receive no project destination paths.
    pub page_image_paths: Vec<String>,
    pub languages: Vec<String>,
    pub layout_hints: Vec<OcrLayoutHint>,
    pub thresholds: OcrQualityThresholds,
}

impl OcrRequest {
    pub fn validate_staging_paths(&self, staging_root: &Path) -> Result<(), BackendError> {
        let root = fs::canonicalize(staging_root).map_err(|_| invalid_request())?;
        if self.page_image_paths.is_empty() || self.languages.is_empty() {
            return Err(invalid_request());
        }
        for page in &self.page_image_paths {
            let relative = Path::new(page);
            if relative.is_absolute()
                || relative
                    .components()
                    .any(|part| !matches!(part, Component::Normal(_)))
            {
                return Err(invalid_request());
            }
            let page = fs::canonicalize(root.join(relative)).map_err(|_| invalid_request())?;
            if !page.starts_with(&root) || !page.is_file() {
                return Err(invalid_request());
            }
        }
        Ok(())
    }

    pub fn cache_material(
        &self,
        input_sha256: &str,
        engine_version: &str,
        model_sha256: &str,
    ) -> String {
        let languages = self
            .languages
            .iter()
            .map(|language| language.trim().to_ascii_lowercase())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "input={input_sha256}\npreprocess={OCR_PREPROCESS_VERSION}\nengine={engine_version}\nlanguages={languages}\nmodel={model_sha256}"
        )
    }

    pub fn cache_key(
        &self,
        input_sha256: &str,
        engine_version: &str,
        model_sha256: &str,
    ) -> String {
        format!(
            "{:x}",
            Sha256::digest(self.cache_material(input_sha256, engine_version, model_sha256))
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OcrCoordinates {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OcrBlock {
    pub text: String,
    pub confidence: f64,
    pub coordinates: OcrCoordinates,
    pub table_cell: Option<OcrCoordinates>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OcrPageResult {
    pub page_index: u32,
    pub blocks: Vec<OcrBlock>,
    pub confidence: f64,
    pub engine_id: String,
    pub engine_version: String,
    pub model_version: String,
    pub model_sha256: String,
}

impl OcrPageResult {
    pub fn meets_confidence_threshold(&self, thresholds: &OcrQualityThresholds) -> bool {
        self.confidence >= thresholds.minimum_block_confidence
            && self
                .blocks
                .iter()
                .all(|block| block.confidence >= thresholds.minimum_block_confidence)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OcrPackAvailability {
    Installed,
    NotInstalled,
    UnsupportedPlatform,
    DownloadInterrupted,
    InvalidModel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OcrPackMetadata {
    pub pack_id: String,
    pub engine_version: String,
    pub model_version: String,
    pub model_sha256: String,
    pub languages: Vec<String>,
    pub license_expression: String,
    pub download_bytes: u64,
    pub installed_bytes: u64,
    pub availability: OcrPackAvailability,
}

impl OcrPackMetadata {
    fn installed_and_supports(&self, languages: &[String]) -> bool {
        self.availability == OcrPackAvailability::Installed
            && languages.iter().all(|requested| {
                self.languages
                    .iter()
                    .any(|available| available.eq_ignore_ascii_case(requested))
            })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OcrRoute {
    Basic,
    CjkAccurate,
    WaitingCapability,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OcrCapabilityRequirement {
    pub capability_id: String,
    pub license_expression: String,
    pub download_bytes: u64,
    pub installed_bytes: u64,
    pub required_disk_bytes: u64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OcrRoutePlan {
    pub route: OcrRoute,
    pub requirement: Option<OcrCapabilityRequirement>,
    pub cloud_fallback: bool,
    pub reason: String,
}

pub struct OcrRouter {
    basic: OcrPackMetadata,
    accurate: OcrPackMetadata,
}

impl OcrRouter {
    pub fn new(basic: OcrPackMetadata, accurate: OcrPackMetadata) -> Self {
        Self { basic, accurate }
    }

    pub fn plan(&self, request: &OcrRequest) -> OcrRoutePlan {
        let wants_cjk = request
            .languages
            .iter()
            .any(|language| language.to_ascii_lowercase().starts_with("chi_"));
        let wants_layout = request
            .layout_hints
            .iter()
            .any(|hint| matches!(hint, OcrLayoutHint::Table | OcrLayoutHint::MultiColumn));
        if (wants_cjk || wants_layout) && self.accurate.installed_and_supports(&request.languages) {
            return ready(
                OcrRoute::CjkAccurate,
                "accurate local CJK/layout OCR selected",
            );
        }
        if !wants_layout && self.basic.installed_and_supports(&request.languages) {
            return ready(OcrRoute::Basic, "pinned local Tesseract OCR selected");
        }
        let pack = if wants_cjk || wants_layout {
            &self.accurate
        } else {
            &self.basic
        };
        OcrRoutePlan {
            route: OcrRoute::WaitingCapability,
            requirement: Some(OcrCapabilityRequirement {
                capability_id: pack.pack_id.clone(),
                license_expression: pack.license_expression.clone(),
                download_bytes: pack.download_bytes,
                installed_bytes: pack.installed_bytes,
                required_disk_bytes: pack.installed_bytes.saturating_add(pack.download_bytes),
                reason: availability_reason(&pack.availability).into(),
            }),
            cloud_fallback: false,
            reason: "required verified local OCR pack is unavailable".into(),
        }
    }
}

fn ready(route: OcrRoute, reason: &str) -> OcrRoutePlan {
    OcrRoutePlan {
        route,
        requirement: None,
        cloud_fallback: false,
        reason: reason.into(),
    }
}

fn availability_reason(availability: &OcrPackAvailability) -> &'static str {
    match availability {
        OcrPackAvailability::Installed => "requested languages are not installed",
        OcrPackAvailability::NotInstalled => "capability pack is not installed",
        OcrPackAvailability::UnsupportedPlatform => "platform has no CI-proven build",
        OcrPackAvailability::DownloadInterrupted => "capability download was interrupted",
        OcrPackAvailability::InvalidModel => "model hash validation failed",
    }
}

pub fn validate_model(path: &Path, expected_sha256: &str) -> Result<(), BackendError> {
    if expected_sha256.len() != 64 || !expected_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(invalid_model());
    }
    let bytes = fs::read(path).map_err(|_| invalid_model())?;
    let actual = format!("{:x}", Sha256::digest(bytes));
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        return Err(invalid_model());
    }
    Ok(())
}

pub fn character_accuracy(expected: &str, actual: &str) -> f64 {
    let expected = expected.chars().collect::<Vec<_>>();
    let actual = actual.chars().collect::<Vec<_>>();
    if expected.is_empty() {
        return f64::from(actual.is_empty());
    }
    let distance = levenshtein(&expected, &actual);
    1.0 - distance.min(expected.len()) as f64 / expected.len() as f64
}

pub fn non_empty_table_cell_accuracy(expected: &[&str], actual: &[&str]) -> f64 {
    let expected = expected
        .iter()
        .filter(|cell| !cell.trim().is_empty())
        .collect::<Vec<_>>();
    if expected.is_empty() {
        return 1.0;
    }
    let correct = expected
        .iter()
        .zip(actual.iter().filter(|cell| !cell.trim().is_empty()))
        .filter(|(left, right)| left.trim() == right.trim())
        .count();
    correct as f64 / expected.len() as f64
}

fn levenshtein(left: &[char], right: &[char]) -> usize {
    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    for (left_index, left_char) in left.iter().enumerate() {
        let mut current = vec![left_index + 1];
        for (right_index, right_char) in right.iter().enumerate() {
            current.push(
                (previous[right_index + 1] + 1)
                    .min(current[right_index] + 1)
                    .min(previous[right_index] + usize::from(left_char != right_char)),
            );
        }
        previous = current;
    }
    previous[right.len()]
}

fn invalid_model() -> BackendError {
    BackendError::new(
        IMPORT_V2_CAPABILITY_INVALID,
        "The OCR model is missing or does not match its pinned SHA-256 hash.",
        false,
        true,
    )
}

fn invalid_request() -> BackendError {
    BackendError::new(
        IMPORT_V2_CAPABILITY_INVALID,
        "The OCR request contains a page path outside its authorized item staging directory.",
        false,
        false,
    )
}
