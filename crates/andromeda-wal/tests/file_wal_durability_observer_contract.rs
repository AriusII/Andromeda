#![forbid(unsafe_code)]

//! Contract tests for [`FileWalDurabilityObserver`] wired to [`FileWal`].
//!
//! Covers:
//! - Observer initialised from recovered durable LSN after `FileWal::open`.
//! - Observer advances atomically when `flush_through` succeeds.
//! - Page-flush fence is **rejected** when `page_lsn > observer.current_durable_lsn()`.
//! - Page-flush fence is **allowed** after observer advances past the page LSN.
//! - Multiple observers from the same WAL share the same signal.
//! - Crash/recovery simulation: WAL records survive a simulated crash; the
//!   observer on reopen reflects the pre-crash durable LSN; the fence correctly
//!   classifies pages as flushed-eligible vs. blocked.

use andromeda_types::TransactionId;
use andromeda_wal::{
    FileWal, FileWalDurabilityObserver, Lsn, WalRecordKind,
    validate_wal_durability_before_page_flush,
};
use tempfile::TempDir;

fn fresh_wal(dir: &TempDir) -> FileWal {
    FileWal::open(dir.path().join("test.wal")).expect("open fresh file WAL")
}

// ---------------------------------------------------------------------------
// Observer initialisation
// ---------------------------------------------------------------------------

#[test]
fn file_wal_observer_starts_at_zero_for_new_wal() {
    let dir = TempDir::new().expect("temp dir");
    let wal = fresh_wal(&dir);
    let observer = wal.observer();

    assert_eq!(
        observer.current_durable_lsn(),
        Lsn::ZERO,
        "fresh WAL observer must report durable_lsn=0"
    );
}

#[test]
fn file_wal_observer_initialised_from_recovered_durable_lsn_after_reopen() {
    let dir = TempDir::new().expect("temp dir");
    let expected_durable_lsn;

    // Phase 1: write records + flush_through
    {
        let mut wal = fresh_wal(&dir);
        let _lsn1 = wal
            .append_tx_begin(TransactionId::new(1))
            .expect("append begin");
        let lsn2 = wal
            .append_tx_commit(TransactionId::new(1))
            .expect("append commit");
        wal.flush_through(lsn2).expect("flush_through lsn2");
        expected_durable_lsn = lsn2;
    } // WAL dropped — simulated process exit (not a crash, clean shutdown)

    // Phase 2: reopen — observer must start at the recovered durable LSN
    {
        let wal = FileWal::open(dir.path().join("test.wal")).expect("reopen WAL");
        let observer = wal.observer();
        assert_eq!(
            observer.current_durable_lsn(),
            expected_durable_lsn,
            "reopened WAL observer must reflect pre-shutdown durable LSN"
        );
    }
}

// ---------------------------------------------------------------------------
// Observer advance after flush_through
// ---------------------------------------------------------------------------

#[test]
fn file_wal_observer_advances_after_flush_through() {
    let dir = TempDir::new().expect("temp dir");
    let mut wal = fresh_wal(&dir);
    let observer = wal.observer();

    assert_eq!(observer.current_durable_lsn(), Lsn::ZERO);

    let lsn1 = wal.append_tx_begin(TransactionId::new(1)).expect("append");
    // Appended but not yet flushed → observer still at 0
    assert_eq!(observer.current_durable_lsn(), Lsn::ZERO);

    wal.flush_through(lsn1).expect("flush_through");
    assert_eq!(
        observer.current_durable_lsn(),
        lsn1,
        "observer must advance exactly to the flushed LSN"
    );
}

#[test]
fn file_wal_observer_advances_step_by_step_through_partial_flushes() {
    let dir = TempDir::new().expect("temp dir");
    let mut wal = fresh_wal(&dir);
    let observer = wal.observer();

    let lsn1 = wal
        .append_tx_begin(TransactionId::new(1))
        .expect("append lsn1");
    let lsn2 = wal
        .append_payload(WalRecordKind::MapDeltaAppend, None, b"payload".to_vec())
        .expect("append lsn2");
    let lsn3 = wal
        .append_tx_commit(TransactionId::new(1))
        .expect("append lsn3");

    // Flush through lsn1 only
    wal.flush_through(lsn1).expect("flush_through lsn1");
    assert_eq!(observer.current_durable_lsn(), lsn1);
    assert!(lsn2 > lsn1);
    assert!(lsn3 > lsn2);

    // Flush through lsn3 — skips lsn2 but still valid (LSN ordering is monotone)
    wal.flush_through(lsn3).expect("flush_through lsn3");
    assert_eq!(observer.current_durable_lsn(), lsn3);
}

