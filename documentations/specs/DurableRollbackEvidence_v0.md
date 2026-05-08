# DurableRollbackEvidence v0 Specification

## Purpose

Define the accepted documentation contract for `DurableRollbackEvidence v0`,
the bounded evidence object that proves a transaction rollback terminal record
is covered by durable WAL and that the transaction reached `RolledBack`.

`DurableRollbackEvidence v0` is rollback-completion evidence. It proves that
the transaction's prepared or partial mutations must not become visible. It is
not database truth by itself, not an undo script, and not a replacement for
manifest plus durable WAL recovery.

## Scope

This specification applies to transaction rollback completion, rollback-log
reconstruction, recovery replay, failed Procedure completion, MVCC visibility
blocking, lock cleanup, and observability projections that claim a transaction
reached `RolledBack`.

It covers:

- required rollback evidence fields;
- the durable source of each field;
- validation before rollback completion is accepted;
- replay reconstruction from WAL and manifest evidence;
- the relationship between rollback LSN, durable LSN, recovery frontier, and
  transaction state;
- evidence projection into traces, audit, and result metadata;
- values and artifacts that are not durable rollback evidence.

## Current Implementation Status

The Rust workspace already contains executable rollback-durability guardrails:

- `TransactionStateMachine` moves `RollingBack` to `RolledBack` only after a
  nonzero durable rollback LSN is recorded.
- `RollbackLogEntry` stores `tx_id`, `rollback_lsn`, `durable_lsn`, timestamp,
  and parameter hash, and rejects durable evidence that does not cover the
  rollback record LSN.
- WAL records carry `lsn`, `previous_lsn`, `transaction_id`, record checksum,
  header checksum, and a `TxRollback` record kind.
- Recovery planning classifies durable transactions as `Committed`,
  `RolledBack`, `Open`, or `Incomplete` and skips redo records for rolled-back
  transactions.
- Execution and observability code can project rollback metadata and
  `RollbackDurable` traces with a durable rollback LSN.

This specification defines the cross-crate documentation contract. It does not
claim that every runtime packet already serializes one concrete
`DurableRollbackEvidence` structure.

## Non-goals

This specification does not:

- define the WAL frame byte format;
- define page, heap, B-Tree, manifest, audit, or RPC binary layouts;
- authorize undo, redo, repair, or manifest publication without recovery
  validation;
- make rollback intent, RAM state, dirty pages, cache entries, audit records,
  trace records, benchmark output, GPU output, or temporary files database
  truth;
- allow rollback handling to bypass typed Procedure contracts or transaction
  admission;
- introduce application-facing ad hoc SQL, gRPC, runtime JSON defaults, or
  untyped command payloads;
- expose Administration, recovery, backup, restore, or HA/DR behavior through
  the Application surface.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `AGENTS.md` for the durable WAL and rollback/recovery invariants.
- `documentations/specs/WalRecord_v0.md` for WAL frame fields, `TxRollback`,
  LSN chain validation, checksums, and durable-prefix scan behavior.
- `documentations/specs/DatabaseManifest_v0.md` for snapshot and
  `required_wal_start_lsn` recovery roots.
- `documentations/specs/RecoveryReport_v0.md` for startup modes, WAL
  boundaries, replay evidence, incomplete transaction handling, and forensic
  behavior.
- `documentations/specs/AuditLedger_v0.md` for audit evidence boundaries.
- `crates/andromeda-tx/src/state.rs` for transaction state-machine durable
  rollback rules.
- `crates/andromeda-tx/src/commit_log/rollback.rs` and
  `crates/andromeda-tx/src/commit_log/replay.rs` for rollback-log fields and
  replay records.
- `crates/andromeda-storage/src/recovery/planning.rs` and
  `crates/andromeda-storage/src/recovery/startup/evidence.rs` for recovery
  frontier and durable-WAL coverage behavior.

## Procedure

### Source

`DurableRollbackEvidence v0` must be derived from durable sources only.

| Source | Required contribution |
| --- | --- |
| WAL frame codec and scan | Valid `TxRollback` record bytes, `rollback_record_lsn`, `previous_lsn`, record checksum, header checksum, transaction id, and durable-prefix boundary. |
| WAL durability owner | Proof that the durable WAL prefix covers `rollback_record_lsn`; the accepted value is `durable_rollback_lsn`. |
| Transaction state machine | Prior state `RollingBack`, terminal state `RolledBack`, and the nonzero durable rollback LSN recorded before rollback completion. |
| Rollback log | Transaction identity, rollback record LSN, durable rollback LSN, rollback timestamp, and bounded rollback parameter hash when available. |
| Database manifest and recovery planner | Snapshot identity, `required_wal_start_lsn`, recovery frontier, transaction classification, and storage-format validation before redo. |
| Observability or audit sink | Trace ids, event ids, and sanitized correlation only after the durable evidence has been accepted. |

