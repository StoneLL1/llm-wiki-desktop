use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::errors::{BackendError, IMPORT_V2_QUALITY_FAILED};
use crate::models::import_v2::{
    ArtifactKind, ImportArtifact, ImportPreviewArtifact, QualityLevel, QualityMetric, QualityReport,
};
use crate::services::import_v2::engine::EngineResult;

pub const MIN_TEXT_COVERAGE: f64 = 0.98;
pub const MIN_TABLE_CELL_ACCURACY: f64 = 0.95;

#[derive(Default)]
pub struct QualityGate;

impl QualityGate {
    pub fn evaluate(
        &self,
        staging_root: &Path,
        result: &EngineResult,
    ) -> Result<ImportPreviewArtifact, BackendError> {
        let markdown = read_artifact(staging_root, &result.markdown_path, ArtifactKind::Markdown)?;
        let markdown_content =
            String::from_utf8(markdown.bytes.clone()).map_err(|_| quality_error())?;
        validate_markdown_content(&markdown_content)?;

        let source_snapshot = read_artifact(
            staging_root,
            &result.source_snapshot_path,
            ArtifactKind::SourceSnapshot,
        )?;
        let mut assets = Vec::with_capacity(result.asset_paths.len());
        let declared_assets: HashSet<String> = result
            .asset_paths
            .iter()
            .map(|path| normalize_relative(path))
            .collect::<Result<_, _>>()?;
        if declared_assets.len() != result.asset_paths.len() {
            return Err(quality_error());
        }
        for asset_path in &result.asset_paths {
            assets.push(
                read_artifact(staging_root, asset_path, classify_asset(asset_path))?.artifact,
            );
        }

        let mut warnings = result.warnings.clone();
        let local_images = image_destinations(&markdown_content);
        for destination in local_images {
            if is_remote(&destination) {
                push_warning(&mut warnings, "REMOTE_IMAGE");
            } else {
                let normalized = normalize_relative(destination.split('#').next().unwrap_or(""))?;
                if !declared_assets.contains(&normalized) {
                    return Err(quality_error());
                }
            }
        }

        let mut metrics = Vec::new();
        push_metric(
            &mut metrics,
            &mut warnings,
            "TEXT_COVERAGE",
            result.text_coverage,
            MIN_TEXT_COVERAGE,
        );
        push_metric(
            &mut metrics,
            &mut warnings,
            "TABLE_CELL_ACCURACY",
            result.table_cell_accuracy,
            MIN_TABLE_CELL_ACCURACY,
        );
        let level = if warnings.is_empty() {
            QualityLevel::Pass
        } else {
            QualityLevel::Warning
        };

        Ok(ImportPreviewArtifact {
            markdown: markdown.artifact,
            assets,
            source_snapshot: source_snapshot.artifact,
            quality: QualityReport {
                level,
                metrics,
                warnings,
            },
            title: result.title.clone(),
        })
    }
}

struct ReadArtifact {
    artifact: ImportArtifact,
    bytes: Vec<u8>,
}

fn read_artifact(
    staging_root: &Path,
    relative_path: &str,
    kind: ArtifactKind,
) -> Result<ReadArtifact, BackendError> {
    let normalized = normalize_relative(relative_path)?;
    let root = staging_root.canonicalize().map_err(|_| quality_error())?;
    let path = root.join(Path::new(&normalized));
    let canonical = path.canonicalize().map_err(|_| quality_error())?;
    if !canonical.starts_with(&root) || !canonical.is_file() {
        return Err(quality_error());
    }
    let before = std::fs::metadata(&canonical).map_err(|_| quality_error())?;
    let bytes = std::fs::read(&canonical).map_err(|_| quality_error())?;
    let after = std::fs::metadata(&canonical).map_err(|_| quality_error())?;
    if before.len() != bytes.len() as u64 || before.len() != after.len() {
        return Err(quality_error());
    }
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    Ok(ReadArtifact {
        artifact: ImportArtifact {
            kind,
            relative_path: normalized,
            sha256,
            size_bytes: bytes.len() as u64,
        },
        bytes,
    })
}

fn normalize_relative(value: &str) -> Result<String, BackendError> {
    let normalized = value.replace('\\', "/");
    let path = Path::new(&normalized);
    if normalized.trim().is_empty()
        || normalized.contains(':')
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir
                    | Component::CurDir
                    | Component::RootDir
                    | Component::Prefix(_)
            )
        })
    {
        return Err(quality_error());
    }
    Ok(normalized)
}

