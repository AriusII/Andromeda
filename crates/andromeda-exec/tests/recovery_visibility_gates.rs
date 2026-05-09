//! End-to-end recovery-visibility gates.
//!
//! These tests join the storage WAL recovery report with the MVCC visibility
//! contract, proving the doctrine that:
//!
//! * Visible commit requires durable WAL — a writer that crashed before its
//!   commit reached durable storage must remain invisible to peer snapshots
//!   reconstructed from the recovered status table. RAM is never truth.
//! * A cold snapshot + durable WAL is reconstructible truth — once the commit
//!   record is durable on the WAL, recovery must replay the writer's row
//!   record(s) and reconstruct a status table under which the row becomes
//!   visible to peer snapshots.
//! * Recovery must never fabricate committed visibility for incomplete or
//!   rolled-back transactions; the status table built from
//!   `transaction_evidence` only ever contains `Committed` / `RolledBack`
//!   for transactions whose terminal record is durably on disk.

use andromeda_core::{CatalogVersion, TransactionId};
use andromeda_storage::write_ahead_log::file::{
    FileWal, FileWalRecoveryBoundaryKind, FileWalRecoveryIgnoredTransactionReason,
    recover_from_file_wal, report_file_wal_recovery_v0,
};
use andromeda_storage::write_ahead_log::record::WalRecordKind;
use andromeda_storage::{
    DatabaseManifest, DurableTransactionResume, DurableTransactionState, Lsn, RedoRecordDecision,
    StartupMode, plan_file_wal_startup_recovery_v0,
};
use andromeda_tx::Lsn as TxLsn;
use andromeda_tx::TransactionManager;
use andromeda_tx::{
    MvccIsolationPolicy, MvccRowHeader, Snapshot, TransactionStatus, TransactionStatusTable,
};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

struct TempWalPath {
    path: PathBuf,
}

impl TempWalPath {
    fn new(name: &str) -> Self {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "andromeda-exec-recovery-visibility-{name}-{}-{suffix}.wal",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        Self { path }
    }
}

impl Drop for TempWalPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn manifest() -> DatabaseManifest {
    DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: Lsn::new(1),
        previous_manifest_hash: [0; 32],
        manifest_crc: 7,
    }
}

/// Project a recovery `transaction_evidence` slice into the MVCC
/// `TransactionStatusTable` used by visibility checks. Only **durable**
/// terminal states are mirrored; `Open` / `Incomplete` transactions remain
/// absent (treated as InFlight by V0 doctrine), and we deliberately never
/// fabricate `Committed` from RAM.
fn status_table_from_recovery(evidence: &[DurableTransactionResume]) -> TransactionStatusTable {
    let statuses = TransactionStatusTable::new();
    for resume in evidence {
        match resume.state {
            DurableTransactionState::Committed => {
                let commit_lsn = resume
                    .commit_lsn
                    .expect("committed recovery evidence must carry commit LSN");
                statuses
                    .record_committed_after_durable_wal(
                        resume.transaction_id,
                        TxLsn::new(commit_lsn.get()),
                        TxLsn::new(resume.last_lsn.get()),
                    )
                    .expect("committed evidence always has durable WAL evidence")
            },
            DurableTransactionState::RolledBack => {
                let rollback_lsn = resume
                    .rollback_lsn
                    .expect("rolled-back recovery evidence must carry rollback LSN");
                statuses
                    .record_rolled_back_after_durable_wal(
                        resume.transaction_id,
                        TxLsn::new(rollback_lsn.get()),
                        TxLsn::new(resume.last_lsn.get()),
                    )
                    .expect("rolled-back evidence always has durable WAL evidence")
            },
            DurableTransactionState::Open | DurableTransactionState::Incomplete => {
                // Doctrine: do NOT promote in-flight or torn writers to a
                // terminal status. Their visibility must derive from
                // absence in the status table.
            },
        }
    }
    statuses
}

fn read_committed_peer_snapshot(
    timestamp: u64,
    reader: TransactionId,
    statuses: &TransactionStatusTable,
) -> Snapshot {
    Snapshot::with_context_validated(
        timestamp,
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        Some(reader),
        Vec::<TransactionId>::new(),
        statuses,
    )
    .expect("peer reader snapshot must validate against the recovered status table")
}

