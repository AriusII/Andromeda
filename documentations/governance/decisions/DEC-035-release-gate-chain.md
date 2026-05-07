# DEC-035: release gate cycle Release Gates

**Status:** ACCEPTED ✓  
**Date:** 2026-06-16  
**Authors:** Release Governance Agent, Recovery Team, Observability Team, Transaction Kernel Team  
**Stakeholders:** Storage, Execution, Security, Catalog, HA/DR, Architecture

---

## Executive summary

This decision locks the release gate cycle blocking release contract for seven non-negotiable gates and ratifies the audit, transaction, WAL, and recovery proof surfaces used to evaluate release readiness. The gates are now enforced as doctrine-level release criteria and mapped to implementation evidence, test plans, and mitigations. A release is blocked if any gate fails.

---

## Decision

release gate cycle release approval requires all seven gates to pass:

1. Recovery failure
2. Visibility violation
3. ContractHash mismatch acceptance
4. Audit omission
5. Panic in critical path
6. Silent corruption
7. Non-reproducible crash

The matrix below is binding for release governance and supersedes implicit gate interpretation.

### 1) Seven-gate release readiness matrix (7x5)

| Gate | Specification locked | Test plan locked | Implementation status | Risk mitigation |
|---|---|---|---|---|
| **G1 Recovery failure** | Recovery must mount manifest + durable WAL, replay committed durable records, skip incomplete transactions, and emit typed boundary evidence (`RecoveryTrace`, corruption boundary). | `crates/andromeda-storage/tests/recovery_completeness_contract.rs`, `crates/andromeda-storage/tests/crash_recovery_impl.rs` (40 scenarios), `crates/andromeda-storage/tests/wal_scan_recovery_contract.rs`, `crates/andromeda-exec/tests/recovery_visibility_gates.rs`. | Implemented in `andromeda-storage` recovery planning/replay and file WAL recovery report path. | Fail-stop on recovery planning/replay errors; boundary classification (`Clean`/forensic) prevents unsafe open. |
| **G2 Visibility violation** | Visible state is commit-durable only. No row may appear committed without durable WAL evidence and status reconstruction. | `crates/andromeda-exec/tests/recovery_visibility_gates.rs`, `crates/andromeda-tx/tests/commit_log_durability.rs`, `crates/andromeda-tx/tests/mvcc_gc_durability_contract.rs`. | Implemented in TX state machine + MVCC status-driven visibility + recovery status-table reconstruction. | Reject illegal transitions; require durable commit LSN before `Committed`; replay skips incomplete transactions. |
| **G3 ContractHash mismatch** | Invocation must reject hash mismatch before transaction creation and before WAL append/dispatch. | `crates/andromeda-exec/src/invocation.rs` tests, `crates/andromeda-exec/tests/core_io_gates.rs`, `crates/andromeda-exec/tests/v0_vertical_e2e.rs`, `crates/andromeda-exec/tests/integration_execution_path.rs`. | Implemented in admission and pre-transaction validators; status mapped to `ContractRejected`. | Fail closed in admission; mismatch emits typed rejection trace and blocks execution path. |
| **G4 Audit omission** | Critical decisions must produce typed event envelopes with non-empty evidence and schema validation; no silent drop in durable sink contract. | `crates/andromeda-observe/tests/audit_family_contract.rs`, `crates/andromeda-observe/tests/durable_audit_sink_contract.rs`, `crates/andromeda-exec/tests/c6_recovery_security_audit.rs`, `crates/andromeda-observe/tests/v0_procedure_lifecycle.rs`. | Implemented in `EventEnvelope::validate`, trace structs, and durable audit sink failure model. | Fail-closed durable sink failure kinds; no global audit disable switch; replay/query surfaces retained. |
| **G5 Panic in critical path** | Admission/auth/commit/WAL/recovery paths must return typed errors; panic/unwrap forbidden in runtime critical path logic. | Static scans for `panic!`, `unwrap`, `expect`, `assert!` in critical modules + regression gate in `crates/andromeda-exec/tests/core_io_gates.rs`. | Runtime paths use `AndromedaResult` and typed error propagation; test-only assertions remain under `#[cfg(test)]`. | Keep fail-stop behavior via error returns; enforce code review policy on critical modules. |
| **G6 Silent corruption** | Cold truth is immutable post-publication; mutation attempts on published cold segments must fail deterministically with traceable error. | `crates/andromeda-storage/tests/core_io_gates.rs`, `crates/andromeda-storage/tests/layout_publication_contract.rs`, `crates/andromeda-storage/tests/wal_scan_recovery_contract.rs`. | Implemented by `PublishedColdSegment` and placement guards that prohibit cold mutation. | Compile-time type boundary + runtime rejection path; corruption boundary traces supported for recovery stop conditions. |
| **G7 Non-reproducible crash** | Crash/restart must converge to one deterministic durable state for same WAL/manifest inputs. | `crates/andromeda-storage/tests/crash_recovery_impl.rs`, `crates/andromeda-storage/tests/property_recovery_replay.rs`, `crates/andromeda-storage/tests/disk_manager_durability_crash_safety.rs`. | Recovery planning and redo decisions are deterministic over sorted durable WAL inputs and manifest anchors. | Fixed replay ordering by LSN, typed boundary kinds, explicit skip reasons for incomplete/rolled-back transactions. |

