use std::collections::HashSet;

use crate::errors::BackendError;
use crate::models::import_v2::{ImportInput, ImportSession};
use crate::models::import_v2_file::{
    DiscoveredFile, FileScanResult, FileSkipReason, ImportScanConfirmationReason,
    ImportScanIdentity, ImportScanTotals, LargeDataEstimate, SkippedFile,
};
use crate::services::import_v2::file_discovery::{new_import_inputs, FileDiscoveryService};

const AGGREGATE_CONFIRM_FILE_COUNT: u32 = 1_000;
const AGGREGATE_CONFIRM_TOTAL_BYTES: u64 = 1024 * 1024 * 1024;
const AGGREGATE_CONFIRM_OUTPUT_FILES: u64 = 2_000;

#[derive(Debug, PartialEq)]
pub struct ScanStagingPlan {
    pub inputs: Vec<ImportInput>,
    pub aggregate_confirmation_pending: bool,
    pub item_confirmation_pending: bool,
}

#[derive(Debug, PartialEq)]
pub enum SavedScanAcceptance {
    AlreadyAccepted,
    Ready(SavedScanAcceptancePlan),
}

#[derive(Debug, PartialEq)]
pub struct SavedScanAcceptancePlan {
    pub inputs: Vec<ImportInput>,
    pub mark_aggregate_confirmed: bool,
    pub fully_accepted: bool,
}

pub fn prepare_scan_staging(
    scan: &mut FileScanResult,
    session: &ImportSession,
    large_data_confirmed: bool,
    new_confirmation_token: impl FnOnce() -> String,
) -> ScanStagingPlan {
    finalize_scan_totals(scan);
    let aggregate_confirmation_pending = scan.totals.requires_confirmation;
    let item_confirmation_pending =
        scan.files.iter().any(requires_item_confirmation) && !large_data_confirmed;
    if aggregate_confirmation_pending || item_confirmation_pending {
        scan.confirmation_token = Some(new_confirmation_token());
    }
    let importable = if aggregate_confirmation_pending {
        Vec::new()
    } else {
        take_importable_files(scan, large_data_confirmed)
    };
    ScanStagingPlan {
        inputs: new_import_inputs(session, importable),
        aggregate_confirmation_pending,
        item_confirmation_pending,
    }
}

pub fn prepare_legacy_scan_staging(
    scan: &mut FileScanResult,
    session: &ImportSession,
    large_data_confirmed: bool,
) -> Vec<ImportInput> {
    new_import_inputs(session, take_importable_files(scan, large_data_confirmed))
}

pub fn prepare_saved_scan_acceptance(
    scan: &FileScanResult,
    expected_identity: &ImportScanIdentity,
    confirmation_token: &str,
    acknowledge_aggregate: bool,
    source_paths: Option<&[String]>,
    session: &ImportSession,
) -> Result<SavedScanAcceptance, BackendError> {
    verify_scan_identity(scan, expected_identity)?;
    verify_confirmation_token(scan, confirmation_token)?;
    if scan.discarded_at.is_some() {
        return Err(scan_confirmation_error(
            "This saved scan was discarded. Scan the sources again.",
        ));
    }
    if scan.accepted_at.is_some() {
        return Ok(SavedScanAcceptance::AlreadyAccepted);
    }
    if scan.totals != totals_for_files(&scan.files) {
        return Err(scan_confirmation_error(
            "The saved import totals changed. Scan the sources again.",
        ));
    }
    let selection = select_saved_scan_files(scan, acknowledge_aggregate, source_paths)?;
    let discovery = FileDiscoveryService;
    // Every acknowledgement is bound to the complete saved scan, not only to
    // the files admitted by this stage. This keeps aggregate totals and the
    // later per-spreadsheet acknowledgement tied to unchanged source facts.
    for file in &scan.files {
        discovery.revalidate_discovered_file(file)?;
    }
    Ok(SavedScanAcceptance::Ready(SavedScanAcceptancePlan {
        inputs: new_import_inputs(session, selection.files),
        mark_aggregate_confirmed: selection.mark_aggregate_confirmed,
        fully_accepted: selection.fully_accepted,
    }))
}

struct SavedScanSelection {
    files: Vec<DiscoveredFile>,
    mark_aggregate_confirmed: bool,
    fully_accepted: bool,
}