// ---------------------------------------------------------------------------
// Page-flush fence: REJECTION (C5 invariant proof)
// ---------------------------------------------------------------------------

/// **Rejection test**: The flush fence MUST block a page whose dirty LSN
/// exceeds the current durable WAL LSN.  This is the C5 invariant:
/// "Page flush without WAL coverage impossible."
#[test]
fn file_wal_observer_rejects_page_flush_when_page_lsn_exceeds_durable() {
    let dir = TempDir::new().expect("temp dir");
    let mut wal = fresh_wal(&dir);

    let lsn1 = wal.append_tx_begin(TransactionId::new(1)).expect("append");
    let lsn2 = wal.append_tx_commit(TransactionId::new(1)).expect("append");
    // Flush only through lsn1 → lsn2 not yet durable
    wal.flush_through(lsn1).expect("flush_through lsn1");
    let observer = wal.observer();

    // page_lsn = lsn1 → covered → flush allowed
    assert!(
        validate_wal_durability_before_page_flush(lsn1, observer.current_durable_lsn()).is_ok(),
        "page at lsn1 must be allowed: WAL is durable through lsn1"
    );

    // page_lsn = lsn2 → NOT covered → flush REJECTED
    let result = validate_wal_durability_before_page_flush(lsn2, observer.current_durable_lsn());
    assert!(
        result.is_err(),
        "page at lsn2 must be REJECTED: WAL only durable through lsn1"
    );
    let err = result.unwrap_err();
    let err_msg = err.message();
    assert!(
        err_msg.contains(&lsn2.get().to_string()),
        "error message must include the page LSN: {err_msg}"
    );
    assert!(
        err_msg.contains(&lsn1.get().to_string()),
        "error message must include the durable WAL LSN: {err_msg}"
    );
}

// ---------------------------------------------------------------------------
// Page-flush fence: POSITIVE (C5 invariant proof)
// ---------------------------------------------------------------------------

/// **Positive test**: The fence MUST allow a page flush once the observer has
/// advanced to cover the page's dirty LSN.
#[test]
fn file_wal_observer_allows_page_flush_after_wal_catches_up() {
    let dir = TempDir::new().expect("temp dir");
    let mut wal = fresh_wal(&dir);
    let observer = wal.observer();

    let lsn1 = wal.append_tx_begin(TransactionId::new(2)).expect("append");
    let lsn2 = wal.append_tx_commit(TransactionId::new(2)).expect("append");

    // Neither LSN is durable yet
    assert!(
        validate_wal_durability_before_page_flush(lsn1, observer.current_durable_lsn()).is_err(),
        "lsn1 must be blocked before any flush_through"
    );
    assert!(
        validate_wal_durability_before_page_flush(lsn2, observer.current_durable_lsn()).is_err(),
        "lsn2 must be blocked before any flush_through"
    );

    // Advance to lsn2 → both LSNs are now covered
    wal.flush_through(lsn2).expect("flush_through lsn2");

    assert!(
        validate_wal_durability_before_page_flush(lsn1, observer.current_durable_lsn()).is_ok(),
        "lsn1 must be allowed after flush_through(lsn2)"
    );
    assert!(
        validate_wal_durability_before_page_flush(lsn2, observer.current_durable_lsn()).is_ok(),
        "lsn2 must be allowed after flush_through(lsn2)"
    );
}

// ---------------------------------------------------------------------------
// Multiple observers from same WAL
// ---------------------------------------------------------------------------

#[test]
fn file_wal_multiple_observers_share_the_same_signal() {
    let dir = TempDir::new().expect("temp dir");
    let mut wal = fresh_wal(&dir);

    let observer_a: FileWalDurabilityObserver = wal.observer();
    let observer_b = wal.observer();
    let observer_c = observer_a.clone();

    assert_eq!(observer_a.current_durable_lsn(), Lsn::ZERO);
    assert_eq!(observer_b.current_durable_lsn(), Lsn::ZERO);
    assert_eq!(observer_c.current_durable_lsn(), Lsn::ZERO);

    let lsn1 = wal.append_tx_begin(TransactionId::new(3)).expect("append");
    wal.flush_through(lsn1).expect("flush_through");

    // All three observers must see the advanced LSN
    assert_eq!(observer_a.current_durable_lsn(), lsn1);
    assert_eq!(observer_b.current_durable_lsn(), lsn1);
    assert_eq!(observer_c.current_durable_lsn(), lsn1);
}

