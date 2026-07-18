use llm_wiki_desktop_lib::services::import_v2::migration::{
    CutoverVerifier, ExternalToolLicenseEvidence, MigrationReadinessEvidence, PackageGateEvidence,
    REQUIRED_IMPORT_V2_CONTRACT,
};

fn passing_evidence() -> MigrationReadinessEvidence {
    MigrationReadinessEvidence {
        core_recovery_passed: true,
        package_gates: ["core", "file", "web", "agent"]
            .into_iter()
            .map(|package| PackageGateEvidence {
                package: package.into(),
                contract_version: REQUIRED_IMPORT_V2_CONTRACT.into(),
                release_gate_passed: true,
            })
            .collect(),
        fixture_matrix_passed: true,
        idempotence_passed: true,
        legacy_immutability_passed: true,
        long_task_recovery_passed: true,
        license_evidence: vec![ExternalToolLicenseEvidence {
            name: "builtin".into(),
            license: "MIT".into(),
            version: "1".into(),
            platform: "windows".into(),
            hash_or_signature: "sha256:abc".into(),
            size_bytes: 1,
            fallback: "native".into(),
        }],
    }
}

#[test]
fn readiness_verifier_accepts_complete_compatible_evidence() {
    let result = CutoverVerifier::default().verify(&passing_evidence());
    assert!(result.passed, "{result:?}");
    assert!(result.blockers.is_empty());
}

#[test]
fn readiness_verifier_blocks_missing_package_evidence_and_bad_licenses() {
    let mut evidence = passing_evidence();
    evidence.package_gates.retain(|gate| gate.package != "web");
    evidence.license_evidence[0].license = "GPL-3.0".into();
    let result = CutoverVerifier::default().verify(&evidence);
    assert!(!result.passed);
    assert!(result
        .blockers
        .iter()
        .any(|blocker| blocker.contains("web")));
    assert!(result
        .blockers
        .iter()
        .any(|blocker| blocker.contains("license")));
}

#[test]
fn readiness_verifier_rejects_missing_external_tool_provenance() {
    let mut evidence = passing_evidence();
    evidence.license_evidence[0].hash_or_signature.clear();
    evidence.license_evidence[0].fallback.clear();
    let result = CutoverVerifier::default().verify(&evidence);
    assert!(!result.passed);
    assert!(result
        .blockers
        .iter()
        .any(|blocker| blocker.contains("provenance")));
}