---

## 2) Audit trace family completeness lock

### Family contract

release gate cycle locks the following eight audit families:

1. `SecurityAuditTrace`
2. `CatalogChangeTrace` (runtime type: `CatalogMutationTrace`)
3. `AdminOperationTrace`
4. `ProcedureInvocationTrace` (runtime type family: invocation/transition traces)
5. `TransactionTrace`
6. `PlanDecisionTrace` (runtime type family: `DecisionTrace` with plan-decision kind)
7. `RecoveryTrace`
8. `ClusterEventTrace` (HA/DR decision/audit family)

### Completeness matrix

| Family | Fields locked | Schema version | Cardinality | Retention policy | Query semantics |
|---|---|---|---|---|---|
| SecurityAuditTrace | trace_id, surface, certificate, principal, permission, outcome, reason | `EventSchemaVersion::V0` required | One per auth decision (allow and deny) | `SecurityPolicy`/`ForensicHold` as needed | `TraceQueryFilter` by family, principal, trace_id, catalog version |
| CatalogChangeTrace | trace_id, catalog_version, object_id, action | V0 envelope validation path | One per catalog mutation step | `CatalogVersion` window | Query by family + catalog version + object correlation |
| AdminOperationTrace | trace_id, surface, certificate, principal, operation, permission, accepted, reason | `EventSchemaVersion::V0` required | One per admin operation decision | `CatalogVersion` or security retention | Query by admin family + principal + operation evidence |
| ProcedureInvocationTrace | trace_id, invocation_id, phase/status/reason, transaction correlation | V0 typed invocation traces | One admission + dispatch + terminal event per invocation path | `WalSegment` minimum, escalates for forensic hold | Query by trace_id/invocation correlation; bounded result sets |
| TransactionTrace | tx_id, from/to state, reason code, durable LSN evidence | V0 transition schema | One per state transition event | `WalSegment`/`CatalogVersion` depending boundary | Query by tx-id correlation and family |
| PlanDecisionTrace | trace_id, decision kind, reason, plan identity evidence | V0 decision schema | One per plan decision boundary | `CatalogVersion` for plan forensic chain | Query by decision family and procedure/catalog filters |
| RecoveryTrace | trace_id, last_durable_lsn, corruption_boundary_lsn | V0 recovery schema | One per recovery start/boundary milestone | `ForensicHold` preferred for incidents | Query by LSN range and recovery family |
| ClusterEventTrace | trace_id, quorum/promotion/fencing decision fields | V0 envelope + durable audit mapping | One per HA/DR critical decision | `ForensicHold` or policy horizon | Query by family and cluster decision scope |

### Audit omission proof obligation

