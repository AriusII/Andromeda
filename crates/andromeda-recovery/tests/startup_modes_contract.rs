use andromeda_manifest::DatabaseManifest;
use andromeda_recovery::{
    ApplicationSurfaceDisposition, ForensicAnomaly, ForensicAnomalyKind, ForensicAnomalyReport,
    ForensicCatalogSection, ForensicIndexSection, ForensicMapRefreshState, ForensicMapsSection,
    ForensicSecurityAuditSection, ForensicSnapshotSection, ForensicStartReport,
    ForensicValidationState, ForensicWalSection, ObservedBoundary, StartupMode,
    block_application_surface, fast_start_from_manifest_and_scan, forensic_start_with_report,
    safe_start_from_manifest_and_scan, verify_safe_start_invariants,
};
use andromeda_wal::{Lsn, WalScanResult, WalScanStop, WalScanStopReason};

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

fn valid_forensic_report() -> ForensicStartReport {
    ForensicStartReport {
        catalog: ForensicCatalogSection {
            catalog_version: 1,
            object_count: 1,
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
            audit_record_count: 1,
        },
        generated_at_epoch: 1_700_000_000,
    }
}

fn valid_forensic_report_for_stop(stop: WalScanStop) -> ForensicStartReport {
    let mut report = valid_forensic_report();
    report.wal.anomaly = Some(ForensicAnomalyReport {
        anomalies: vec![ForensicAnomaly {
            kind: ForensicAnomalyKind::LsnChainBreak {
                offset: stop.offset as u64,
            },
            detail: "test WAL chain break".to_string(),
        }],
        scan_stop: Some(stop),
    });
    report.wal.scan_complete = false;
    report
}

#[test]
fn fast_start_accepts_only_clean_wal_boundary() {
    let proof = fast_start_from_manifest_and_scan(&clean_manifest(), &clean_scan()).unwrap();
    assert!(proof.inner.replay_allowed);
    assert_eq!(proof.observed_boundary, ObservedBoundary::Clean);

    let scan = WalScanResult {
        stopped: Some(WalScanStop {
            reason: WalScanStopReason::TruncatedRecord,
            offset: 100,
        }),
        ..clean_scan()
    };
    let err = fast_start_from_manifest_and_scan(&clean_manifest(), &scan).unwrap_err();
    assert!(
        err.message()
            .contains("fast_start_recoverable_tail_not_allowed")
    );
}

#[test]
fn safe_start_accepts_recoverable_tail_and_reports_discard_boundary() {
    let scan = WalScanResult {
        stopped: Some(WalScanStop {
            reason: WalScanStopReason::TruncatedRecord,
            offset: 200,
        }),
        ..clean_scan()
    };

    let proof = safe_start_from_manifest_and_scan(&clean_manifest(), &scan).unwrap();
    assert!(proof.inner.replay_allowed);
    assert_eq!(proof.observed_boundary, ObservedBoundary::RecoverableTail);
    assert_eq!(proof.tail_discard.unwrap().discard_after_lsn, Lsn::new(20));
    assert!(
        verify_safe_start_invariants(&clean_manifest(), &scan)
            .unwrap()
            .all_pass()
    );
}

#[test]
fn forensic_start_requires_report_and_classifies_chain_breaks() {
    let scan = WalScanResult {
        stopped: Some(WalScanStop {
            reason: WalScanStopReason::LsnGap,
            offset: 42,
        }),
        ..clean_scan()
    };
    let stop = scan.stopped.expect("fixture carries a WAL scan stop");

    let proof = forensic_start_with_report(
        &clean_manifest(),
        &scan,
        valid_forensic_report_for_stop(stop),
    )
    .expect("typed forensic report path must accept a validated report");
    assert!(!proof.replay_allowed());
    assert!(proof.has_chain_break());
    assert_eq!(
        proof.anomaly_report.anomalies[0].kind,
        ForensicAnomalyKind::LsnChainBreak { offset: 42 }
    );
    assert_eq!(
        proof.inner.observed_boundary,
        ObservedBoundary::ForensicChainBreak
    );
    assert!(!proof.inner.replay_allowed);
}

#[test]
fn forensic_start_with_valid_report_always_blocks_application_surface() {
    let proof =
        forensic_start_with_report(&clean_manifest(), &clean_scan(), valid_forensic_report())
            .expect("validated forensic report must be accepted");

    assert_eq!(
        proof.application_surface_disposition(),
        ApplicationSurfaceDisposition::BlockForensic,
        "ForensicStart acceptance must emit BlockForensic disposition"
    );
    assert_ne!(
        proof.application_surface_disposition(),
        ApplicationSurfaceDisposition::Allow,
        "ForensicStart must never produce Allow application disposition"
    );
    assert!(
        block_application_surface(StartupMode::ForensicStart),
        "startup-mode gate must remain blocked for ForensicStart"
    );
}

#[test]
fn forensic_start_rejects_report_claiming_complete_scan_when_wal_stopped() {
    let scan = WalScanResult {
        stopped: Some(WalScanStop {
            reason: WalScanStopReason::LsnGap,
            offset: 42,
        }),
        ..clean_scan()
    };
    let stop = scan.stopped.expect("fixture carries a WAL scan stop");
    let mut report = valid_forensic_report_for_stop(stop);
    report.wal.scan_complete = true;

    let err = forensic_start_with_report(&clean_manifest(), &scan, report)
        .expect_err("stopped WAL scan cannot be reported as complete");
    assert!(
        err.message().contains("scan_complete"),
        "unexpected error: {err}"
    );
}
