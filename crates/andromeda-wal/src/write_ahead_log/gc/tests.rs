use super::*;
use crate::Lsn;
use std::time::Duration;

#[test]
fn wal_gc_candidate_creation_validates() {
    let candidate = WalGcCandidate::new(1, Lsn::new(100), Lsn::new(200), 4096);
    assert!(candidate.is_ok());

    let err = WalGcCandidate::new(0, Lsn::new(100), Lsn::new(200), 4096);
    assert!(err.is_err());

    let err = WalGcCandidate::new(1, Lsn::ZERO, Lsn::new(200), 4096);
    assert!(err.is_err());

    let err = WalGcCandidate::new(1, Lsn::new(100), Lsn::ZERO, 4096);
    assert!(err.is_err());

    let err = WalGcCandidate::new(1, Lsn::new(200), Lsn::new(100), 4096);
    assert!(err.is_err());

    let err = WalGcCandidate::new(1, Lsn::new(100), Lsn::new(200), 0);
    assert!(err.is_err());
}

#[test]
fn wal_gc_candidate_eligibility_checks_lsn_thresholds() {
    let candidate =
        WalGcCandidate::new(1, Lsn::new(100), Lsn::new(200), 4096).expect("valid candidate");

    let eligible = candidate.is_eligible(Lsn::new(300), Lsn::new(50));
    assert!(eligible);

    let not_eligible = candidate.is_eligible(Lsn::new(200), Lsn::new(50));
    assert!(!not_eligible);

    let not_eligible = candidate.is_eligible(Lsn::new(300), Lsn::new(100));
    assert!(!not_eligible);
}

#[test]
fn archive_status_display_formats_correctly() {
    assert_eq!(ArchiveStatus::Archived.to_string(), "archived");
    assert_eq!(ArchiveStatus::Pending.to_string(), "pending");
    assert_eq!(ArchiveStatus::Unknown.to_string(), "unknown");
}

#[test]
fn wal_gc_audit_event_names_are_unique() {
    let events = [
        WalGcAuditEvent::CandidateIdentified {
            segment_id: 1,
            creation_lsn: Lsn::new(100),
            sealing_lsn: Lsn::new(200),
            size_bytes: 4096,
        },
        WalGcAuditEvent::ArchiveVerifyRequested {
            segment_id: 1,
            archive_status: ArchiveStatus::Archived,
        },
        WalGcAuditEvent::SegmentRemoved {
            segment_id: 1,
            bytes_freed: 4096,
        },
        WalGcAuditEvent::Summary {
            run_id: 1,
            candidates_identified: 1,
            candidates_archived: 1,
            candidates_blocked: 0,
            bytes_freed: 4096,
            segments_removed: 1,
        },
    ];

    let names: Vec<_> = events.iter().map(|e| e.as_str()).collect();
    assert_eq!(names.len(), 4);
    assert_eq!(
        names,
        vec![
            "GcCandidateIdentified",
            "GcArchiveVerifyRequested",
            "GcSegmentRemoved",
            "GcSummary"
        ]
    );
}

#[test]
fn wal_gc_summary_tracks_metrics() {
    let mut summary = WalGcSummary::new(1);
    assert_eq!(summary.run_id, 1);
    assert_eq!(summary.candidates_identified, 0);
    assert!(!summary.any_removed());

    summary.segments_removed = 1;
    summary.bytes_freed = 4096;
    assert!(summary.any_removed());
}

#[test]
fn wal_gc_scheduler_config_validates() {
    let config = WalGcSchedulerConfig::new(Duration::from_secs(60), 10);
    assert!(config.validate().is_ok());

    let config = WalGcSchedulerConfig {
        interval: Duration::ZERO,
        target_free_gib: 10,
        max_segments_per_run: 0,
    };
    assert!(config.validate().is_err());

    let config = WalGcSchedulerConfig {
        interval: Duration::from_secs(60),
        target_free_gib: 0,
        max_segments_per_run: 0,
    };
    assert!(config.validate().is_err());
}

#[test]
fn wal_gc_scheduler_config_with_max_segments() {
    let config = WalGcSchedulerConfig::new(Duration::from_secs(60), 10).with_max_segments(100);
    assert_eq!(config.max_segments_per_run, 100);
}
