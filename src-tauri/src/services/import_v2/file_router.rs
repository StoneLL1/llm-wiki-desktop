use crate::models::import_v2_file::FileFormat;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CapabilitySnapshot {
    pub document_standard: bool,
    pub office_legacy: bool,
    pub office_oxide_installed: bool,
    /// True only after an independently produced qualification record passes
    /// the critical corpus and security gates on every supported platform.
    pub office_oxide_qualified: bool,
    pub agent_available: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OfficeOxideQualification {
    schema_version: u32,
    critical_assertions_passed: bool,
    security_blockers: u32,
    fuzz_blockers: u32,
    qualified_target_triples: Vec<String>,
}

impl CapabilitySnapshot {
    /// Missing, malformed, incomplete, or platform-mismatched evidence is fail-closed.
    pub fn from_installation(
        document_standard: bool,
        office_oxide_installed: bool,
        qualification_path: &Path,
        target_triple: &str,
        agent_available: bool,
    ) -> Self {
        let office_oxide_qualified = office_oxide_installed
            && std::fs::read(qualification_path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<OfficeOxideQualification>(&bytes).ok())
                .is_some_and(|evidence| {
                    evidence.schema_version == 1
                        && evidence.critical_assertions_passed
                        && evidence.security_blockers == 0
                        && evidence.fuzz_blockers == 0
                        && evidence
                            .qualified_target_triples
                            .iter()
                            .any(|triple| triple == target_triple)
                });
        Self {
            document_standard,
            office_legacy: false,
            office_oxide_installed,
            office_oxide_qualified,
            agent_available,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RouteAttempt {
    pub route: &'static str,
    pub required_pack: Option<&'static str>,
    pub quality_floor: QualityFloor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityFloor {
    ModernOffice,
    DeterministicDocument,
    ComparisonFallback,
    AgentCandidate,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QualityRequirements {
    pub minimum_text_coverage: f32,
    pub require_exact_unit_count: bool,
    pub require_ordered_structure: bool,
    pub require_tables: bool,
    pub require_images: bool,
    pub require_notes_or_footnotes: bool,
    pub require_formula_and_display_value: bool,
}

impl QualityFloor {
    pub fn requirements(self) -> QualityRequirements {
        match self {
            Self::ModernOffice => QualityRequirements {
                minimum_text_coverage: 0.98,
                require_exact_unit_count: true,
                require_ordered_structure: true,
                require_tables: true,
                require_images: true,
                require_notes_or_footnotes: true,
                require_formula_and_display_value: true,
            },
            Self::DeterministicDocument => QualityRequirements {
                minimum_text_coverage: 0.98,
                require_exact_unit_count: false,
                require_ordered_structure: true,
                require_tables: false,
                require_images: false,
                require_notes_or_footnotes: false,
                require_formula_and_display_value: false,
            },
            Self::ComparisonFallback | Self::AgentCandidate => QualityRequirements {
                minimum_text_coverage: 0.0,
                require_exact_unit_count: false,
                require_ordered_structure: false,
                require_tables: false,
                require_images: false,
                require_notes_or_footnotes: false,
                require_formula_and_display_value: false,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum RouteFailure {
    UnsupportedFeature { feature: String },
    CorruptInput,
    ResourceLimit,
    CapabilityUnavailable,
    EngineFailure { code: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum AttemptOutcome {
    Succeeded,
    Failed(RouteFailure),
    QualityRejected { actual: f32, required: f32 },
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AttemptRecord {
    pub route: String,
    pub outcome: AttemptOutcome,
}

impl AttemptRecord {
    pub fn allows_fallback(&self) -> bool {
        matches!(
            self.outcome,
            AttemptOutcome::Failed(_) | AttemptOutcome::QualityRejected { .. }
        )
    }
}

pub struct FileRoutePlanner;
impl FileRoutePlanner {
    pub fn deterministic_routes(
        format: FileFormat,
        capabilities: CapabilitySnapshot,
    ) -> Vec<&'static str> {
        let planned = Self::plan(format, capabilities);
        if !planned.is_empty() {
            return planned.into_iter().map(|attempt| attempt.route).collect();
        }
        match format {
            FileFormat::Markdown => vec!["native.markdown"],
            FileFormat::Pdf => vec!["pack.pdf", "pack.ocr"],
            _ => vec!["native.file"],
        }
    }
    pub fn plan(format: FileFormat, capabilities: CapabilitySnapshot) -> Vec<RouteAttempt> {
        let legacy_target = match format {
            FileFormat::Doc => Some("office.modern.docx"),
            FileFormat::Xls => Some("office.modern.xlsx"),
            FileFormat::Ppt => Some("office.modern.pptx"),
            _ => None,
        };
        if let Some(modern) = legacy_target {
            let mut routes = Vec::new();
            if capabilities.office_legacy {
                routes.push(RouteAttempt {
                    route: "pack.office-legacy",
                    required_pack: Some("office-legacy"),
                    quality_floor: QualityFloor::ModernOffice,
                });
                // This route consumes only the validated OOXML cache artifact emitted by
                // office-legacy; it never changes the legacy source snapshot identity.
                routes.push(RouteAttempt {
                    route: modern,
                    required_pack: None,
                    quality_floor: QualityFloor::ModernOffice,
                });
                if capabilities.document_standard {
                    routes.push(RouteAttempt {
                        route: "pack.markitdown",
                        required_pack: Some("document-standard"),
                        quality_floor: QualityFloor::ComparisonFallback,
                    });
                }
            }
            if capabilities.office_oxide_installed && capabilities.office_oxide_qualified {
                routes.push(RouteAttempt {
                    route: "pack.office-oxide",
                    required_pack: Some("office-oxide"),
                    quality_floor: QualityFloor::ModernOffice,
                });
            }
            if capabilities.agent_available {
                routes.push(RouteAttempt {
                    route: "agent.office",
                    required_pack: None,
                    quality_floor: QualityFloor::AgentCandidate,
                });
            }
            return routes;
        }
        let primary = match format {
            FileFormat::Docx => Some("office.modern.docx"),
            FileFormat::Xlsx => Some("office.modern.xlsx"),
            FileFormat::Pptx => Some("office.modern.pptx"),
            _ => None,
        };
        let Some(primary) = primary else {
            return Vec::new();
        };
        let mut routes = vec![RouteAttempt {
            route: primary,
            required_pack: None,
            quality_floor: QualityFloor::ModernOffice,
        }];
        if capabilities.document_standard {
            routes.push(RouteAttempt {
                route: "pack.markitdown",
                required_pack: Some("document-standard"),
                quality_floor: QualityFloor::ComparisonFallback,
            });
        }
        if capabilities.office_oxide_installed && capabilities.office_oxide_qualified {
            routes.push(RouteAttempt {
                route: "pack.office-oxide",
                required_pack: Some("office-oxide"),
                quality_floor: QualityFloor::ModernOffice,
            });
        }
        if capabilities.agent_available {
            routes.push(RouteAttempt {
                route: "agent.office",
                required_pack: None,
                quality_floor: QualityFloor::AgentCandidate,
            });
        }
        routes
    }

    pub fn record(route: impl Into<String>, outcome: AttemptOutcome) -> AttemptRecord {
        AttemptRecord {
            route: route.into(),
            outcome,
        }
    }
}
