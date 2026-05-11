//! Tests confirming that the existing [`ForensicAnomalyReport`] can be wrapped
//! inside a [`ForensicWalSection`] and carried through a [`ForensicStartReport`].
//!
//! This validates the reuse relationship between the WAL anomaly classification
//! already present in `forensic_start.rs` and the new report structure.

use andromeda_recovery::{
    ForensicAnomaly, ForensicAnomalyKind, ForensicAnomalyReport, ForensicCatalogSection,
    ForensicIndexSection, ForensicMapRefreshState, ForensicMapsSection,
    ForensicSecurityAuditSection, ForensicSnapshotSection, ForensicStartReport,
    ForensicValidationState, ForensicWalSection,
};
use andromeda_wal::{WalScanStop, WalScanStopReason};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn chain_break_anomaly_report(offset: u64) -> ForensicAnomalyReport {
    ForensicAnomalyReport {
        anomalies: vec![ForensicAnomaly {
            kind: ForensicAnomalyKind::LsnChainBreak { offset },
            detail: format!("WAL LSN chain break detected at offset {offset}: LsnGap"),
        }],
        scan_stop: Some(WalScanStop {
            offset: offset as usize,
            reason: WalScanStopReason::LsnGap,
        }),
    }
}

fn wal_section_with_chain_break(offset: u64) -> ForensicWalSection {
    ForensicWalSection {
        coverage_start_lsn: 1,
        coverage_end_lsn: offset,
        anomaly: Some(chain_break_anomaly_report(offset)),
        scan_complete: false,
    }
}

fn minimal_valid_report_with_wal_section(wal: ForensicWalSection) -> ForensicStartReport {
    ForensicStartReport {
        catalog: ForensicCatalogSection {
            catalog_version: 1,
            object_count: 0,
            validation_state: ForensicValidationState::Validated,
        },
        wal,
        snapshot: ForensicSnapshotSection {
            snapshot_id: 42,
            snapshot_hash: [0xAA; 32],
            manifest_validated: true,
        },
        indexes: ForensicIndexSection {
            segment_index_count: 0,
            segment_index_validated: true,
            last_segment_id: 0,
        },
        maps: ForensicMapsSection {
            map_count: 0,
            refresh_plan_state: ForensicMapRefreshState::Idle,
            staleness_max_lsn_lag: 0,
        },
        security_audit: ForensicSecurityAuditSection {
            last_audit_lsn: 0,
            retention_boundary_satisfied: true,
            audit_record_count: 0,
        },
        generated_at_epoch: 1_700_000_000,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn wal_section_carries_chain_break_anomaly_report() {
    let offset = 42u64;
    let wal = wal_section_with_chain_break(offset);

    let anomaly_report = wal.anomaly.as_ref().expect("anomaly must be present");
    assert!(
        anomaly_report.has_chain_break(),
        "ForensicAnomalyReport must detect the chain break"
    );
    assert_eq!(anomaly_report.anomalies.len(), 1);
    assert_eq!(
        anomaly_report.anomalies[0].kind,
        ForensicAnomalyKind::LsnChainBreak { offset }
    );
    assert_eq!(
        anomaly_report.scan_stop.unwrap().reason,
        WalScanStopReason::LsnGap
    );
}

#[test]
fn forensic_start_report_aggregates_chain_break_anomaly_in_wal_section() {
    let offset = 99u64;
    let wal = wal_section_with_chain_break(offset);
    let report = minimal_valid_report_with_wal_section(wal);

    assert!(
        report.all_sections_validated(),
        "report with a chain break and bound scan stop must still pass all_sections_validated"
    );

    let anomaly = report
        .wal
        .anomaly
        .as_ref()
        .expect("wal section must carry anomaly");
    assert!(anomaly.has_chain_break());
    assert!(!anomaly.is_clean());
}

#[test]
fn clean_wal_section_has_no_anomaly() {
    let wal = ForensicWalSection {
        coverage_start_lsn: 1,
        coverage_end_lsn: 100,
        anomaly: None,
        scan_complete: true,
    };
    let report = minimal_valid_report_with_wal_section(wal);

    assert!(
        report.wal.anomaly.is_none(),
        "clean scan must have no anomaly"
    );
    assert!(report.all_sections_validated());
}

#[test]
fn chain_break_anomaly_report_is_not_clean() {
    let report = chain_break_anomaly_report(77);
    assert!(!report.is_clean(), "chain-break report must not be clean");
    assert!(report.has_chain_break());
    assert!(!report.has_recoverable_tail());
}

#[test]
fn recoverable_tail_anomaly_does_not_register_as_chain_break() {
    let tail_anomaly = ForensicAnomalyReport {
        anomalies: vec![ForensicAnomaly {
            kind: ForensicAnomalyKind::RecoverableTailTruncation { offset: 50 },
            detail: "WAL tail truncated at offset 50: TruncatedRecord".into(),
        }],
        scan_stop: Some(WalScanStop {
            offset: 50,
            reason: WalScanStopReason::TruncatedRecord,
        }),
    };
    assert!(
        !tail_anomaly.has_chain_break(),
        "recoverable tail must not be classified as chain break"
    );
    assert!(tail_anomaly.has_recoverable_tail());
    assert!(!tail_anomaly.is_clean());
}
