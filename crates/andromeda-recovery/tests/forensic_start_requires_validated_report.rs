//! Contract tests confirming that `forensic_start_with_report` rejects any
//! [`ForensicStartReport`] whose `all_sections_validated()` returns `false`.
//!
//! The function is a sibling of `forensic_start_from_manifest_and_scan` and
//! adds the strict requirement that a fully validated report MUST accompany the
//! startup request.

use andromeda_manifest::DatabaseManifest;
use andromeda_recovery::{
    ForensicCatalogSection, ForensicIndexSection, ForensicMapRefreshState, ForensicMapsSection,
    ForensicSecurityAuditSection, ForensicSnapshotSection, ForensicStartReport,
    ForensicValidationState, ForensicWalSection, forensic_start_with_report,
};
use andromeda_wal::{Lsn, WalScanResult};

// ── Fixtures ──────────────────────────────────────────────────────────────────

fn clean_manifest() -> DatabaseManifest {
    DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: Lsn::ZERO,
        required_wal_start_lsn: Lsn::new(10),
        previous_manifest_hash: [0; 32],
        manifest_crc: 0xdead_beef,
        segment_index_file_id: 0,
        btree_root_page_id: 0,
    }
}

fn clean_scan() -> WalScanResult {
    WalScanResult {
        records: vec![],
        valid_bytes: 0,
        last_valid_lsn: Some(Lsn::new(20)),
        stopped: None,
    }
}

fn valid_report() -> ForensicStartReport {
    ForensicStartReport {
        catalog: ForensicCatalogSection {
            catalog_version: 1,
            object_count: 10,
            validation_state: ForensicValidationState::Validated,
        },
        wal: ForensicWalSection {
            coverage_start_lsn: 10,
            coverage_end_lsn: 20,
            anomaly: None,
            scan_complete: true,
        },
        snapshot: ForensicSnapshotSection {
            snapshot_id: 1,
            snapshot_hash: [0xAA; 32],
            manifest_validated: true,
        },
        indexes: ForensicIndexSection {
            segment_index_count: 1,
            segment_index_validated: true,
            last_segment_id: 1,
        },
        maps: ForensicMapsSection {
            map_count: 1,
            refresh_plan_state: ForensicMapRefreshState::Idle,
            staleness_max_lsn_lag: 0,
        },
        security_audit: ForensicSecurityAuditSection {
            last_audit_lsn: 20,
            retention_boundary_satisfied: true,
            audit_record_count: 100,
        },
        generated_at_epoch: 1_700_000_000,
    }
}

// ── Acceptance test ───────────────────────────────────────────────────────────

#[test]
fn forensic_start_with_valid_report_is_accepted() {
    let report = valid_report();
    assert!(
        report.all_sections_validated(),
        "fixture report must pass all_sections_validated"
    );

    let acceptance = forensic_start_with_report(&clean_manifest(), &clean_scan(), report.clone())
        .expect("valid report must be accepted by forensic_start_with_report");

    assert!(
        !acceptance.replay_allowed(),
        "ForensicStart must never allow replay"
    );
    assert!(
        acceptance.report.is_some(),
        "acceptance must carry the attached report"
    );
    assert_eq!(
        acceptance.report.as_ref().unwrap().generated_at_epoch,
        report.generated_at_epoch
    );
}

// ── Rejection tests — one invalid section per test ───────────────────────────

#[test]
fn forensic_start_rejects_report_with_incomplete_wal_scan() {
    let mut report = valid_report();
    report.wal.scan_complete = false;
    assert!(!report.all_sections_validated());

    let err = forensic_start_with_report(&clean_manifest(), &clean_scan(), report)
        .expect_err("report with scan_complete=false must be rejected");
    assert!(
        err.message().contains("all_sections_validated()"),
        "error must reference all_sections_validated: {:?}",
        err.message()
    );
}

#[test]
fn forensic_start_rejects_report_with_failed_catalog() {
    let mut report = valid_report();
    report.catalog.validation_state = ForensicValidationState::Failed("catalog failure".into());
    assert!(!report.all_sections_validated());

    let err = forensic_start_with_report(&clean_manifest(), &clean_scan(), report)
        .expect_err("report with Failed catalog must be rejected");
    assert!(err.message().contains("all_sections_validated()"));
}

#[test]
fn forensic_start_rejects_report_with_unvalidated_snapshot_manifest() {
    let mut report = valid_report();
    report.snapshot.manifest_validated = false;
    assert!(!report.all_sections_validated());

    let err = forensic_start_with_report(&clean_manifest(), &clean_scan(), report)
        .expect_err("report with manifest_validated=false must be rejected");
    assert!(err.message().contains("all_sections_validated()"));
}

#[test]
fn forensic_start_rejects_report_with_unvalidated_segment_index() {
    let mut report = valid_report();
    report.indexes.segment_index_validated = false;
    assert!(!report.all_sections_validated());

    let err = forensic_start_with_report(&clean_manifest(), &clean_scan(), report)
        .expect_err("report with segment_index_validated=false must be rejected");
    assert!(err.message().contains("all_sections_validated()"));
}

#[test]
fn forensic_start_rejects_report_with_stale_maps() {
    let mut report = valid_report();
    report.maps.refresh_plan_state = ForensicMapRefreshState::Stale;
    assert!(!report.all_sections_validated());

    let err = forensic_start_with_report(&clean_manifest(), &clean_scan(), report)
        .expect_err("report with Stale maps must be rejected");
    assert!(err.message().contains("all_sections_validated()"));
}

#[test]
fn forensic_start_rejects_report_with_unsatisfied_retention_boundary() {
    let mut report = valid_report();
    report.security_audit.retention_boundary_satisfied = false;
    assert!(!report.all_sections_validated());

    let err = forensic_start_with_report(&clean_manifest(), &clean_scan(), report)
        .expect_err("report with retention_boundary_satisfied=false must be rejected");
    assert!(err.message().contains("all_sections_validated()"));
}

#[test]
fn forensic_start_rejects_report_bound_to_different_snapshot() {
    let mut report = valid_report();
    report.snapshot.snapshot_id = 2;
    assert!(report.all_sections_validated());

    let err = forensic_start_with_report(&clean_manifest(), &clean_scan(), report)
        .expect_err("report for a different snapshot must be rejected");
    assert!(err.message().contains("snapshot id must match"));
}

#[test]
fn forensic_start_rejects_report_bound_to_stale_wal_coverage() {
    let mut report = valid_report();
    report.wal.coverage_end_lsn = 19;
    assert!(report.all_sections_validated());

    let err = forensic_start_with_report(&clean_manifest(), &clean_scan(), report)
        .expect_err("report with stale WAL coverage must be rejected");
    assert!(err.message().contains("WAL coverage end"));
}