The durable source of truth is the validated WAL prefix. A caller must not
populate `durable_rollback_lsn` from RAM-only abort flags, client cancellation,
temporary files, unvalidated file headers, audit records, traces, or result
metadata.

### Required fields

`DurableRollbackEvidence v0` is a bounded, typed object. Field names in code may
vary, but the following semantics are required.

| Field | Required rule |
| --- | --- |
| `schema_version` | Must identify this contract as `DurableRollbackEvidence/v0`. |
| `source_mode` | Must identify whether the evidence was produced by live rollback completion, recovery replay, restore validation, or forensic inspection. |
| `transaction_id` | Must be nonzero and must match the WAL `TxRollback` record transaction id. |
| `terminal_state` | Must be exactly `RolledBack`. |
| `previous_transaction_state` | Required for live rollback completion and must be `RollingBack`. Recovery reconstruction must not fabricate a prior runtime state when only WAL evidence is available. |
| `begin_lsn` | Must identify the transaction `TxBegin` record when the transaction is reconstructed from the replay range. |
| `last_mutation_lsn` | Highest known redo-relevant WAL record for the transaction before the rollback record. May be absent only when the transaction has no durable mutation record in the replay range. |
| `rollback_record_lsn` | Must be the nonzero LSN of the `TxRollback` record. |
| `durable_rollback_lsn` | Must be nonzero and greater than or equal to `rollback_record_lsn`. It records the durable WAL prefix accepted when rollback completion became durable. |
| `durable_prefix_lsn` | Must equal the durable WAL prefix boundary used to validate this evidence. It may be the same value as `durable_rollback_lsn`. |
| `required_wal_start_lsn` | Must come from the accepted `DatabaseManifest v0` recovery root when the evidence is reconstructed during startup. |
| `recovery_frontier_lsn` | Highest LSN accepted by recovery for normal visibility decisions. During live rollback it may be absent until the next recovery report exists. |
| `wal_segment_id` | WAL segment or mono-segment identity that contains `rollback_record_lsn`; current mono-segment evidence uses segment id `1`. |
| `wal_format_version` | WAL frame format version used to decode the rollback record. |
| `previous_lsn` | The predecessor LSN encoded in the rollback record, or the base predecessor evidence for the first record in a durable prefix. |
| `record_checksum` | WAL record checksum from the decoded `TxRollback` frame. |
| `header_checksum` | WAL frame header checksum from the decoded `TxRollback` frame. |
| `chain_validation` | Must state that LSN order, `previous_lsn`, length, and checksum checks passed through `rollback_record_lsn`. |
| `rollback_reason_code` | Bounded reason classification when known, such as business rejection, runtime failure, poison handling, or explicit rollback request. |
| `rollback_parameter_hash` | Bounded hash of rollback parameters when the rollback log carries one. It must not contain raw parameters or secret-bearing values. |
| `source_component` | Must name the component that accepted the evidence, such as `andromeda-tx`, `andromeda-wal`, `andromeda-storage`, or a recovery service. |
| `source_attempt_id` | Must identify the rollback, startup, restore, or replay attempt when such an attempt id exists. |
| `generated_at` | Bounded timestamp or engine timestamp for evidence emission. It is not a durability source. |

### Optional projection fields

The following fields may be included when available. They support correlation
and explainability, but they do not replace WAL evidence.

| Field | Use |
| --- | --- |
| `procedure_contract_binding` | Binds rollback or failure handling to the typed Procedure contract that admitted the work. |
| `invocation_id`, `request_id`, `session_id` | Correlate failed completion metadata and traces. |
| `rollback_trace_id` | Links to a `RollbackDurable` or transaction transition trace. |
| `audit_event_id` | Links to durable audit evidence. |
| `manifest_version`, `snapshot_id` | Bind recovered evidence to the manifest and snapshot used by startup. |
| `storage_format_fingerprints` | Names the storage subformats validated before recovery classifies redo decisions. |

### Validation

Validation must fail closed. Rollback completion may be accepted only after all
applicable checks pass.