fn assert_no_committed_evidence(evidence: &[DurableTransactionResume], writer: TransactionId) {
    let writer_state = evidence
        .iter()
        .find(|resume| resume.transaction_id == writer)
        .map(|resume| resume.state);
    assert_ne!(
        writer_state,
        Some(DurableTransactionState::Committed),
        "recovery must not synthesize a Committed state for an undurable transaction"
    );
}

/// Crash **before** the commit record reaches durable WAL bytes.
///
/// The writer appended a row but only the row LSN was flushed; the commit
/// record never made it (or never existed). On reopen:
///
/// * `RecoveryPlan` must replay nothing for that writer.
/// * The recovery report must classify the writer as `Incomplete` and emit
///   a `Clean` boundary (torn-tail handling, not forensic).
/// * The reconstructed `TransactionStatusTable` must NOT mark the writer
///   `Committed`, and the writer's row header must be invisible to a peer
///   snapshot under ReadCommitted — RAM observations are not truth.
#[test]
fn crash_before_durable_commit_keeps_writes_invisible_after_recovery() {
    let temp = TempWalPath::new("before-commit");
    let writer = TransactionId::new(0xC0_DE_01);
    let row_begin_ts: u64 = 100;

    {
        let mut wal = FileWal::open(&temp.path).expect("fresh WAL opens");
        wal.append_tx_begin(writer).expect("begin appends");
        let row_lsn = wal
            .append_payload(WalRecordKind::RowInsert, Some(writer), b"pre-crash-row")
            .expect("row append succeeds");
        // Make ONLY the row durable, never the commit. Simulates a crash
        // strictly before the durable commit barrier.
        wal.flush_through(row_lsn).expect("row LSN flushes");
        // wal dropped here — process crash; no tx_commit was ever appended.
    }

    let plan = recover_from_file_wal(&manifest(), StartupMode::SafeStart, &temp.path)
        .expect("recovery plan computes from file WAL");

    assert!(
        plan.replay_lsns().collect::<Vec<_>>().is_empty(),
        "no replay records may be produced for an undurable-commit writer"
    );
    assert!(
        plan.has_incomplete_transactions(),
        "writer without durable commit must surface as incomplete"
    );
    assert_no_committed_evidence(&plan.transaction_evidence, writer);

    let report = report_file_wal_recovery_v0(&manifest(), StartupMode::SafeStart, &temp.path)
        .expect("recovery report computes");
    assert_eq!(report.boundary_kind, FileWalRecoveryBoundaryKind::Clean);
    assert!(!report.forensic_required);
    assert!(report.replay_lsns().collect::<Vec<_>>().is_empty());
    assert!(
        report
            .ignored_transactions
            .iter()
            .any(|ignored| ignored.transaction_id == writer
                && ignored.reason == FileWalRecoveryIgnoredTransactionReason::Incomplete)
    );

    // Recovery → status table → MVCC visibility check.
    let mut statuses = status_table_from_recovery(&plan.transaction_evidence);
    assert_eq!(
        statuses.status(writer),
        None,
        "an undurable writer must be absent from the recovered status table"
    );
    assert!(
        !statuses.is_durable_committed(writer),
        "doctrine: visible commit requires durable WAL — recovery cannot fabricate it"
    );

    let reader = TransactionId::new(0xC0_DE_02);
    let peer_statuses = &mut statuses;
    peer_statuses
        .record(reader, TransactionStatus::InFlight)
        .expect("reader registers as in-flight");
    let peer_snapshot = read_committed_peer_snapshot(row_begin_ts + 1, reader, peer_statuses);

    let row = MvccRowHeader::open_version(row_begin_ts, writer, None)
        .expect("row header builds for writer");

    assert!(
        !row.visible_in_snapshot(&peer_snapshot, peer_statuses)
            .expect("visibility check completes"),
        "writer row must remain INVISIBLE after crash-before-durable-commit recovery"
    );
}