Audit omission gate passes only if all conditions hold:

1. `EventEnvelope::validate` rejects structurally incomplete traces.
2. Security/admin traces require identity evidence and supported schema version.
3. Durable audit sink success implies appended + flushed + replayable evidence.
4. Failure kinds (`ValidationRejected`, `WalAppendRejected`, `WalFlushRejected`, `CorruptionDetected`, `PermissionDenied`, `RetentionRejected`) are fail-closed for critical families.
5. No global audit-disable switch is permitted by trait boundary and doctrine.

---

## 3) Transaction state machine final proof

### Transition lock

Locked states:

- `Created → Active → Committing → Committed → Disposed`
- `Created → Active → Failed → RollingBack → RolledBack → Disposed`
- `Active → Poisoned → RollingBack → RolledBack → Disposed`

Locked transition controls:

- `Committed` requires non-zero durable commit LSN.
- `RolledBack` requires non-zero durable rollback LSN.
- Illegal transitions return typed transaction errors.

### MVCC visibility lock

The release gate cycle visibility formula is locked as:

`visible(row, ts) = row.BeginTs ≤ ts ∧ (row.EndTs = INF ∨ row.EndTs > ts)`

Andromeda runtime enforcement additionally requires durable terminal status evidence before exposing committed writers to peer snapshots.

### Safety properties

1. No partial-commit visible state.
2. No fabricated committed visibility during recovery.
3. Deterministic post-recovery visibility for identical snapshot + status inputs.

---

## 4) WAL durability fence final specification

### Core invariant

No mutation becomes externally visible until WAL durability evidence crosses the required LSN fence.

### Fence classification

| Fence class | Definition | Examples |
|---|---|---|
| Persistent (required redo) | Record kinds with `is_redo_relevant() == true`; recovery replay depends on durable presence. | `PageAllocate`, `PageFormat`, `RowInsert`, `RowUpdate`, `RowDelete`, `IndexInsert`, `IndexDelete`, `MvccVersionCreate`, `MvccVersionClose`, `MapDeltaAppend`, `ManifestSwitch`, `CatalogChangeApply`, `CatalogChangeCommit`, `SecurityAuditAppend`, `BTreeInsert/Delete/Split/Merge` |
| Non-persistent (optional redo / structural only) | Transaction markers and structural boundaries that do not apply business redo directly. | `TxBegin`, `TxCommit`, `TxRollback`, `CheckpointBegin/End`, `SnapshotBegin/End`, `CatalogChangeBegin` |

B-Tree records are redo-relevant but remain deferred behind rebuild-evidence and fail-closed gates; they are not non-redo placeholders.

### Crash semantics lock

- If durable commit LSN exists for transaction terminal path, commit is recoverably visible.
- If durable commit LSN does not exist, transaction effects remain non-visible and are skipped/rolled back by recovery plan.

---

## 5) ContractHash validation gate

### Locked contract

Before transaction creation, invocation validates:

`ContractHash = digest(canonical_procedure_repr + input_shape + output_shape + required_permissions)`

Runtime gate behavior:

1. Validate invocation identity (non-zero invocation id, non-zero expected hash, non-zero catalog version).
2. Validate executable contract identity.
3. Reject any hash/catalog mismatch before dispatch and before WAL append.
4. Emit typed rejection (`CompletionStatus::ContractRejected`) with trace evidence.

### Implementation evidence

- `crates/andromeda-exec/src/services/admission.rs`
- `crates/andromeda-exec/src/services/pre_transaction.rs`
- `crates/andromeda-exec/src/invocation.rs`
- `crates/andromeda-exec/src/registry.rs`

### Verification evidence

- `crates/andromeda-exec/tests/core_io_gates.rs` (`pre_transaction_contract_rejection_blocks_runtime_io_admission_evidence`)
- `crates/andromeda-exec/tests/v0_vertical_e2e.rs` (`v0_inventory_rejects_contract_mismatch_before_wal_append`)
- `crates/andromeda-exec/tests/integration_execution_path.rs` (contract mismatch rejection pre-transaction)

