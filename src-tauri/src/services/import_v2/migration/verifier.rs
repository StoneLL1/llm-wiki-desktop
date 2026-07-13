use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const REQUIRED_IMPORT_V2_CONTRACT: &str = "import-v2-core-v2";
const REQUIRED_PACKAGES: [&str; 4] = ["core", "file", "web", "agent"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PackageGateEvidence {
    pub package: String,
    pub contract_version: String,
    pub release_gate_passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExternalToolLicenseEvidence {
    pub name: String,
    pub license: String,
    pub version: String,
    pub platform: String,
    pub hash_or_signature: String,
    pub size_bytes: u64,
    pub fallback: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct MigrationReadinessEvidence {
    pub core_recovery_passed: bool,
    pub package_gates: Vec<PackageGateEvidence>,
    pub fixture_matrix_passed: bool,
    pub idempotence_passed: bool,
    pub legacy_immutability_passed: bool,
    pub long_task_recovery_passed: bool,
    pub license_evidence: Vec<ExternalToolLicenseEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationResult {
    pub passed: bool,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CutoverVerifier;

impl CutoverVerifier {
    pub fn verify(&self, evidence: &MigrationReadinessEvidence) -> VerificationResult {
        let mut blockers = Vec::new();
        let mut warnings = Vec::new();
        if !evidence.core_recovery_passed {
            blockers.push("Core recovery/invariant evidence is missing or failed.".into());
        }
        let mut seen = BTreeSet::new();
        for gate in &evidence.package_gates {
            seen.insert(gate.package.clone());
            if !REQUIRED_PACKAGES.contains(&gate.package.as_str()) {
                warnings.push(format!("Unrecognized package gate evidence: {}", gate.package));
            }
            if gate.contract_version != REQUIRED_IMPORT_V2_CONTRACT {
                blockers.push(format!(
                    "Package {} does not use contract {}.",
                    gate.package, REQUIRED_IMPORT_V2_CONTRACT
                ));
            }
            if !gate.release_gate_passed {
                blockers.push(format!("Package {} release gate is not passed.", gate.package));
            }
        }
        for package in REQUIRED_PACKAGES {
            if !seen.contains(package) {
                blockers.push(format!("Missing release gate evidence for package {package}."));
            }
        }
        if !evidence.fixture_matrix_passed {
            blockers.push("Hostile migration fixture matrix is incomplete.".into());
        }
        if !evidence.idempotence_passed {
            blockers.push("Dry-run/apply/resume idempotence evidence is incomplete.".into());
        }
        if !evidence.legacy_immutability_passed {
            blockers.push("Legacy content/index immutability evidence is incomplete.".into());
        }
        if !evidence.long_task_recovery_passed {
            blockers.push("Long-task progress/cancellation/restart evidence is incomplete.".into());
        }
        if evidence.license_evidence.is_empty() {
            blockers.push("External tool license/provenance evidence is missing.".into());
        }
        for tool in &evidence.license_evidence {
            let license = tool.license.to_ascii_lowercase();
            if license.contains("gpl")
                || license.contains("agpl")
                || license.contains("non-commercial")
                || license.contains("noncommercial")
            {
                blockers.push(format!("Disallowed license evidence for {}.", tool.name));
            }
            if tool.name.trim().is_empty()
                || tool.version.trim().is_empty()
                || tool.platform.trim().is_empty()
                || tool.hash_or_signature.trim().is_empty()
                || tool.size_bytes == 0
                || tool.fallback.trim().is_empty()
            {
                blockers.push(format!(
                    "Incomplete external tool provenance for {}.",
                    if tool.name.is_empty() { "unnamed tool" } else { &tool.name }
                ));
            }
        }
        VerificationResult {
            passed: blockers.is_empty(),
            blockers,
            warnings,
        }
    }
}