fn select_saved_scan_files(
    scan: &FileScanResult,
    acknowledge_aggregate: bool,
    source_paths: Option<&[String]>,
) -> Result<SavedScanSelection, BackendError> {
    let risky = scan
        .files
        .iter()
        .filter(|file| requires_item_confirmation(file))
        .collect::<Vec<_>>();
    if scan.totals.requires_confirmation && scan.aggregate_confirmed_at.is_none() {
        if !acknowledge_aggregate || source_paths.is_some() {
            return Err(scan_confirmation_error(
                "Confirm the complete aggregate scan before acknowledging individual spreadsheet risks.",
            ));
        }
        return Ok(SavedScanSelection {
            files: scan
                .files
                .iter()
                .filter(|file| !requires_item_confirmation(file))
                .cloned()
                .collect(),
            mark_aggregate_confirmed: true,
            fully_accepted: risky.is_empty(),
        });
    }
    if acknowledge_aggregate {
        if scan.totals.requires_confirmation && scan.aggregate_confirmed_at.is_some() {
            return Ok(SavedScanSelection {
                files: Vec::new(),
                mark_aggregate_confirmed: false,
                fully_accepted: false,
            });
        }
        return Err(scan_confirmation_error(
            "This saved scan does not require aggregate confirmation.",
        ));
    }
    let Some(source_paths) = source_paths else {
        return Err(scan_confirmation_error(
            "Select every pending large spreadsheet before continuing.",
        ));
    };
    let selected = source_paths.iter().cloned().collect::<HashSet<_>>();
    let expected = risky
        .iter()
        .map(|file| file.source_path.clone())
        .collect::<HashSet<_>>();
    if selected.is_empty() || selected != expected {
        return Err(scan_confirmation_error(
            "The spreadsheet acknowledgement must match every pending large-data source in the saved scan.",
        ));
    }
    Ok(SavedScanSelection {
        files: risky.into_iter().cloned().collect(),
        mark_aggregate_confirmed: false,
        fully_accepted: true,
    })
}

pub fn mark_scan_accepted(scan: &mut FileScanResult, accepted_at: String) {
    scan.accepted_at = Some(accepted_at);
}

pub fn mark_scan_aggregate_confirmed(scan: &mut FileScanResult, confirmed_at: String) {
    scan.aggregate_confirmed_at = Some(confirmed_at);
}

pub fn mark_scan_discarded(
    scan: &mut FileScanResult,
    expected_identity: &ImportScanIdentity,
    confirmation_token: &str,
    discarded_at: String,
) -> Result<bool, BackendError> {
    verify_scan_identity(scan, expected_identity)?;
    verify_confirmation_token(scan, confirmation_token)?;
    if scan.accepted_at.is_some() {
        return Err(scan_confirmation_error(
            "This saved scan was already accepted and cannot be discarded.",
        ));
    }
    if scan.discarded_at.is_some() {
        return Ok(false);
    }
    scan.discarded_at = Some(discarded_at);
    Ok(true)
}

fn take_importable_files(
    scan: &mut FileScanResult,
    large_data_confirmed: bool,
) -> Vec<DiscoveredFile> {
    let pending = scan
        .files
        .iter()
        .filter(|file| !large_data_confirmed && requires_item_confirmation(file))
        .cloned()
        .collect::<Vec<_>>();
    let importable = scan
        .files
        .iter()
        .filter(|file| large_data_confirmed || !requires_item_confirmation(file))
        .cloned()
        .collect::<Vec<_>>();
    for file in &pending {
        let estimate = file.large_data.as_ref().expect("partition checked");
        scan.skipped.push(SkippedFile {
            source_path: file.source_path.clone(),
            relative_path: Some(file.relative_path.clone()),
            reason: FileSkipReason::LargeDataConfirmationRequired,
            detail: Some(large_data_confirmation_detail(estimate)),
        });
    }
    importable
}

fn large_data_confirmation_detail(estimate: &LargeDataEstimate) -> String {
    match (estimate.estimate_complete, estimate.sheet_count) {
        (false, _) => format!(
            "workbook output count unavailable, {} bytes",
            estimate.total_bytes
        ),
        (true, Some(sheet_count)) => format!(
            "{} sheets, about {} output files, {} bytes",
            sheet_count, estimate.estimated_output_files, estimate.total_bytes
        ),
        (true, None) => format!(
            "{} rows, about {} output files, {} bytes",
            estimate.row_count, estimate.estimated_output_files, estimate.total_bytes
        ),
    }
}

