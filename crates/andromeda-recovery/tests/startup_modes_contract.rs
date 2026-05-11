use andromeda_manifest::DatabaseManifest;
use andromeda_recovery::{
    ForensicAnomalyKind, ObservedBoundary, fast_start_from_manifest_and_scan,
    forensic_start_from_manifest_and_scan, safe_start_from_manifest_and_scan,
    verify_safe_start_invariants,
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

    assert!(
        forensic_start_from_manifest_and_scan(&clean_manifest(), &scan, false)
            .unwrap_err()
            .message()
            .contains("ForensicStart rejected")
    );

    let proof = forensic_start_from_manifest_and_scan(&clean_manifest(), &scan, true).unwrap();
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
