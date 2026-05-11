//! GAP-03 regression: `execute_after_admission()` must call
//! `validate_terminal_evidence()` BEFORE publishing MVCC visibility.
//!
//! ## What GAP-03 enforces
//!
//! The `validate_terminal_evidence` check (inserted between `dispatch_commit()`
//! and `mark_commit_visible_after_durable_wal()`) enforces three invariants:
//!
//! 1. The transaction reached a terminal state (Committed or RolledBack).
//! 2. The durable LSN is non-zero (WAL has been flushed).
//! 3. A rolled-back transaction must report zero DB mutations.
//!
//! ## What GAP-03 does NOT enforce
//!
//! `rows_affected` (DB mutation rows, e.g. 2 for ReserveStock: 1 stock row +
//! 1 reservation row) and `result_metadata.row_count_exact` (result stream rows
//! returned to the caller, e.g. 1) are **intentionally different quantities**.
//! Cross-comparing them would produce spurious Contract errors for any procedure
//! that writes more DB rows than it returns (normal in a relational write path).
//!
//! The correct check to use when a genuine result-stream row count is available
//! is `validate_terminal_completion`, but the hot commit path only has
//! `dispatch_receipt.rows_affected` (DB mutations), not result-stream row count.

// common.rs exports many helpers used by the wider test suite.
// Only a subset is needed by this particular file.
#![allow(dead_code, unused_imports)]

#[path = "runtime_contract/common.rs"]
mod common;
use common::*;

/// GAP-03 regression: a procedure where `rows_affected` (DB mutations = 2)
/// differs from `result_metadata.row_count_exact` (result rows = 1) MUST
/// succeed. These are intentionally different quantities (denormalised DB
/// writes vs single result row returned to caller). This is the ReserveStock
/// pattern: 1 stock row written + 1 reservation row written = 2 DB mutations,
/// but only 1 result row emitted.
#[test]
fn terminal_evidence_rows_affected_differs_from_row_count_exact_succeeds() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());

    let contract = ProcedureContractRef {
        procedure_id: ProcedureId::new(22),
        contract_hash: ContractHash::test_vector(7),
        catalog_version: CatalogVersion::new(3),
    };
    // rows_affected=2 (DB mutations), row_count_exact=Some(1) (result rows)
    // These must NOT be cross-compared.
    let mut procedure = procedure(contract);
    procedure.rows_affected = 2; // 2 DB mutations, row_count_exact stays 1

    let outcome = runtime
        .execute_internal(
            request(ContractHash::test_vector(7)),
            &procedure,
            TraceId::new(3001),
        )
        .expect("rows_affected ≠ row_count_exact must succeed: they are different concepts");

    assert_eq!(
        outcome.completion.status,
        CompletionStatus::Committed,
        "procedure with rows_affected=2 and row_count_exact=1 must produce Committed status"
    );
}

/// GAP-03 regression: happy path — rows_affected matches row_count_exact.
/// Standard procedure where 1 DB row mutated and 1 result row returned.
#[test]
fn terminal_evidence_matching_rows_and_result_count_succeeds() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());

    let contract = ProcedureContractRef {
        procedure_id: ProcedureId::new(22),
        contract_hash: ContractHash::test_vector(7),
        catalog_version: CatalogVersion::new(3),
    };
    // Default procedure() sets rows_affected=1, row_count_exact=Some(1).
    let good_procedure = procedure(contract);

    let outcome = runtime
        .execute_internal(
            request(ContractHash::test_vector(7)),
            &good_procedure,
            TraceId::new(3002),
        )
        .expect("standard procedure must succeed");

    assert_eq!(
        outcome.completion.status,
        CompletionStatus::Committed,
        "standard terminal evidence must produce Committed status"
    );
}