fn requires_item_confirmation(file: &DiscoveredFile) -> bool {
    file.large_data
        .as_ref()
        .is_some_and(|estimate| estimate.requires_confirmation)
}

fn finalize_scan_totals(scan: &mut FileScanResult) {
    scan.totals = totals_for_files(&scan.files);
}

fn totals_for_files(files: &[DiscoveredFile]) -> ImportScanTotals {
    let file_count = files.len().min(u32::MAX as usize) as u32;
    let total_bytes = files
        .iter()
        .fold(0_u64, |total, file| total.saturating_add(file.size_bytes));
    let estimate_complete = files.iter().all(|file| {
        file.large_data
            .as_ref()
            .is_none_or(|estimate| estimate.estimate_complete)
    });
    let estimated_output_files = estimate_complete.then(|| {
        files.iter().fold(0_u64, |total, file| {
            total.saturating_add(file.large_data.as_ref().map_or(1, |estimate| {
                u64::from(estimate.estimated_output_files.max(1))
            }))
        })
    });
    aggregate_scan_totals(file_count, total_bytes, estimated_output_files)
}

fn aggregate_scan_totals(
    file_count: u32,
    total_bytes: u64,
    estimated_output_files: Option<u64>,
) -> ImportScanTotals {
    let mut reasons = Vec::new();
    if file_count > AGGREGATE_CONFIRM_FILE_COUNT {
        reasons.push(ImportScanConfirmationReason::FileCount);
    }
    if total_bytes > AGGREGATE_CONFIRM_TOTAL_BYTES {
        reasons.push(ImportScanConfirmationReason::TotalBytes);
    }
    if estimated_output_files.is_none_or(|count| count > AGGREGATE_CONFIRM_OUTPUT_FILES) {
        reasons.push(ImportScanConfirmationReason::EstimatedOutputFiles);
    }
    ImportScanTotals {
        file_count,
        total_bytes,
        estimated_output_files,
        requires_confirmation: !reasons.is_empty(),
        reasons,
    }
}

fn verify_confirmation_token(
    scan: &FileScanResult,
    confirmation_token: &str,
) -> Result<(), BackendError> {
    if scan.confirmation_token.as_deref() == Some(confirmation_token) {
        return Ok(());
    }
    Err(scan_confirmation_error(
        "The import scan confirmation is missing or stale. Scan the sources again.",
    ))
}

fn verify_scan_identity(
    scan: &FileScanResult,
    expected: &ImportScanIdentity,
) -> Result<(), BackendError> {
    if scan.scan_identity.as_ref() == Some(expected) {
        return Ok(());
    }
    Err(scan_confirmation_error(
        "The saved import scan belongs to a different project, session, or task.",
    ))
}