| Check | Required outcome |
| --- | --- |
| Transaction identity | `transaction_id` is nonzero and matches every WAL, rollback-log, state-machine, and projection field. |
| State transition | Live evidence moves only from `RollingBack` to `RolledBack`; recovery evidence reconstructs only terminal `RolledBack` status from durable WAL and must not claim an observed prior runtime state. |
| Record kind | `rollback_record_lsn` resolves to a `TxRollback` WAL record for the transaction. |
| LSN nonzero | `rollback_record_lsn`, `durable_rollback_lsn`, and `durable_prefix_lsn` are nonzero. |
| Durability coverage | `durable_rollback_lsn >= rollback_record_lsn`; evidence below the rollback record is rejected. |
| WAL prefix | The validated durable prefix includes the rollback record and all earlier accepted records required for the transaction summary. |
| WAL chain | LSN order, `previous_lsn`, length, record checksum, and header checksum validate through `rollback_record_lsn`. |
| Transaction summary | The transaction has a `TxBegin`, exactly one `TxRollback`, no `TxCommit`, and no conflicting terminal record in the accepted replay range. |
| Visibility blocking | Redo-relevant mutation records for the rolled-back transaction must be skipped and must not become MVCC-visible. |
| Recovery frontier | Reconstructed evidence is valid only when `rollback_record_lsn <= recovery_frontier_lsn` and the frontier covers `required_wal_start_lsn`. |
| Storage format gate | Recovery must validate required storage format fingerprints before classifying redo records. |
| Projection boundary | Result metadata, traces, audit, and Procedure Store records may carry the durable rollback LSN only after rollback evidence validates. |

If the WAL scan stops at a truncation or corrupt tail after
`rollback_record_lsn`, the rollback evidence may remain valid for that
transaction only when the accepted durable prefix still contains a complete,
checksum-valid transaction summary and recovery policy accepts the boundary. If
the scan stops on an LSN gap, duplicate or reordered LSN, or `previous_lsn`
mismatch before or at the rollback record, normal rollback evidence must be
rejected and startup must enter forensic handling or refuse.

### Replay reconstruction

Recovery reconstructs `DurableRollbackEvidence v0` from durable artifacts in
this order:

1. Decode and validate the active `DatabaseManifest v0`.
2. Read the manifest `required_wal_start_lsn` and selected snapshot identity.
3. Scan WAL bytes from the required start until the last complete valid record
   or the first fail-closed boundary.
4. Validate WAL frame checksums, chain continuity, and storage format
   fingerprints before redo classification.
5. Group durable WAL records by `transaction_id`.
6. For each group, require `TxBegin` and exactly one `TxRollback`, reject a
   matching `TxCommit`, and record `rollback_record_lsn`.
7. Set `durable_prefix_lsn` and `recovery_frontier_lsn` to the accepted durable
   recovery boundary for normal startup.
8. Accept `DurableRollbackEvidence v0` only when
   `recovery_frontier_lsn >= rollback_record_lsn`.
9. Mark the transaction `RolledBack` in the reconstructed transaction status
   table.
10. Skip redo-relevant records whose transaction state is `RolledBack`.

Incomplete transactions, conflicting terminal records, terminal records beyond
the durable frontier, and records past a forensic chain break do not produce
rollback evidence. Their mutations must remain invisible. Recovery may classify
them as incomplete and discard their replay effects, but that classification is
not the same as durable rollback evidence unless a valid `TxRollback` record is
inside the accepted durable prefix.

### Relationship to WAL LSN, recovery frontier, and transaction state

The following fields must stay distinct.

| Concept | Meaning | Visibility rule |
| --- | --- | --- |
| `rollback_record_lsn` | Exact LSN of the transaction `TxRollback` record. | Necessary but not sufficient for rollback completion evidence. |
| `durable_rollback_lsn` | Durable WAL prefix reported by the WAL owner when rollback evidence was accepted. | Must be nonzero and cover `rollback_record_lsn`. |
| `durable_prefix_lsn` | Highest LSN in the accepted checksum-valid WAL prefix used by this evidence. | Must not be lower than `rollback_record_lsn`. |
| `required_wal_start_lsn` | Manifest recovery anchor for replay. | Recovery evidence must be reconstructed from a prefix that covers this anchor. |
| `recovery_frontier_lsn` | Highest LSN recovery accepts for normal visibility decisions after manifest and WAL validation. | Recovered rollback evidence is valid only at or before this frontier. |
| `RollingBack` | Runtime state after rollback is requested and before durable WAL coverage is proven. | Transaction mutations remain invisible and terminal cleanup is not yet durable. |
| `RolledBack` | Runtime or recovered state after durable rollback evidence validates. | MVCC must reject the transaction's mutations, and cleanup may proceed according to policy. |
| `Disposed` | Cleanup state after terminal handling. | Does not add durability evidence and must not erase retained rollback evidence. |

## What is not evidence

The following artifacts must not be treated as `DurableRollbackEvidence v0`:

- a rollback request, business rejection, error code, timeout, cancellation, or
  poison marker without a durable `TxRollback` record;
- a `TxRollback` record allocated in memory but not covered by the durable WAL
  prefix;
- a nonzero rollback LSN without a durable prefix that covers it;
- an in-memory `TransactionStatus::RolledBack` flag;
- a lock release, savepoint rollback, compensation action, cleanup record, or
  undo decision by itself;
- a dirty page flush, BufferPool state, cache entry, HotStore state, or
  temporary file;
- a WAL file header value that has not been cross-checked by a checksum-valid
  scan;
