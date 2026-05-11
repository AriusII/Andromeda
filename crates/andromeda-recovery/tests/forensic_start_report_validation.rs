//! Negative validation tests for [`ForensicStartReport`] section validation.
//!
//! Six negative tests — one per section — confirm that each section's
//! `validate()` call is fail-closed, and that a single failing section causes
//! `ForensicStartReport::all_sections_validated()` to return `false`.
//!
//! P13 exit criterion line 38: ForensicStartReport v0 must gate on every
//! section being valid before the report can be accepted.

use andromeda_recovery::{
    ForensicCatalogSection, ForensicIndexSection, ForensicMapRefreshState, ForensicMapsSection,
    ForensicSecurityAuditSection, ForensicSnapshotSection, ForensicStartReport,
    ForensicValidationState, ForensicWalSection,
};

// ── Fixtures ──────────────────────────────────────────────────────────────────

fn valid_catalog() -> ForensicCatalogSection {
    ForensicCatalogSection {
        catalog_version: 7,
        object_count: 42,
        validation_state: ForensicValidationState::Validated,
    }
}

fn valid_wal() -> ForensicWalSection {
    ForensicWalSection {
        coverage_start_lsn: 10,
        coverage_end_lsn: 200,
        anomaly: None,
        scan_complete: true,
    }
}

fn valid_snapshot() -> ForensicSnapshotSection {
    ForensicSnapshotSection {
        snapshot_id: 99,
        snapshot_hash: [0xAB; 32],
        manifest_validated: true,
    }
}

fn valid_indexes() -> ForensicIndexSection {
    ForensicIndexSection {
        segment_index_count: 5,
        segment_index_validated: true,
        last_segment_id: 5,
    }
}

fn valid_maps() -> ForensicMapsSection {
    ForensicMapsSection {
        map_count: 3,
        refresh_plan_state: ForensicMapRefreshState::Idle,
        staleness_max_lsn_lag: 0,
    }
}

fn valid_security_audit() -> ForensicSecurityAuditSection {
    ForensicSecurityAuditSection {
        last_audit_lsn: 200,
        retention_boundary_satisfied: true,
        audit_record_count: 1_024,
    }
}

fn valid_report() -> ForensicStartReport {
    ForensicStartReport {
        catalog: valid_catalog(),
        wal: valid_wal(),
        snapshot: valid_snapshot(),
        indexes: valid_indexes(),
        maps: valid_maps(),
        security_audit: valid_security_audit(),
        generated_at_epoch: 1_700_000_000,
    }
}

// ── Positive baseline ──────────────────────────────────────────────────────────

#[test]
fn fully_valid_report_passes_all_sections() {
    let report = valid_report();
    assert!(
        report.all_sections_validated(),
        "a fully valid report must pass all_sections_validated"
    );
    assert!(report.catalog.validate().is_ok());
    assert!(report.wal.validate().is_ok());
    assert!(report.snapshot.validate().is_ok());
    assert!(report.indexes.validate().is_ok());
    assert!(report.maps.validate().is_ok());
    assert!(report.security_audit.validate().is_ok());
}

// ── Negative tests — one per section ─────────────────────────────────────────

#[test]
fn catalog_section_validation_failed_state_rejects_report() {
    let mut report = valid_report();
    report.catalog.validation_state =
        ForensicValidationState::Failed("test catalog inconsistency".into());

    assert!(
        report.catalog.validate().is_err(),
        "catalog with Failed state must fail validation"
    );
    assert!(
        !report.all_sections_validated(),
        "report with failed catalog must not pass all_sections_validated"
    );
    let err = report.catalog.validate().unwrap_err();
    assert!(
        err.message().contains("catalog section validation failed"),
        "error message must reference catalog failure: {:?}",
        err.message()
    );
}

#[test]
fn catalog_section_pending_state_rejects_report() {
    let mut report = valid_report();
    report.catalog.validation_state = ForensicValidationState::Pending;

    assert!(
        !report.all_sections_validated(),
        "report with Pending catalog must not pass all_sections_validated"
    );
    let err = report.catalog.validate().unwrap_err();
    assert!(
        err.message().contains("Pending"),
        "error message must reference Pending state: {:?}",
        err.message()
    );
}

#[test]
fn wal_section_scan_incomplete_rejects_report() {
    let mut report = valid_report();
    report.wal.scan_complete = false;

    assert!(
        report.wal.validate().is_err(),
        "WAL section with scan_complete=false must fail validation"
    );
    assert!(
        !report.all_sections_validated(),
        "report with incomplete WAL scan must not pass all_sections_validated"
    );
    let err = report.wal.validate().unwrap_err();
    assert!(
        err.message().contains("scan_complete is false"),
        "error message must reference scan_complete: {:?}",
        err.message()
    );
}

#[test]
fn snapshot_section_unvalidated_manifest_rejects_report() {
    let mut report = valid_report();
    report.snapshot.manifest_validated = false;

    assert!(
        report.snapshot.validate().is_err(),
        "snapshot section with manifest_validated=false must fail validation"
    );
    assert!(
        !report.all_sections_validated(),
        "report with unvalidated manifest must not pass all_sections_validated"
    );
    let err = report.snapshot.validate().unwrap_err();
    assert!(
        err.message().contains("manifest_validated is false"),
        "error message must reference manifest_validated: {:?}",
        err.message()
    );
}

#[test]
fn index_section_unvalidated_segment_index_rejects_report() {
    let mut report = valid_report();
    report.indexes.segment_index_validated = false;

    assert!(
        report.indexes.validate().is_err(),
        "index section with segment_index_validated=false must fail validation"
    );
    assert!(
        !report.all_sections_validated(),
        "report with unvalidated segment index must not pass all_sections_validated"
    );
    let err = report.indexes.validate().unwrap_err();
    assert!(
        err.message().contains("segment_index_validated is false"),
        "error message must reference segment_index_validated: {:?}",
        err.message()
    );
}

#[test]
fn maps_section_stale_refresh_state_rejects_report() {
    let mut report = valid_report();
    report.maps.refresh_plan_state = ForensicMapRefreshState::Stale;

    assert!(
        report.maps.validate().is_err(),
        "maps section with Stale refresh state must fail validation"
    );
    assert!(
        !report.all_sections_validated(),
        "report with Stale maps must not pass all_sections_validated"
    );
    let err = report.maps.validate().unwrap_err();
    assert!(
        err.message().contains("refresh_plan_state is Stale"),
        "error message must reference Stale: {:?}",
        err.message()
    );
}

#[test]
fn security_audit_section_unsatisfied_retention_rejects_report() {
    let mut report = valid_report();
    report.security_audit.retention_boundary_satisfied = false;

    assert!(
        report.security_audit.validate().is_err(),
        "security audit section with retention_boundary_satisfied=false must fail validation"
    );
    assert!(
        !report.all_sections_validated(),
        "report with unsatisfied retention boundary must not pass all_sections_validated"
    );
    let err = report.security_audit.validate().unwrap_err();
    assert!(
        err.message()
            .contains("retention_boundary_satisfied is false"),
        "error message must reference retention_boundary_satisfied: {:?}",
        err.message()
    );
}