fn validate_markdown_content(markdown: &str) -> Result<(), BackendError> {
    let lowercase = markdown.to_ascii_lowercase();
    if markdown.trim().is_empty()
        || lowercase.contains("<script")
        || lowercase.contains("javascript:")
        || lowercase.contains("data:text/html")
    {
        return Err(quality_error());
    }
    Ok(())
}

fn image_destinations(markdown: &str) -> Vec<String> {
    let mut destinations = Vec::new();
    let mut rest = markdown;
    while let Some(image_start) = rest.find("![") {
        rest = &rest[image_start + 2..];
        let Some(label_end) = rest.find("](") else {
            continue;
        };
        rest = &rest[label_end + 2..];
        let Some(destination_end) = rest.find(')') else {
            continue;
        };
        let raw = rest[..destination_end].trim();
        let destination = raw
            .strip_prefix('<')
            .and_then(|value| value.split_once('>').map(|pair| pair.0))
            .unwrap_or_else(|| raw.split_ascii_whitespace().next().unwrap_or(""));
        if !destination.is_empty() {
            destinations.push(destination.to_string());
        }
        rest = &rest[destination_end + 1..];
    }
    destinations
}

fn is_remote(destination: &str) -> bool {
    let lowercase = destination.to_ascii_lowercase();
    lowercase.starts_with("http://") || lowercase.starts_with("https://")
}

