//! Integration tests for execution path (admission → dispatch → commit)
//!
//! This test suite validates the complete execution pipeline with focus on:
//! 1. End-to-End Procedure Invocation — Single procedure call with real transaction
//! 2. Permission Enforcement — Denied invocations produce audit traces
//! 3. Contract Validation — Mismatched contracts rejected before transaction
//! 4. Result Streaming — Batches emitted with correct metadata
//! 5. Rollback on Error — Procedure failure triggers controlled rollback
//! 6. Concurrent Invocations — Multiple procedures in parallel (MVCC isolation)
//!
//! Test Coverage Matrix:
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │ Test ID │ Phase        │ Component              │ Invariant             │
//! ├─────────────────────────────────────────────────────────────────────────┤
//! │ E2E.1   │ Admission    │ AdmissionService       │ Valid invocation OK   │
//! │ E2E.2   │ Admission    │ PermissionEvaluator    │ Denied blocked + event│
//! │ E2E.3   │ Admission    │ ContractValidator      │ Hash mismatch reject  │
//! │ E2E.4   │ Dispatch     │ LocalDispatcher        │ Mutation LOC accurate │
//! │ E2E.5   │ Commit       │ TransactionStateMachine│ WAL evidence valid    │
//! │ E2E.6   │ Rollback     │ LocalRollback          │ Rollback payload set  │
//! │ E2E.7   │ Concurrency  │ MVCC tx isolation      │ No dirty reads        │
//! │ E2E.8   │ Audit        │ EventEmitter           │ Events correlated     │
//! └─────────────────────────────────────────────────────────────────────────┘

#[path = "integration_execution_path/admission_and_contracts.rs"]
mod admission_and_contracts;
#[path = "integration_execution_path/commit_and_wal.rs"]
mod commit_and_wal;
#[path = "integration_execution_path/observability_matrix.rs"]
mod observability_matrix;
#[path = "integration_execution_path/rollback_and_concurrency.rs"]
mod rollback_and_concurrency;
#[path = "integration_execution_path/support.rs"]
mod support;