fn scan_confirmation_error(message: &str) -> BackendError {
    BackendError::new("IMPORT_SCAN_CONFIRMATION_INVALID", message, true, false)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};
    use std::path::PathBuf;

    use super::*;
    use crate::models::import_v2::{ImportResourceMode, ImportSessionStatus};
    use crate::models::import_v2_file::{FileScanPolicy, FileSkipReason, LargeDataEstimate};
    use crate::models::paths::ProjectContext;

    fn empty_session(project_id: &str) -> ImportSession {
        ImportSession {
            schema_version: 1,
            session_id: "session-1".into(),
            project_id: project_id.into(),
            status: ImportSessionStatus::Draft,
            resource_mode: ImportResourceMode::Balanced,
            created_at: "2026-08-06T00:00:00Z".into(),
            updated_at: "2026-08-06T00:00:00Z".into(),
            discovery_task_id: None,
            media_authorizations: Vec::new(),
            collection_relations: Vec::new(),
            items: Vec::new(),
        }
    }

    fn expected_identity(context: &ProjectContext) -> ImportScanIdentity {
        ImportScanIdentity {
            project_id: context.project_id.clone(),
            project_root_path: context.root.to_string_lossy().into_owned(),
            session_id: "session-1".into(),
            task_id: "task-1".into(),
        }
    }

    fn scan_fixture(name: &str) -> (tempfile::TempDir, ProjectContext, FileScanResult) {
        let root = tempfile::tempdir().unwrap();
        let project_root = root.path().join("project");
        std::fs::create_dir_all(&project_root).unwrap();
        let source = root.path().join(name);
        std::fs::write(&source, "# Original\n").unwrap();
        let context = ProjectContext::new("scan-confirmation", project_root);
        let mut scan = FileDiscoveryService
            .scan(
                &context,
                std::slice::from_ref(&source),
                FileScanPolicy::default(),
                |_| {},
                || false,
            )
            .unwrap();
        scan.scan_identity = Some(expected_identity(&context));
        finalize_scan_totals(&mut scan);
        (root, context, scan)
    }

    #[test]
    fn aggregate_confirmation_thresholds_are_backend_owned_and_strict() {
        let boundary = aggregate_scan_totals(
            AGGREGATE_CONFIRM_FILE_COUNT,
            AGGREGATE_CONFIRM_TOTAL_BYTES,
            Some(AGGREGATE_CONFIRM_OUTPUT_FILES),
        );
        assert!(!boundary.requires_confirmation);
        assert!(boundary.reasons.is_empty());

        let above = aggregate_scan_totals(
            AGGREGATE_CONFIRM_FILE_COUNT + 1,
            AGGREGATE_CONFIRM_TOTAL_BYTES + 1,
            Some(AGGREGATE_CONFIRM_OUTPUT_FILES + 1),
        );
        assert!(above.requires_confirmation);
        assert_eq!(
            above.reasons,
            vec![
                ImportScanConfirmationReason::FileCount,
                ImportScanConfirmationReason::TotalBytes,
                ImportScanConfirmationReason::EstimatedOutputFiles,
            ]
        );
    }

    #[test]
    fn aggregate_confirmation_produces_no_session_inputs_before_acceptance() {
        let (_root, _context, mut scan) = scan_fixture("source.md");
        scan.files = (0..=AGGREGATE_CONFIRM_FILE_COUNT)
            .map(|index| {
                let mut file = scan.files[0].clone();
                file.source_path = format!("{}-{index}", file.source_path);
                file.relative_path = format!("source-{index}.md");
                file
            })
            .collect();
        let session = empty_session("scan-confirmation");

        let plan = prepare_scan_staging(&mut scan, &session, false, || "token-a".into());

        assert!(plan.aggregate_confirmation_pending);
        assert!(plan.inputs.is_empty());
        assert!(session.items.is_empty());
        assert_eq!(scan.confirmation_token.as_deref(), Some("token-a"));

        let mut legacy_flag_scan = scan.clone();
        let legacy_flag_plan =
            prepare_scan_staging(&mut legacy_flag_scan, &session, true, || "token-b".into());
        assert!(legacy_flag_plan.aggregate_confirmation_pending);
        assert!(legacy_flag_plan.inputs.is_empty());

        let mut legacy_scan = scan;
        assert_eq!(
            prepare_legacy_scan_staging(&mut legacy_scan, &session, false).len(),
            (AGGREGATE_CONFIRM_FILE_COUNT + 1) as usize
        );
    }

    #[test]
    fn large_csv_requires_confirmation_before_it_becomes_a_session_input() {
        let root = tempfile::tempdir().unwrap();
        let context = ProjectContext::new("large-csv", root.path().to_path_buf());
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/import-v2/local/batch3/large.csv");
        let scan = FileDiscoveryService
            .scan(
                &context,
                &[fixture],
                FileScanPolicy::default(),
                |_| {},
                || false,
            )
            .unwrap();
        let session = empty_session("large-csv");

        let mut pending = scan.clone();
        let pending_plan = prepare_scan_staging(&mut pending, &session, false, || "token-b".into());
        assert!(pending_plan.inputs.is_empty());
        assert!(pending_plan.item_confirmation_pending);
        assert!(pending
            .skipped
            .iter()
            .any(|entry| entry.reason == FileSkipReason::LargeDataConfirmationRequired));

        let mut confirmed = scan;
        let confirmed_plan =
            prepare_scan_staging(&mut confirmed, &session, true, || unreachable!());
        assert_eq!(confirmed_plan.inputs.len(), 1);
        assert!(!confirmed_plan.inputs[0]
            .source_identity
            .as_ref()
            .unwrap()
            .sha256
            .is_empty());
    }

    #[test]
    fn aggregate_and_spreadsheet_risks_require_separate_acknowledgements() {
        let (_root, _context, mut scan) = scan_fixture("ordinary.md");
        let mut risky = scan.files[0].clone();
        risky.source_path.push_str("-large.csv");
        risky.relative_path = "large.csv".into();
        risky.large_data = Some(LargeDataEstimate {
            row_count: 10_000,
            sheet_count: None,
            estimated_output_files: 3,
            total_bytes: risky.size_bytes,
            requires_confirmation: true,
            estimate_complete: true,
        });
        scan.files.push(risky.clone());
        scan.totals.requires_confirmation = true;

        assert!(select_saved_scan_files(&scan, false, None).is_err());
        assert!(select_saved_scan_files(
            &scan,
            false,
            Some(std::slice::from_ref(&risky.source_path)),
        )
        .is_err());

        let aggregate = select_saved_scan_files(&scan, true, None).unwrap();
        assert!(aggregate.mark_aggregate_confirmed);
        assert!(!aggregate.fully_accepted);
        assert_eq!(aggregate.files.len(), 1);
        assert!(!requires_item_confirmation(&aggregate.files[0]));

        scan.aggregate_confirmed_at = Some("aggregate-confirmed".into());
        let item_risk =
            select_saved_scan_files(&scan, false, Some(std::slice::from_ref(&risky.source_path)))
                .unwrap();
        assert!(!item_risk.mark_aggregate_confirmed);
        assert!(item_risk.fully_accepted);
        assert_eq!(item_risk.files, vec![risky]);
    }

    #[test]
    fn scanner_refuses_the_first_file_beyond_the_hard_limit_without_staging_inputs() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let sources = root.path().join("sources");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&sources).unwrap();
        std::fs::write(sources.join("one.md"), "# One\n").unwrap();
        std::fs::write(sources.join("two.md"), "# Two\n").unwrap();
        let session = empty_session("hard-limit");
        let mut callback_count = 0;

        let error = FileDiscoveryService
            .scan(
                &ProjectContext::new("hard-limit", project),
                &[sources],
                FileScanPolicy {
                    max_files: 1,
                    ..FileScanPolicy::default()
                },
                |_| callback_count += 1,
                || false,
            )
            .unwrap_err();

        assert_eq!(error.code, "IMPORT_FILE_HARD_LIMIT_EXCEEDED");
        assert_eq!(callback_count, 0);
        assert!(session.items.is_empty());
    }

    #[test]
    fn workbook_sheet_estimates_cross_the_output_threshold_and_malformed_xlsx_fails_closed() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        let workbook_path = root.path().join("many-sheets.xlsx");
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("[Content_Types].xml", options).unwrap();
        writer.write_all(b"<Types/>").unwrap();
        writer.start_file("xl/workbook.xml", options).unwrap();
        writer.write_all(b"<workbook/>").unwrap();
        for index in 0..=AGGREGATE_CONFIRM_OUTPUT_FILES {
            writer
                .start_file(format!("xl/worksheets/sheet{index}.xml"), options)
                .unwrap();
            writer
                .write_all(b"<worksheet><sheetData><row/></sheetData></worksheet>")
                .unwrap();
        }
        std::fs::write(&workbook_path, writer.finish().unwrap().into_inner()).unwrap();
        let context = ProjectContext::new("xlsx-estimate", project.clone());
        let mut scan = FileDiscoveryService
            .scan(
                &context,
                &[workbook_path],
                FileScanPolicy::default(),
                |_| {},
                || false,
            )
            .unwrap();

        let plan = prepare_scan_staging(&mut scan, &empty_session("xlsx-estimate"), false, || {
            "xlsx-token".into()
        });
        assert!(plan.aggregate_confirmation_pending);
        assert!(plan.item_confirmation_pending);
        assert!(plan.inputs.is_empty());
        assert_eq!(
            scan.totals.estimated_output_files,
            Some(u64::from(AGGREGATE_CONFIRM_OUTPUT_FILES) + 2)
        );
        assert_eq!(
            scan.files[0]
                .large_data
                .as_ref()
                .and_then(|estimate| estimate.sheet_count),
            Some((AGGREGATE_CONFIRM_OUTPUT_FILES + 1) as u32)
        );

        let malformed = root.path().join("malformed.xlsx");
        std::fs::write(&malformed, [0x50, 0x4b, 0x03, 0x04]).unwrap();
        let malformed_scan = FileDiscoveryService
            .scan(
                &ProjectContext::new("malformed-xlsx", project),
                &[malformed],
                FileScanPolicy::default(),
                |_| {},
                || false,
            )
            .unwrap();
        assert!(malformed_scan.files.is_empty());
        assert_eq!(malformed_scan.skipped.len(), 1);
        assert_eq!(
            malformed_scan.skipped[0].reason,
            FileSkipReason::UnsupportedFormat
        );
        assert!(malformed_scan.skipped[0]
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("valid ZIP archive")));
    }

    #[test]
    fn saved_scan_accepts_only_saved_identity_and_deduplicates_session_inputs() {
        let (root, context, mut scan) = scan_fixture("source.md");
        scan.confirmation_token = Some("token-c".into());
        let source_path = scan.files[0].source_path.clone();
        scan.files[0].large_data = Some(LargeDataEstimate {
            row_count: 10_000,
            sheet_count: None,
            estimated_output_files: 3,
            total_bytes: scan.files[0].size_bytes,
            requires_confirmation: true,
            estimate_complete: true,
        });
        scan.files.push(scan.files[0].clone());
        finalize_scan_totals(&mut scan);
        let session = empty_session("scan-confirmation");
        let identity = expected_identity(&context);
        let selected_paths = [source_path];

        let acceptance = prepare_saved_scan_acceptance(
            &scan,
            &identity,
            "token-c",
            false,
            Some(&selected_paths),
            &session,
        )
        .unwrap();
        let SavedScanAcceptance::Ready(plan) = acceptance else {
            panic!("expected saved scan inputs");
        };
        assert_eq!(plan.inputs.len(), 1);
        assert!(plan.fully_accepted);

        assert_eq!(
            prepare_saved_scan_acceptance(
                &scan,
                &identity,
                "stale",
                false,
                Some(&selected_paths),
                &session,
            )
            .unwrap_err()
            .code,
            "IMPORT_SCAN_CONFIRMATION_INVALID"
        );
        let wrong_identity = ImportScanIdentity {
            task_id: "other-task".into(),
            ..identity.clone()
        };
        assert_eq!(
            prepare_saved_scan_acceptance(
                &scan,
                &wrong_identity,
                "token-c",
                false,
                Some(&selected_paths),
                &session,
            )
            .unwrap_err()
            .code,
            "IMPORT_SCAN_CONFIRMATION_INVALID"
        );
        let mut changed_totals = scan.clone();
        changed_totals.totals.file_count += 1;
        assert_eq!(
            prepare_saved_scan_acceptance(
                &changed_totals,
                &identity,
                "token-c",
                false,
                Some(&selected_paths),
                &session,
            )
            .unwrap_err()
            .code,
            "IMPORT_SCAN_CONFIRMATION_INVALID"
        );
        std::fs::write(root.path().join("source.md"), "# Changed\n").unwrap();
        assert_eq!(
            prepare_saved_scan_acceptance(
                &scan,
                &identity,
                "token-c",
                false,
                Some(&selected_paths),
                &session,
            )
            .unwrap_err()
            .code,
            "IMPORT_SCAN_SOURCE_CHANGED"
        );
    }

    #[test]
    fn accepted_and_discarded_saved_scans_are_terminal_and_idempotent() {
        let (_root, context, mut scan) = scan_fixture("source.md");
        scan.confirmation_token = Some("token-d".into());
        let identity = expected_identity(&context);
        assert!(mark_scan_discarded(&mut scan, &identity, "token-d", "discarded".into()).unwrap());
        assert!(!mark_scan_discarded(&mut scan, &identity, "token-d", "later".into()).unwrap());
        assert_eq!(scan.discarded_at.as_deref(), Some("discarded"));

        scan.discarded_at = None;
        mark_scan_accepted(&mut scan, "accepted".into());
        assert_eq!(
            mark_scan_discarded(&mut scan, &identity, "token-d", "discarded".into())
                .unwrap_err()
                .code,
            "IMPORT_SCAN_CONFIRMATION_INVALID"
        );
        assert_eq!(
            prepare_saved_scan_acceptance(
                &scan,
                &identity,
                "token-d",
                false,
                None,
                &empty_session("scan-confirmation")
            )
            .unwrap(),
            SavedScanAcceptance::AlreadyAccepted
        );
    }
}