fn classify_asset(path: &str) -> ArtifactKind {
    let extension = PathBuf::from(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(
        extension.as_str(),
        "avif" | "gif" | "jpeg" | "jpg" | "png" | "svg" | "webp"
    ) {
        ArtifactKind::Image
    } else if matches!(extension.as_str(), "srt" | "vtt") {
        ArtifactKind::Subtitle
    } else {
        ArtifactKind::Attachment
    }
}

fn push_metric(
    metrics: &mut Vec<QualityMetric>,
    warnings: &mut Vec<String>,
    code: &str,
    actual: Option<f64>,
    minimum: f64,
) {
    if let Some(actual) = actual.filter(|value| value.is_finite()) {
        let passed = actual >= minimum;
        metrics.push(QualityMetric {
            code: code.to_string(),
            actual,
            minimum,
            passed,
        });
        if !passed {
            let warning = match code {
                "TABLE_CELL_ACCURACY" => "LOW_TABLE_ACCURACY".to_string(),
                _ => format!("LOW_{code}"),
            };
            push_warning(warnings, &warning);
        }
    }
}

fn push_warning(warnings: &mut Vec<String>, warning: &str) {
    if !warnings.iter().any(|existing| existing == warning) {
        warnings.push(warning.to_string());
    }
}

fn quality_error() -> BackendError {
    BackendError::new(
        IMPORT_V2_QUALITY_FAILED,
        "Generated import artifacts failed deterministic quality validation.",
        true,
        true,
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::errors::IMPORT_V2_QUALITY_FAILED;
    use crate::models::import_v2::{ArtifactKind, QualityLevel};
    use crate::services::import_v2::engine::EngineResult;

    struct QualityFixture {
        root: PathBuf,
        result: EngineResult,
    }

    impl Drop for QualityFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn quality_fixture(markdown: &str) -> QualityFixture {
        quality_fixture_with_metrics(markdown, 1.0, 1.0)
    }

    fn quality_fixture_with_metrics(markdown: &str, text: f64, table: f64) -> QualityFixture {
        let root = std::env::temp_dir().join(format!("quality-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("candidate.md"), markdown).unwrap();
        std::fs::write(root.join("source.bin"), b"source").unwrap();
        QualityFixture {
            root,
            result: EngineResult {
                source_snapshot_path: "source.bin".into(),
                markdown_path: "candidate.md".into(),
                asset_paths: Vec::new(),
                title: "Fixture".into(),
                text_coverage: Some(text),
                table_cell_accuracy: Some(table),
                warnings: Vec::new(),
            },
        }
    }

    #[test]
    fn quality_gate_rejects_unsafe_or_empty_markdown() {
        for markdown in [
            "   ",
            "<script>alert(1)</script>",
            "[x](javascript:alert(1))",
            "[x](data:text/html,bad)",
        ] {
            let fixture = quality_fixture(markdown);
            let error = QualityGate::default()
                .evaluate(&fixture.root, &fixture.result)
                .unwrap_err();
            assert_eq!(error.code, IMPORT_V2_QUALITY_FAILED);
        }
    }

    #[test]
    fn quality_gate_warns_but_allows_low_coverage_preview() {
        let fixture = quality_fixture_with_metrics("# 标题\n\n正文", 0.91, 0.93);
        let preview = QualityGate::default()
            .evaluate(&fixture.root, &fixture.result)
            .unwrap();
        assert_eq!(preview.quality.level, QualityLevel::Warning);
        assert!(preview
            .quality
            .warnings
            .iter()
            .any(|warning| warning == "LOW_TEXT_COVERAGE"));
        assert!(preview
            .quality
            .warnings
            .iter()
            .any(|warning| warning == "LOW_TABLE_ACCURACY"));
    }

    #[test]
    fn quality_gate_requires_declared_existing_local_assets() {
        let mut missing = quality_fixture("![图](assets/图.png)");
        missing.result.asset_paths = vec!["assets/图.png".into()];
        assert_eq!(
            QualityGate::default()
                .evaluate(&missing.root, &missing.result)
                .unwrap_err()
                .code,
            IMPORT_V2_QUALITY_FAILED
        );

        let undeclared = quality_fixture("![图](assets/图.png)");
        std::fs::create_dir_all(undeclared.root.join("assets")).unwrap();
        std::fs::write(undeclared.root.join("assets/图.png"), b"png").unwrap();
        assert_eq!(
            QualityGate::default()
                .evaluate(&undeclared.root, &undeclared.result)
                .unwrap_err()
                .code,
            IMPORT_V2_QUALITY_FAILED
        );
    }

    #[test]
    fn quality_gate_rejects_path_escape_and_missing_source_snapshot() {
        let mut escaped = quality_fixture("# 标题");
        escaped.result.markdown_path = "../outside.md".into();
        assert_eq!(
            QualityGate::default()
                .evaluate(&escaped.root, &escaped.result)
                .unwrap_err()
                .code,
            IMPORT_V2_QUALITY_FAILED
        );

        let missing = quality_fixture("# 标题");
        std::fs::remove_file(missing.root.join("source.bin")).unwrap();
        assert_eq!(
            QualityGate::default()
                .evaluate(&missing.root, &missing.result)
                .unwrap_err()
                .code,
            IMPORT_V2_QUALITY_FAILED
        );
    }

    #[test]
    fn quality_gate_hashes_and_sizes_all_artifacts_and_preserves_cjk() {
        let mut fixture = quality_fixture("# 标题\n\n![图](assets/图.png)");
        std::fs::create_dir_all(fixture.root.join("assets")).unwrap();
        std::fs::write(fixture.root.join("assets/图.png"), b"image").unwrap();
        fixture.result.asset_paths = vec!["assets/图.png".into()];

        let preview = QualityGate::default()
            .evaluate(&fixture.root, &fixture.result)
            .unwrap();

        assert_eq!(preview.title, "Fixture");
        assert_eq!(preview.markdown.kind, ArtifactKind::Markdown);
        assert_eq!(preview.markdown.size_bytes, 32);
        assert_eq!(preview.assets[0].relative_path, "assets/图.png");
        assert_eq!(preview.assets[0].size_bytes, 5);
        assert_eq!(
            preview.assets[0].sha256,
            "6105d6cc76af400325e94d588ce511be5bfdbb73b437dc51eca43917d7a43e3d"
        );
        assert_eq!(preview.source_snapshot.size_bytes, 6);
    }

    #[test]
    fn quality_gate_warns_for_remote_images_and_engine_warnings() {
        let mut fixture = quality_fixture("# 标题\n\n![remote](https://example.com/image.png)");
        fixture.result.warnings.push("ENGINE_OCR_PARTIAL".into());
        let preview = QualityGate::default()
            .evaluate(&fixture.root, &fixture.result)
            .unwrap();
        assert_eq!(preview.quality.level, QualityLevel::Warning);
        assert!(preview
            .quality
            .warnings
            .iter()
            .any(|warning| warning == "REMOTE_IMAGE"));
        assert!(preview
            .quality
            .warnings
            .iter()
            .any(|warning| warning == "ENGINE_OCR_PARTIAL"));
    }
}