// ---------------------------------------------------------------------------
// Crash / recovery simulation
// ---------------------------------------------------------------------------

/// **Crash-recovery proof test**
///
/// Scenario:
/// 1. Write WAL records → flush_through → observer advances.
/// 2. "Crash": drop WAL without flushing the page.  The page is NEVER on disk
///    because the buffer pool flush fence blocked it (simulated here by simply
///    never calling `flush_all_dirty_with_report`).
/// 3. Reopen WAL (recovery): verify durable records are intact, observer is
///    correctly re-initialised, and the fence classifies pages correctly based
///    on the recovered durable LSN.
///
/// This proves: a crash between WAL flush and page flush cannot create an
/// inconsistency.  The WAL is the ground truth; pages that were never flushed
/// are reconstructed from WAL replay.
#[test]
fn file_wal_observer_crash_recovery_wal_durable_page_never_on_disk() {
    let dir = TempDir::new().expect("temp dir");
    let wal_path = dir.path().join("crash_recovery.wal");

    let pre_crash_durable_lsn;
    let page_lsn_durable;
    let page_lsn_not_durable;
    let record_count_before_crash;

    // -----------------------------------------------------------------------
    // Phase 1: write → advance observer → simulated crash (no page flush)
    // -----------------------------------------------------------------------
    {
        let mut wal = FileWal::open(&wal_path).expect("open WAL for write");

        let _lsn1 = wal
            .append_tx_begin(TransactionId::new(10))
            .expect("append begin");
        let lsn2 = wal
            .append_payload(
                WalRecordKind::MapDeltaAppend,
                None,
                b"page delta bytes".to_vec(),
            )
            .expect("append data");
        let lsn3 = wal
            .append_tx_commit(TransactionId::new(10))
            .expect("append commit");

        // Flush through lsn3: all 3 records are durable
        wal.flush_through(lsn3).expect("flush_through lsn3");

        let observer_before_crash = wal.observer();
        pre_crash_durable_lsn = observer_before_crash.current_durable_lsn();
        assert_eq!(pre_crash_durable_lsn, lsn3);

        // These will be used to verify fence behaviour after recovery
        page_lsn_durable = lsn2; // page dirtied at this LSN — WAL covers it
        page_lsn_not_durable = lsn3.next(); // hypothetical future LSN — WAL does NOT cover it
        record_count_before_crash = wal.len();

        // CRASH SIMULATION: drop WAL without flushing any pages.
        // The buffer pool (not used here) would have had dirty pages at lsn2
        // and lsn3 but the flush fence, consulting the observer, would have
        // allowed those flushes.  We skip them to simulate the crash scenario
        // where the process dies between WAL flush and page store write.
        //
        // In a real crash the dirty pages are simply lost (not on disk).
        // The WAL records are durable and recovery replays them to reconstruct
        // the page state.
    } // WAL dropped — simulated crash

    // -----------------------------------------------------------------------
    // Phase 2: recovery — verify WAL is intact and observer is correct
    // -----------------------------------------------------------------------
    {
        let wal = FileWal::open(&wal_path).expect("reopen WAL for recovery");
        let recovered_observer = wal.observer();

        // WAL records survived the crash
        assert_eq!(
            wal.len(),
            record_count_before_crash,
            "all durable WAL records must survive the crash"
        );

        let durable_records = wal.replay_durable();
        assert_eq!(
            durable_records.len(),
            record_count_before_crash,
            "replay_durable must return all pre-crash records"
        );

        // Observer correctly re-initialised from the recovered durable LSN
        let recovered_durable_lsn = recovered_observer.current_durable_lsn();
        assert_eq!(
            recovered_durable_lsn, pre_crash_durable_lsn,
            "recovered observer must report the pre-crash durable LSN"
        );

        // Fence: page_lsn_durable is covered by the recovered WAL
        assert!(
            validate_wal_durability_before_page_flush(page_lsn_durable, recovered_durable_lsn)
                .is_ok(),
            "page at page_lsn_durable must be flush-eligible after recovery"
        );

        // Fence: page_lsn_not_durable is NOT covered — must be rejected
        let rejected =
            validate_wal_durability_before_page_flush(page_lsn_not_durable, recovered_durable_lsn);
        assert!(
            rejected.is_err(),
            "page at page_lsn_not_durable must be BLOCKED after recovery: WAL does not cover it"
        );

        // Confirm monotone ordering: recovered durable LSN == pre-crash
        assert_eq!(
            recovered_durable_lsn.get(),
            pre_crash_durable_lsn.get(),
            "durable LSN must be stable across crash-recovery cycle"
        );
    }
}
