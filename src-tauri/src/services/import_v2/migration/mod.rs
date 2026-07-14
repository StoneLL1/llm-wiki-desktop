mod scanner;
mod planner;
mod apply;
mod legacy_history;
mod verifier;

pub use apply::MigrationService;
pub use legacy_history::{LegacyHistoryAdapter, LegacyHistoryLimits};
pub use verifier::{
    CutoverVerifier, ExternalToolLicenseEvidence, MigrationReadinessEvidence,
    PackageGateEvidence, VerificationResult, REQUIRED_IMPORT_V2_CONTRACT,
};
pub use planner::{DefaultMigrationPlanner, MigrationPlanner};
pub use scanner::{DefaultLegacyScanner, LegacyScanner, ScannerLimits};