- a client acknowledgment, RPC completion frame, ResultStream terminal message,
  audit event, trace event, or Procedure Store record by itself;
- benchmark output, scenario evidence, GPU output, statistics, learned-model
  output, or analytical Map output;
- an operator statement, log line, or manually edited report;
- commit evidence for the same or another transaction.

## Validation

Documentation acceptance checks:

- The spec names the durable WAL prefix as the source of rollback evidence.
- The spec requires `durable_rollback_lsn >= rollback_record_lsn`.
- The spec keeps `rollback_record_lsn`, `durable_rollback_lsn`,
  `durable_prefix_lsn`, and `recovery_frontier_lsn` distinct.
- The spec states that rollback evidence blocks MVCC visibility for that
  transaction's mutations.
- The spec requires replay reconstruction from manifest plus durable WAL.
- The spec states that audit, trace, result metadata, RAM, temporary storage,
  benchmark output, GPU output, rollback intent, and lock cleanup are not
  rollback evidence.
- The spec does not introduce ad hoc SQL, gRPC, runtime JSON defaults,
  Application-surface administration, or native Rust struct serialization.

Future implementation work should keep or add targeted validation for:

```powershell
cargo test -p andromeda-tx --test v0_transaction_lifecycle
cargo test -p andromeda-tx --test tx_wal_replay_recovery
cargo test -p andromeda-tx --test commit_log_durability
cargo test -p andromeda-storage --test wal_scan_recovery_contract
cargo test -p andromeda-storage --test file_wal_recovery_contract
cargo test -p andromeda-exec --test c5_commit_rollback_lifecycle
cargo test -p andromeda-exec --test inventory_runtime_e2e_gates
```

### Validation matrix

| Scenario | Expected result |
| --- | --- |
| `TxRollback` LSN is covered by durable prefix and transaction has no conflicting terminal record. | Evidence validates and transaction may become `RolledBack`. |
| `durable_rollback_lsn < rollback_record_lsn`. | Reject evidence and keep transaction `RollingBack` or recovery-incomplete. |
| `durable_rollback_lsn == 0`. | Reject evidence. |
| Rollback record exists only after a truncated or corrupt tail boundary. | Reject evidence; transaction remains incomplete or forensic-only. |
| WAL chain gap, duplicate, reorder, or `previous_lsn` mismatch occurs before the rollback record. | Reject normal recovery and require forensic handling or refusal. |
| Both `TxCommit` and `TxRollback` exist for one transaction in the accepted prefix. | Reject conflicting terminal evidence. |
| Audit or trace says rollback durable but WAL evidence is missing. | Reject the projection as non-authoritative. |
| Recovery frontier is below the rollback record LSN. | Do not reconstruct rollback evidence. |
| A rolled-back transaction has redo-relevant mutation records. | Skip those records and keep the mutations invisible. |

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| A transaction appears `RolledBack` with no durable LSN. | Status was projected from RAM, an error path, or forged metadata. | Reject the status and require durable WAL reconstruction. |
| Rollback evidence uses the same field for record LSN and frontier LSN. | Rollback position and recovery boundary were conflated. | Split `rollback_record_lsn`, `durable_rollback_lsn`, and `recovery_frontier_lsn`. |
| Recovery replays a mutation for a rolled-back transaction. | Redo classification ignored terminal rollback evidence. | Skip the record and keep the transaction invisible. |
| Audit replay completes rollback. | Audit was treated as storage truth. | Use audit only as forensic correlation and require WAL-backed evidence. |
| Error handling reports rollback durable after client cancellation only. | Rollback intent was confused with durable terminal evidence. | Require a valid `TxRollback` record in the durable prefix. |
| Lock cleanup deletes all local state before evidence is retained. | Cleanup ran before durable rollback evidence was captured. | Retain the rollback evidence or reconstruct it from WAL before cleanup. |

## References

- `AGENTS.md`
- `documentations/CURRENT_STATE.md`
- `documentations/WORKER_EXECUTION_MATRIX_2026.md`
- `documentations/governance/adr-backlog-2026-05-08.md`
- `documentations/specs/AuditLedger_v0.md`
- `documentations/specs/DatabaseManifest_v0.md`
- `documentations/specs/RecoveryReport_v0.md`
- `documentations/specs/WalRecord_v0.md`
- `crates/andromeda-tx/src/state.rs`
- `crates/andromeda-tx/src/commit_log/rollback.rs`
- `crates/andromeda-tx/src/commit_log/replay.rs`
- `crates/andromeda-tx/src/commit_log/manager/validation.rs`
- `crates/andromeda-wal/src/write_ahead_log/transaction.rs`
- `crates/andromeda-storage/src/recovery/planning.rs`
- `crates/andromeda-storage/src/recovery/startup/evidence.rs`
- `crates/andromeda-exec/src/local/runtime/events.rs`
