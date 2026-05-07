use crate::support::{TempWalPath, manifest, write_two_committed_transactions};
use andromeda_storage::write_ahead_log::codec::WalScanStopReason;
use andromeda_storage::write_ahead_log::file::{
    FileWalRecoveryBoundaryKind, recover_from_file_wal, report_file_wal_recovery_v0, scan_file_wal,
};
use andromeda_storage::{Lsn, StartupMode};
use std::{
    fs::OpenOptions,
    io::{Seek, SeekFrom, Write},
};

#[test]
fn file_wal_truncated_and_corrupt_prefix_boundaries_keep_recoverable_prefix() {
    for corrupt_tail in [false, true] {
        let temp = TempWalPath::new(if corrupt_tail {
            "corrupt-prefix"
        } else {
            "truncated-prefix"
        });
        let first_row_lsn = write_two_committed_transactions(&temp.path);

        if corrupt_tail {
            let len = std::fs::metadata(&temp.path).unwrap().len();
            let mut file = OpenOptions::new().write(true).open(&temp.path).unwrap();
            file.seek(SeekFrom::Start(len - 1)).unwrap();
            file.write_all(&[0xa5]).unwrap();
        } else {
            let len = std::fs::metadata(&temp.path).unwrap().len();
            OpenOptions::new()
                .write(true)
                .open(&temp.path)
                .unwrap()
                .set_len(len - 1)
                .unwrap();
        }

        let disk_scan = scan_file_wal(&temp.path).unwrap();
        let stop_reason = disk_scan.scan.stopped.unwrap().reason;
        if corrupt_tail {
            assert!(matches!(
                stop_reason,
                WalScanStopReason::CorruptHeader | WalScanStopReason::CorruptRecord
            ));
        } else {
            assert!(matches!(
                stop_reason,
                WalScanStopReason::TruncatedHeader | WalScanStopReason::TruncatedRecord
            ));
        }

        let plan =
            recover_from_file_wal(&manifest(Lsn::new(1)), StartupMode::SafeStart, &temp.path)
                .unwrap();
        assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), vec![first_row_lsn]);
        assert_eq!(plan.wal_scan_stop().unwrap().reason, stop_reason);

        let report =
            report_file_wal_recovery_v0(&manifest(Lsn::new(1)), StartupMode::SafeStart, &temp.path)
                .unwrap();
        assert_eq!(
            report.boundary_kind,
            FileWalRecoveryBoundaryKind::RecoverableTail
        );
        assert!(report.has_recoverable_tail_boundary());
        assert!(!report.forensic_required);
        assert_eq!(report.scan_stop.unwrap().reason, stop_reason);
        assert_eq!(
            report.replay_lsns().collect::<Vec<_>>(),
            vec![first_row_lsn]
        );
    }
}