---

## 6) Panic-free critical-path verification

Critical-path runtime modules (admission, pre-transaction, WAL durability fence, recovery startup/replay, transaction state machine) are locked to typed error returns (`AndromedaResult`) rather than panics.

Verification policy:

1. Scan critical runtime modules for `panic!`, `unwrap()`, `expect()`, `assert!`.
2. Treat test blocks (`#[cfg(test)]`) as non-blocking for doctrine gate.
3. Reject runtime additions of panic/unwrap in critical path during review.

Gate disposition: **PASS with enforcement policy** (runtime path typed-error model retained; test assertions remain scoped to tests).

---

## 7) Cold storage immutability audit

### Compile-time boundary

`PublishedColdSegment` requires `SegmentState::PublishedCold` and exposes mutation rejection path.

### Runtime boundary

`reject_mutation(...)` always fails for published cold segments with storage error:

`PublishedColdSegment rejects all mutations; ColdStore is immutable after publication`

### Verification evidence

- `crates/andromeda-storage/src/cold_store.rs`
- `crates/andromeda-storage/tests/core_io_gates.rs`
- `crates/andromeda-storage/tests/layout_publication_contract.rs`
- `crates/andromeda-storage/tests/wal_scan_recovery_contract.rs`

---

## 8) Recovery invariant verification matrix (deterministic convergence)

The following matrix locks deterministic crash/recovery convergence and covers pre/post-WAL boundaries, manifest/ checkpoint boundaries, and replay selection.

| Test ID | File | Scenario | Expected invariant |
|---|---|---|---|
| RCV-01 | `crates/andromeda-storage/tests/crash_recovery_impl.rs` | CBF-01 TxBegin only | `SkipNonRedoRecord`, no fabricated commit |
| RCV-02 | `crates/andromeda-storage/tests/crash_recovery_impl.rs` | CBF-02 RowInsert no commit | `SkipIncompleteTransaction` |
| RCV-03 | `crates/andromeda-storage/tests/crash_recovery_impl.rs` | CBF-03 RowUpdate no commit | `SkipIncompleteTransaction` |
| RCV-04 | `crates/andromeda-storage/tests/crash_recovery_impl.rs` | CBF-04 RowDelete no commit | `SkipIncompleteTransaction` |
| RCV-05 | `crates/andromeda-storage/tests/crash_recovery_impl.rs` | CBF-05 IndexInsert no commit | `SkipIncompleteTransaction` |
| RCV-06 | `crates/andromeda-storage/tests/crash_recovery_impl.rs` | CBF-06 IndexDelete no commit | `SkipIncompleteTransaction` |
| RCV-07 | `crates/andromeda-storage/tests/crash_recovery_impl.rs` | CBF-07 MVCC create no commit | `SkipIncompleteTransaction` |
| RCV-08 | `crates/andromeda-storage/tests/crash_recovery_impl.rs` | CBF-08 rollback path | `SkipRolledBackTransaction` |
| RCV-09 | `crates/andromeda-storage/tests/crash_recovery_impl.rs` | CAC committed mutation | `Replay` |
| RCV-10 | `crates/andromeda-storage/tests/crash_recovery_impl.rs` | CAC mixed txs | committed replay only |
| RCV-11 | `crates/andromeda-storage/tests/wal_scan_recovery_contract.rs` | WAL scan replay boundaries | deterministic LSN ordering |
| RCV-12 | `crates/andromeda-storage/tests/wal_durability_fence_contract.rs` | page flush fence | page LSN must be durable |
| RCV-13 | `crates/andromeda-storage/tests/wal_durability_fence_contract.rs` | manifest switch fence | manifest checkpoint bounded by WAL checkpoint |
| RCV-14 | `crates/andromeda-storage/tests/wal_durability_fence_contract.rs` | recovery floor | floor cannot precede required WAL start |
| RCV-15 | `crates/andromeda-storage/tests/property_recovery_replay.rs` | replay property tests | deterministic replay classification |
| RCV-16 | `crates/andromeda-storage/tests/disk_manager_durability_crash_safety.rs` | disk/WAL crash-safety integration | durable state survives crash |
| RCV-17 | `crates/andromeda-storage/tests/crash_recovery_impl.rs` | CBF-15..18 B-Tree mutation without commit | `SkipIncompleteTransaction` |
| RCV-18 | `crates/andromeda-storage/tests/crash_recovery_impl.rs` | committed `MvccVersionClose` | replay classification for committed MVCC close |
| RCV-19 | `crates/andromeda-storage/tests/crash_recovery_impl.rs` | non-transactional `PageAllocate` and `PageFormat` | explicit deferred replay gate; no inferred page payload apply |
| RCV-20 | `crates/andromeda-storage/tests/crash_recovery_impl.rs` | non-transactional `MapDeltaAppend` | explicit deferred replay gate |
| RCV-21 | `crates/andromeda-storage/tests/recovery_completeness_contract.rs` | 26-kind recovery classification and promotion gates | every deferred kind requires payload codec, golden vectors, property or fuzz coverage, and crash/recovery gate before promotion |

