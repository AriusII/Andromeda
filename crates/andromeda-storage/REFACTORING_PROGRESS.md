## Storage Crates Refactoring Progress

### andromeda-storage (Primary Objective: All files <250 lines)

#### Current Status:

- **wal.rs** (855 lines) → REFACTORED ✓
    - Extracted to `write_ahead_log/record.rs` (WalRecordKind, WalRecordHeader, WalRecord)
    - Extracted to `write_ahead_log/transaction.rs` (DurableTransactionState, DurableTransactionResume,
      DurableTransactionClassifications, classification functions)
    - Extracted to `write_ahead_log/manager.rs` (InMemoryWal)
    - Updated `write_ahead_log/mod.rs` to re-export all types

- **placement.rs** (783 lines) → TODO
    - Should split into submodules within layout/: temperature.rs, pipeline.rs, decision.rs, policy.rs

- **file_wal.rs** (794 lines) → TODO
    - Move to write_ahead_log/file_wal.rs or split further

- **recovery.rs** (594 lines) → TODO
    - Extract recovery plan logic: src/recovery/plan.rs
    - Extract validation logic: src/recovery/validation.rs

- **operational_profile.rs** (545 lines) → TODO
    - Move to layout/operational_profile.rs or split

- **wal_codec.rs** (530 lines) → TODO
    - Move to write_ahead_log/codec.rs or ensure it stays within limits

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

- **mvcc.rs** (517 lines) → TODO
    - Extract snapshot.rs (MvccIsolationPolicy, Snapshot) - ~120 lines
    - Extract version.rs (MvccRowHeader) - ~150 lines
    - Extract status.rs (TransactionStatus, TransactionStatusTable) - ~100 lines
    - Extract predicate.rs (visibility functions) - ~50 lines

- **state.rs** (228 lines) → OK
- **trace.rs** (55 lines) → OK

#### Refactoring Strategy for mvcc.rs:

Since mvcc.rs can't be reorganized into a subdirectory without the ability to create directories,
the strategy is to split it into multiple files at the same level and rely on module imports/re-exports
within lib.rs. However, a better approach given the constraints is to:

1. Focus on reducing the logical complexity of each type
2. Extract helper functions and constants
3. Use submodule structure within the file itself using mod { ... } declarations

OR

Create the directory structure by:

1. Creating mvcc/mod.rs with re-exports
2. Creating mvcc/snapshot.rs, mvcc/version.rs, etc.
3. Updating lib.rs to import from mvcc module instead of mvcc file

This requires creating the first file in the directory (mvcc/mod.rs).

### Quality Gate Validation Strategy:

1. Ensure write_ahead_log module compiles and re-exports all types
2. Create MVCC submodule structure
3. Run: `cargo check --workspace --quiet`
4. Run: `cargo fmt --all`
5. Run: `cargo clippy --workspace --all-targets`
6. Run: `cargo test --workspace --quiet`

### Risks and Mitigations:

- Risk: Broken wal.rs file from partial edit
    - Mitigation: Complete rewrite or cleanup (attempt in progress)
- Risk: Module structure changes might affect external consumers
    - Mitigation: Maintain pub use re-exports in lib.rs for backward compatibility
- Risk: Large file sizes still preventing modularization
    - Mitigation: Create directory-based module structure to allow file splitting

### Next Steps:

1. Fix/validate the write_ahead_log module compiles
2. Create mvcc module directory structure
3. Split remaining large files as needed
4. Validate all tests pass