/// Crash **after** the commit record is durably flushed.
///
/// The writer appended a row, appended its commit, and `flush_through` the
/// commit LSN before the simulated crash. On reopen:
///
/// * `RecoveryPlan` must list the row LSN as a committed replay record.
/// * `transaction_evidence` must show `DurableTransactionState::Committed`
///   for the writer.
/// * The reconstructed `TransactionStatusTable` must mark the writer as
///   `Committed`, and the writer's row header must be visible to a peer
///   snapshot — durable WAL + status reconstruction = truth.
#[test]
fn crash_after_durable_commit_keeps_writes_visible_and_replayable() {
    let temp = TempWalPath::new("after-commit");
    let writer = TransactionId::new(0xC0_DE_11);
    let row_begin_ts: u64 = 100;

    let row_lsn = {
        let mut wal = FileWal::open(&temp.path).expect("fresh WAL opens");
        wal.append_tx_begin(writer).expect("begin appends");
        let row_lsn = wal
            .append_payload(WalRecordKind::RowInsert, Some(writer), b"post-commit-row")
            .expect("row append succeeds");
        let commit_lsn = wal.append_tx_commit(writer).expect("commit appends");
        // Durable commit barrier: the commit record is on disk before the
        // simulated crash. Per doctrine, this is the moment visibility may
        // become truth on the next startup.
        wal.flush_through(commit_lsn).expect("commit LSN flushes");
        row_lsn
        // wal dropped here — process crash AFTER durable commit barrier.
    };

    let plan = recover_from_file_wal(&manifest(), StartupMode::SafeStart, &temp.path)
        .expect("recovery plan computes from file WAL");

    let replay = plan.replay_lsns().collect::<Vec<_>>();
    assert_eq!(
        replay,
        vec![row_lsn],
        "durable-committed writer must contribute exactly its row LSN to replay"
    );
    assert!(
        !plan.has_incomplete_transactions(),
        "no transaction may be classified as incomplete after a durable commit"
    );

    let writer_evidence = plan
        .transaction_evidence
        .iter()
        .find(|resume| resume.transaction_id == writer)
        .expect("writer must appear in transaction evidence");
    assert_eq!(
        writer_evidence.state,
        DurableTransactionState::Committed,
        "writer evidence must reflect durable commit state"
    );

    let report = report_file_wal_recovery_v0(&manifest(), StartupMode::SafeStart, &temp.path)
        .expect("recovery report computes");
    assert_eq!(report.boundary_kind, FileWalRecoveryBoundaryKind::Clean);
    assert!(!report.forensic_required);
    assert_eq!(report.replay_lsns().collect::<Vec<_>>(), vec![row_lsn]);
    assert!(report.ignored_transactions.is_empty());

    // Recovery → status table → MVCC visibility check.
    let mut statuses = status_table_from_recovery(&plan.transaction_evidence);
    assert_eq!(
        statuses.status(writer),
        Some(TransactionStatus::Committed),
        "writer must be reconstructed as Committed from durable WAL evidence"
    );
    assert!(statuses.is_durable_committed(writer));

    let reader = TransactionId::new(0xC0_DE_12);
    let peer_statuses = &mut statuses;
    peer_statuses
        .record(reader, TransactionStatus::InFlight)
        .expect("reader registers as in-flight");
    let peer_snapshot = read_committed_peer_snapshot(row_begin_ts + 1, reader, peer_statuses);

    let row = MvccRowHeader::open_version(row_begin_ts, writer, None)
        .expect("row header builds for writer");

    assert!(
        row.visible_in_snapshot(&peer_snapshot, peer_statuses)
            .expect("visibility check completes"),
        "writer row MUST be visible after crash-after-durable-commit recovery"
    );
}

