## Storage Crates Refactoring Progress

> **Status note (Wave 1):** the recovery and WAL refactor items below are
> complete. `recovery.rs` is now backed by `recovery/{coverage,planning,trace}.rs`,
> `file_wal.rs` re-exports `file_wal/{format,scan,report}.rs`, and `wal_codec.rs`
> re-exports `wal_codec/{binary,checksum,frame,record,scan}.rs`. Owner of WAL
> recovery / forensic-boundary classification is the recovery module; do not
> reintroduce duplicate plan/coverage logic at the crate root.

### andromeda-storage (Primary Objective: All files <250 lines)

#### Current Status:

- **wal.rs** (855 lines) → REFACTORED ✓
    - Extracted to `write_ahead_log/record.rs` (WalRecordKind, WalRecordHeader, WalRecord)
    - Extracted to `write_ahead_log/transaction.rs` (DurableTransactionState, DurableTransactionResume,
      DurableTransactionClassifications, classification functions)
    - Extracted to `write_ahead_log/manager.rs` (InMemoryWal)
    - Updated `write_ahead_log/mod.rs` to re-export all types

- **placement.rs** (783 lines) → REFACTORED ✓
    - Split into `placement/{budget,decision,policy,types,workload}.rs`.

- **file_wal.rs** (794 lines) → REFACTORED ✓
    - Split into `file_wal/{format,scan,report}.rs`. Recovery boundary
      classification (Clean / RecoverableTail / ForensicChainBreak) lives in
      `file_wal/report.rs` and is the single source of truth for the
      `report_file_wal_recovery_v0` facade.

- **recovery.rs** (594 lines) → REFACTORED ✓
    - Split into `recovery/{coverage,planning,trace}.rs`. `RecoveryPlan` and
      `ConceptualRedoPlan` are the canonical recovery surface and expose three
      explicit boundary slots (mounted snapshot, WAL replay range, corrupt /
      unavailable segment boundary). The canonical observability event type is
      `andromeda_observe::RecoveryTrace`; the storage-internal `RecoveryTrace`
      retained at this crate boundary is a forensic *input* snapshot only.

- **operational_profile.rs** (545 lines) → REFACTORED ✓
    - Split into `operational_profile/{budgets,constants,errors,hardware,profile,workflow,tests}.rs`.

- **wal_codec.rs** (530 lines) → REFACTORED ✓
    - Split into `wal_codec/{binary,checksum,frame,record,scan}.rs`.

- **io_budget.rs** (415 lines) → BORDERLINE (within 250 is ideal)
    - Consider moving to placement/ module

- **manifest.rs** (396 lines) → BORDERLINE

- **page.rs** (357 lines) → BORDERLINE

#### Files Already Meeting Target:

- **segment.rs** (281 lines) → OK
- **wal_segment.rs** (172 lines) → OK
- **extent.rs** (122 lines) → OK
- **cold_store.rs** (111 lines) → OK
- **lsn.rs** (63 lines) → OK

### andromeda-tx (Primary Objective: All files <300 lines)

#### Current Status:

- **mvcc.rs** (30 lines) → REFACTORED ✓
    - Compatibility facade re-exporting `mvcc_snapshot::{MvccIsolationPolicy,
      Snapshot}`, `mvcc_status::{TransactionStatus, TransactionStatusTable}`,
      and `mvcc_version::{creator_is_visible, delete_is_visible,
      MvccRowHeader}`. Do not reintroduce implementation here.
- **mvcc_snapshot.rs** → OK (snapshots and isolation policy)
- **mvcc_status.rs** → OK (transaction status table)
- **mvcc_version.rs** → OK (row header, visibility predicates)
- **state.rs** (228 lines) → OK
- **trace.rs** (55 lines) → OK

### Quality Gate Validation Strategy:

1. `cargo check --workspace --quiet`
2. `cargo fmt --all`
3. `cargo clippy --workspace --all-targets`
4. `cargo test --workspace --quiet`

### Notes for Future Maintenance:

- Keep `wal.rs`, `file_wal.rs`, `recovery.rs`, `wal_codec.rs`, `placement.rs`,
  and `operational_profile.rs` as thin re-export facades over their
  sub-directories. Do not move types back to the crate root.
- The canonical observability event for recovery is
  `andromeda_observe::RecoveryTrace`; the storage-internal `RecoveryTrace`
  is a forensic *input* snapshot only.
- WAL submodule facades (`write_ahead_log/file.rs`, `segment`, `codec`) MUST
  NOT define new types; add them in the canonical owner module and
  re-export.