Determinism property lock:

`restart(crash + recovery) == restart(crash + recovery)` for identical manifest + WAL inputs.

---

## 9) Visibility violation prevention tests

| Test ID | File | Scenario | Property |
|---|---|---|---|
| VIS-01 | `crates/andromeda-exec/tests/recovery_visibility_gates.rs` | crash before durable commit | writer remains invisible post-recovery |
| VIS-02 | `crates/andromeda-exec/tests/recovery_visibility_gates.rs` | crash after durable commit | writer visible and replayable |
| VIS-03 | `crates/andromeda-tx/tests/commit_log_durability.rs` | commit durability boundary | visible only after durable WAL |
| VIS-04 | `crates/andromeda-tx/tests/commit_log_gates_final.rs` | five-step commit sequence | status + visibility ordered after flush |
| VIS-05 | `crates/andromeda-tx/tests/mvcc_gc_durability_contract.rs` | MVCC visibility over snapshots | deterministic visibility with durable statuses |
| VIS-06 | `crates/andromeda-tx/tests/mvcc_eligibility_contract.rs` | version lifecycle | no illegal visible/deleted ambiguity |
| VIS-07 | `crates/andromeda-exec/tests/v0_vertical_e2e.rs` | end-to-end execution path | contract + WAL + visibility chain consistent |

Visibility gate lock:

No row may be visible in TX1 and invisible in TX2 unless TX1 committed durably before TX2 snapshot boundary and isolation policy permits that visibility.

---

## Validation plan

Required gate evidence commands (workspace baseline):

```powershell
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
```

Targeted gate suites:

```powershell
cargo test -p andromeda-exec --test recovery_visibility_gates -- --nocapture
cargo test -p andromeda-storage --test crash_recovery_impl -- --nocapture
cargo test -p andromeda-storage --test wal_durability_fence_contract -- --nocapture
cargo test -p andromeda-observe --test audit_family_contract -- --nocapture
```

---

## Rollback or disablement plan

If any gate fails:

1. Freeze release branch.
2. Record failing gate and owner in release checklist.
3. Apply fix only through typed contracts (no emergency bypass flag).
4. Re-run targeted suite, then workspace gates.
5. Re-approve only after all seven gates return PASS.

---

## Related sources

- `documentations/governance/decisions/DEC-026-release-gates-and-deferral-policy.md`
- `documentations/governance/decisions/DEC-033-durable-audit-ledger.md`
- `crates/andromeda-exec/src/services/admission.rs`
- `crates/andromeda-exec/src/services/pre_transaction.rs`
- `crates/andromeda-storage/src/write_ahead_log/durability_fence.rs`
- `crates/andromeda-storage/src/cold_store.rs`
- `crates/andromeda-tx/src/state.rs`
- `crates/andromeda-observe/src/events.rs`
- `crates/andromeda-observe/src/events/durable_audit.rs`
