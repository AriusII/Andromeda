use crate::support::manifest;
use andromeda_error::AndromedaErrorKind;
use andromeda_recovery::{
    FileWalRecoveryBoundaryKind, recover_from_file_wal, report_file_wal_recovery_v0,
};
use andromeda_recovery::{
    ObservedBoundary, StartupMode, StartupRejectionReason, plan_file_wal_startup_recovery_v0,
};
use andromeda_wal::Lsn;
use andromeda_wal::{FILE_WAL_HEADER_LEN, FileWal, scan_file_wal};
use andromeda_wal::{WalRecord, WalRecordKind};
use andromeda_wal::{WalScanStopReason, encode_wal_record};
use std::{
    fs::OpenOptions,
    io::{Seek, SeekFrom, Write},
};

#[test]
fn file_wal_lsn_gap_is_forensic_rejection() {
    let temp = crate::support::TempWalPath::new("lsn-gap");
    let first_record = WalRecord::from_parts(
        WalRecordKind::PageAllocate,
        Lsn::new(1),
        None,
        None,
        b"first",
    )
    .unwrap();
    let first_len = encode_wal_record(&first_record).unwrap().len() as u64;

    {
        let mut wal = FileWal::open(&temp.path).unwrap();
        wal.append(first_record).unwrap();
        wal.append_payload(WalRecordKind::PageFormat, None, b"second")
            .unwrap();
        wal.flush_all().unwrap();
    }

    let gap_record = WalRecord::from_parts(
        WalRecordKind::PageFormat,
        Lsn::new(3),
        Some(Lsn::new(1)),
        None,
        b"second",
    )
    .unwrap();
    let mut file = OpenOptions::new().write(true).open(&temp.path).unwrap();
    file.seek(SeekFrom::Start(FILE_WAL_HEADER_LEN as u64 + first_len))
        .unwrap();
    file.write_all(&encode_wal_record(&gap_record).unwrap())
        .unwrap();

    let disk_scan = scan_file_wal(&temp.path).unwrap();
    assert_eq!(
        disk_scan.scan.stopped.unwrap().reason,
        WalScanStopReason::LsnGap
    );

    let report =
        report_file_wal_recovery_v0(&manifest(Lsn::new(1)), StartupMode::SafeStart, &temp.path)
            .unwrap();
    assert_eq!(
        report.boundary_kind,
        FileWalRecoveryBoundaryKind::ForensicChainBreak
    );
    assert!(report.forensic_required);
    assert_eq!(report.durable_lsn, Lsn::new(1));
    assert_eq!(report.replay_lsns().collect::<Vec<_>>(), Vec::<Lsn>::new());

    assert_eq!(
        recover_from_file_wal(
            &manifest(Lsn::new(1)),
            StartupMode::ForensicStart,
            &temp.path
        )
        .unwrap_err()
        .kind(),
        AndromedaErrorKind::Storage
    );
    assert_eq!(
        FileWal::open(&temp.path).unwrap_err().kind(),
        AndromedaErrorKind::Storage
    );
}

#[test]
fn file_wal_previous_lsn_mismatch_is_forensic_report_and_rejection() {
    let temp = crate::support::TempWalPath::new("previous-mismatch");
    let first_record = WalRecord::from_parts(
        WalRecordKind::PageAllocate,
        Lsn::new(1),
        None,
        None,
        b"first",
    )
    .unwrap();
    let first_len = encode_wal_record(&first_record).unwrap().len() as u64;

    {
        let mut wal = FileWal::open(&temp.path).unwrap();
        wal.append(first_record).unwrap();
        wal.append_payload(WalRecordKind::PageFormat, None, b"second")
            .unwrap();
        wal.flush_all().unwrap();
    }

    let mismatch_record = WalRecord::from_parts(
        WalRecordKind::PageFormat,
        Lsn::new(2),
        None,
        None,
        b"second",
    )
    .unwrap();
    let mut file = OpenOptions::new().write(true).open(&temp.path).unwrap();
    file.seek(SeekFrom::Start(FILE_WAL_HEADER_LEN as u64 + first_len))
        .unwrap();
    file.write_all(&encode_wal_record(&mismatch_record).unwrap())
        .unwrap();

    let disk_scan = scan_file_wal(&temp.path).unwrap();
    assert_eq!(
        disk_scan.scan.stopped.unwrap().reason,
        WalScanStopReason::PreviousLsnMismatch
    );

    let report =
        report_file_wal_recovery_v0(&manifest(Lsn::new(1)), StartupMode::SafeStart, &temp.path)
            .unwrap();
    assert_eq!(
        report.boundary_kind,
        FileWalRecoveryBoundaryKind::ForensicChainBreak
    );
    assert!(report.forensic_required);
    assert_eq!(
        report.scan_stop.unwrap().reason,
        WalScanStopReason::PreviousLsnMismatch
    );

    assert_eq!(
        recover_from_file_wal(&manifest(Lsn::new(1)), StartupMode::SafeStart, &temp.path)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Storage
    );
    assert_eq!(
        FileWal::open(&temp.path).unwrap_err().kind(),
        AndromedaErrorKind::Storage
    );

    let safe_start = plan_file_wal_startup_recovery_v0(
        &manifest(Lsn::new(1)),
        StartupMode::SafeStart,
        &temp.path,
        false,
    )
    .unwrap();
    assert_eq!(
        safe_start.evidence.observed_boundary(),
        ObservedBoundary::ForensicChainBreak
    );
    assert_eq!(
        safe_start.decision.rejection(),
        Some(StartupRejectionReason::ForensicHandlingRequired)
    );
    assert!(
        safe_start.redo_plan.is_none(),
        "SafeStart must not produce a replay plan across a durable chain break"
    );

    let forensic_start = plan_file_wal_startup_recovery_v0(
        &manifest(Lsn::new(1)),
        StartupMode::ForensicStart,
        &temp.path,
        true,
    )
    .unwrap();
    assert!(forensic_start.decision.is_accepted());
    assert!(
        !forensic_start.replay_allowed(),
        "ForensicStart is inspect-only and must not replay/mutate durable truth"
    );
    assert!(
        forensic_start.redo_plan.is_none(),
        "ForensicStart must not produce a replay plan for a chain break"
    );
}