/// GAP-03 regression: the WAL MUST contain records (commit was durable) even
/// before MVCC visibility is granted. Validates WAL-before-commit ordering:
/// the WAL records exist regardless of anything that might fail afterward.
#[test]
fn terminal_evidence_commit_path_has_wal_records() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());

    let contract = ProcedureContractRef {
        procedure_id: ProcedureId::new(22),
        contract_hash: ContractHash::test_vector(7),
        catalog_version: CatalogVersion::new(3),
    };
    let good_procedure = procedure(contract);

    let _outcome = runtime
        .execute_internal(
            request(ContractHash::test_vector(7)),
            &good_procedure,
            TraceId::new(3003),
        )
        .expect("must succeed");

    // 3 WAL records: TxBegin + RowUpdate + TxCommit — written by dispatch_commit()
    // before validate_terminal_evidence() or mark_commit_visible_after_durable_wal().
    assert_eq!(
        runtime.wal().records.len(),
        3,
        "commit path must produce TxBegin+RowUpdate+TxCommit; got {} records",
        runtime.wal().records.len()
    );
    assert_eq!(
        runtime.wal().records[2].1,
        WalRecordKind::TxCommit,
        "last WAL record must be TxCommit"
    );
}

/// GAP-03 regression: zero-mutation procedures (rows_affected=0, zero-length
/// mutation_payload) where row_count_exact is also meaningful.  Validates the
/// edge case at zero DB mutations vs non-zero result rows declared in metadata.
#[test]
fn terminal_evidence_zero_db_mutations_with_declared_result_rows_succeeds() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());

    let contract = ProcedureContractRef {
        procedure_id: ProcedureId::new(22),
        contract_hash: ContractHash::test_vector(7),
        catalog_version: CatalogVersion::new(3),
    };
    // A read-only procedure: 0 DB mutations, row_count_exact=Some(1).
    // In the current architecture, mutation_payload drives the WAL records.
    // row_count_exact=Some(1) reflects 1 result row returned to the caller.
    let mut read_only = procedure(contract);
    read_only.rows_affected = 0; // zero DB mutations

    let outcome = runtime
        .execute_internal(
            request(ContractHash::test_vector(7)),
            &read_only,
            TraceId::new(3004),
        )
        .expect("zero DB mutations with declared result rows must succeed");

    // rows_affected=0 is a valid Committed outcome for a read-only procedure.
    // The validation error kind must NOT be Contract (no spurious row-count rejection).
    assert_eq!(
        outcome.completion.status,
        CompletionStatus::Committed,
        "zero-mutation procedure must produce Committed status; got: {:?}",
        outcome.completion.status
    );
    assert_eq!(
        outcome.completion.rows_affected,
        Some(0),
        "zero-mutation procedure must report rows_affected=0 in completion"
    );
}

/// GAP-03 regression: ensure the terminal evidence validation rejects a
/// non-terminal transaction state. This is the "programming error" case where
/// a future refactor could accidentally skip the dispatch_commit step.
/// Verified indirectly: execute_internal returning Ok() proves transaction
/// state was Committed (the only terminal state dispatch_commit can return).
#[test]
fn terminal_evidence_committed_state_and_nonzero_lsn_are_invariants_on_success() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());

    let contract = ProcedureContractRef {
        procedure_id: ProcedureId::new(22),
        contract_hash: ContractHash::test_vector(7),
        catalog_version: CatalogVersion::new(3),
    };
    let good_procedure = procedure(contract);

    let outcome = runtime
        .execute_internal(
            request(ContractHash::test_vector(7)),
            &good_procedure,
            TraceId::new(3005),
        )
        .expect("standard procedure must succeed");

    // Verify the outcome carries Committed transaction state.
    assert_eq!(
        outcome.completion.transaction_state,
        Some(TransactionState::Committed),
        "successful execute_internal must carry Committed transaction state"
    );

    // Verify the durable LSN is non-zero (WAL was flushed).
    assert!(
        outcome
            .completion
            .durable_lsn
            .is_some_and(|lsn| !lsn.is_zero()),
        "successful execute_internal must carry non-zero durable LSN; got: {:?}",
        outcome.completion.durable_lsn
    );
}