/// Crash **after** a rollback record is durably flushed.
///
/// The writer's row bytes are present in durable WAL, but durable rollback is
/// the terminal transaction evidence. On restart:
///
/// * the row record is classified `SkipRolledBackTransaction`;
/// * the reconstructed status table marks the writer `RolledBack`, not
///   `Committed`;
/// * the row remains invisible to a peer snapshot; and
/// * the transaction-id allocator floor derived from durable WAL evidence makes
///   the next allocated id strictly greater than every recovered writer id.
#[test]
fn crash_after_durable_rollback_keeps_writes_invisible_and_seeds_allocator_above_floor() {
    let temp = TempWalPath::new("after-rollback");
    let writer = TransactionId::new(0xC0_DE_31);
    let row_begin_ts: u64 = 100;

    let row_lsn = {
        let mut wal = FileWal::open(&temp.path).expect("fresh WAL opens");
        wal.append_tx_begin(writer).expect("begin appends");
        let row_lsn = wal
            .append_payload(WalRecordKind::RowDelete, Some(writer), b"rolled-back-row")
            .expect("row append succeeds");
        let rollback_lsn = wal.append_tx_rollback(writer).expect("rollback appends");
        wal.flush_through(rollback_lsn)
            .expect("rollback LSN flushes");
        row_lsn
        // wal dropped here — process crash AFTER durable rollback barrier.
    };

    let startup =
        plan_file_wal_startup_recovery_v0(&manifest(), StartupMode::SafeStart, &temp.path, false)
            .expect("startup recovery seam computes from durable WAL scan");
    assert!(startup.decision.is_accepted());
    assert!(startup.replay_allowed());
    assert_eq!(
        startup.transaction_manager_allocator_floor(),
        writer.get(),
        "allocator floor must be the max transaction id recovered from durable WAL evidence"
    );
    let recovered_manager =
        TransactionManager::with_recovered_floor(startup.transaction_manager_allocator_floor());
    let next_after_recovery = recovered_manager
        .begin()
        .expect("post-recovery transaction allocation succeeds");
    assert!(
        next_after_recovery.get() > writer.get(),
        "post-recovery allocator must issue ids strictly above the recovered floor"
    );

    let plan = startup
        .redo_plan
        .as_ref()
        .expect("SafeStart should produce a redo plan for a clean durable rollback");
    assert_eq!(
        plan.replay_lsns().collect::<Vec<_>>(),
        Vec::<Lsn>::new(),
        "rolled-back writer must not contribute row records to redo"
    );
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == row_lsn)
            .expect("rolled-back row should appear in the durable plan")
            .decision,
        RedoRecordDecision::SkipRolledBackTransaction
    );

    let writer_evidence = plan
        .transaction_evidence
        .iter()
        .find(|resume| resume.transaction_id == writer)
        .expect("writer must appear in transaction evidence");
    assert_eq!(
        writer_evidence.state,
        DurableTransactionState::RolledBack,
        "writer evidence must reflect durable rollback state"
    );

    let mut statuses = status_table_from_recovery(&plan.transaction_evidence);
    assert_eq!(
        statuses.status(writer),
        Some(TransactionStatus::RolledBack),
        "writer must be reconstructed as RolledBack from durable WAL evidence"
    );
    assert!(
        !statuses.is_durable_committed(writer),
        "rolled-back writer must not be promoted to committed visibility"
    );

    let reader = TransactionId::new(12_639_794);
    let peer_statuses = &mut statuses;
    peer_statuses
        .record(reader, TransactionStatus::InFlight)
        .expect("reader registers as in-flight");
    let peer_snapshot = read_committed_peer_snapshot(row_begin_ts + 1, reader, peer_statuses);

    let row = MvccRowHeader::open_version(row_begin_ts, writer, None)
        .expect("row header builds for writer");

    assert!(
        !row.visible_in_snapshot(&peer_snapshot, peer_statuses)
            .expect("visibility check completes"),
        "writer row must remain INVISIBLE after crash-after-durable-rollback recovery"
    );
}

/// Reopening the same WAL file after the crash-before-commit case must
/// produce the *same* recovery verdict twice — recovery is deterministic
/// and never grows visibility on subsequent startups.
#[test]
fn crash_before_durable_commit_recovery_is_idempotent() {
    let temp = TempWalPath::new("idempotent-before");
    let writer = TransactionId::new(0xC0_DE_21);

    {
        let mut wal = FileWal::open(&temp.path).expect("fresh WAL opens");
        wal.append_tx_begin(writer).expect("begin appends");
        let row_lsn = wal
            .append_payload(WalRecordKind::RowUpdate, Some(writer), b"torn")
            .expect("row append succeeds");
        wal.flush_through(row_lsn).expect("row LSN flushes");
    }

    let first =
        recover_from_file_wal(&manifest(), StartupMode::SafeStart, &temp.path).expect("first plan");
    let second = recover_from_file_wal(&manifest(), StartupMode::SafeStart, &temp.path)
        .expect("second plan");

    assert_eq!(
        first.replay_lsns().collect::<Vec<_>>(),
        second.replay_lsns().collect::<Vec<_>>(),
        "recovery must be deterministic across reopens"
    );
    let first_states: Vec<_> = first
        .transaction_evidence
        .iter()
        .map(|r| (r.transaction_id, r.state))
        .collect();
    let second_states: Vec<_> = second
        .transaction_evidence
        .iter()
        .map(|r| (r.transaction_id, r.state))
        .collect();
    assert_eq!(first_states, second_states);
    assert_no_committed_evidence(&second.transaction_evidence, writer);
}

// End of file.
