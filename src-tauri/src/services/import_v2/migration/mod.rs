mod apply;
mod legacy_history;
mod planner;
mod scanner;
mod verifier;

pub use apply::MigrationService;
pub use legacy_history::{LegacyHistoryAdapter, LegacyHistoryLimits};
pub use planner::{DefaultMigrationPlanner, MigrationPlanner};
pub use scanner::{DefaultLegacyScanner, LegacyScanner, ScannerLimits};
pub use verifier::{
    CutoverVerifier, ExternalToolLicenseEvidence, MigrationReadinessEvidence, PackageGateEvidence,
    VerificationResult, REQUIRED_IMPORT_V2_CONTRACT,
};
