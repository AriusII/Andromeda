# DurableCommitEvidence v0 Specification

## Purpose

Define the accepted documentation contract for `DurableCommitEvidence v0`, the
bounded evidence object that permits Andromeda to make a transaction commit
visible after the transaction commit record is covered by durable WAL.

`DurableCommitEvidence v0` is commit-visibility evidence. It is not database
truth by itself. Reconstructible database truth remains the latest valid cold
snapshot plus the durable WAL prefix accepted from that snapshot.

## Scope

This specification applies to transaction commit visibility, commit-log
reconstruction, recovery replay, Procedure completion metadata, domain adapter
publication, and observability projections that claim a transaction reached
`Committed`.

It covers:

- required commit evidence fields;
- the durable source of each field;
- validation before visible commit;
- replay reconstruction from WAL and manifest evidence;
- the relationship between commit LSN, durable LSN, recovery frontier, and
  transaction state;
- evidence projection into traces, audit, and result metadata;
- values and artifacts that are not durable commit evidence.

## Current Implementation Status

The Rust workspace already contains executable commit-durability guardrails:

- `TransactionStateMachine` moves `Committing` to `Committed` only after a
  nonzero durable commit LSN is recorded.
- `CommitLogEntry` stores `tx_id`, `commit_lsn`, `durable_lsn`, timestamp,
  affected row count, and isolation level, and rejects durable evidence that
  does not cover the commit record LSN.
- WAL records carry `lsn`, `previous_lsn`, `transaction_id`, record checksum,
  header checksum, and a `TxCommit` record kind.
- Recovery planning uses the validated durable WAL prefix, transaction
  summaries, and manifest `required_wal_start_lsn` before replay.
- Execution and observability code can project committed completion metadata
  and `CommitVisible` traces with a durable commit LSN.

This specification defines the cross-crate documentation contract. It does not
claim that every runtime packet already serializes one concrete
`DurableCommitEvidence` structure.

## Non-goals

This specification does not:

- define the WAL frame byte format;
- define page, heap, B-Tree, manifest, audit, or RPC binary layouts;
- authorize visible commit without durable WAL;
- make RAM state, dirty pages, cache entries, audit records, trace records,
  benchmark output, GPU output, or temporary files database truth;
- allow a commit to bypass typed Procedure contracts;
- introduce application-facing ad hoc SQL, gRPC, runtime JSON defaults, or
  untyped command payloads;
- expose Administration, recovery, backup, restore, or HA/DR behavior through
  the Application surface.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `AGENTS.md` for the WAL-before-visible-commit invariant and durable-format
  rules.
- `documentations/specs/WalRecord_v0.md` for WAL frame fields, `TxCommit`,
  LSN chain validation, checksums, and durable-prefix scan behavior.
- `documentations/specs/DatabaseManifest_v0.md` for snapshot and
  `required_wal_start_lsn` recovery roots.
- `documentations/specs/RecoveryReport_v0.md` for startup modes, WAL
  boundaries, replay evidence, and forensic behavior.
- `documentations/specs/AuditLedger_v0.md` for audit evidence boundaries.
- `crates/andromeda-tx/src/state.rs` for transaction state-machine durable
  commit rules.
- `crates/andromeda-tx/src/commit_log/entry.rs` and
  `crates/andromeda-tx/src/commit_log/replay.rs` for commit-log fields and
  replay records.
- `crates/andromeda-storage/src/recovery/planning.rs` and
  `crates/andromeda-storage/src/recovery/startup/evidence.rs` for recovery
  frontier and durable-WAL coverage behavior.

## Procedure

### Source

`DurableCommitEvidence v0` must be derived from durable sources only.

| Source | Required contribution |
| --- | --- |
| WAL frame codec and scan | Valid `TxCommit` record bytes, `commit_record_lsn`, `previous_lsn`, record checksum, header checksum, transaction id, and durable-prefix boundary. |
| WAL durability owner | Proof that the durable WAL prefix covers `commit_record_lsn`; the accepted value is `durable_commit_lsn`. |
| Transaction state machine | Prior state `Committing`, terminal state `Committed`, and the nonzero durable commit LSN recorded before visibility. |
| Commit log | Transaction identity, commit record LSN, durable commit LSN, commit timestamp, affected row count, and isolation level when available. |
| Database manifest and recovery planner | Snapshot identity, `required_wal_start_lsn`, recovery frontier, and storage-format validation before redo. |
| Observability or audit sink | Trace ids, event ids, and sanitized correlation only after the durable evidence has been accepted. |

The durable source of truth is the validated WAL prefix. A caller must not
populate `durable_commit_lsn` from RAM-only append state, client acknowledgments,
temporary files, unvalidated file headers, audit records, traces, or result
metadata.

### Required fields

`DurableCommitEvidence v0` is a bounded, typed object. Field names in code may
vary, but the following semantics are required.

| Field | Required rule |
| --- | --- |
| `schema_version` | Must identify this contract as `DurableCommitEvidence/v0`. |
| `source_mode` | Must identify whether the evidence was produced by live commit publication, recovery replay, restore validation, or forensic inspection. |
| `transaction_id` | Must be nonzero and must match the WAL `TxCommit` record transaction id. |
| `terminal_state` | Must be exactly `Committed`. |
| `previous_transaction_state` | Required for live publication and must be `Committing`. Recovery reconstruction must not fabricate a prior runtime state when only WAL evidence is available. |
| `begin_lsn` | Must identify the transaction `TxBegin` record when the transaction is reconstructed from the replay range. |
| `last_mutation_lsn` | Highest known redo-relevant WAL record for the transaction before the commit record. May be absent only when the transaction has no durable mutation record in the replay range. |
| `commit_record_lsn` | Must be the nonzero LSN of the `TxCommit` record. |
| `durable_commit_lsn` | Must be nonzero and greater than or equal to `commit_record_lsn`. It records the durable WAL prefix accepted when the commit became visible. |
| `durable_prefix_lsn` | Must equal the durable WAL prefix boundary used to validate this evidence. It may be the same value as `durable_commit_lsn`. |
| `required_wal_start_lsn` | Must come from the accepted `DatabaseManifest v0` recovery root when the evidence is reconstructed during startup. |
| `recovery_frontier_lsn` | Highest LSN accepted by recovery for normal visibility. During live commit it may be absent until the next recovery report exists. |
| `wal_segment_id` | WAL segment or mono-segment identity that contains `commit_record_lsn`; current mono-segment evidence uses segment id `1`. |
| `wal_format_version` | WAL frame format version used to decode the commit record. |
| `previous_lsn` | The predecessor LSN encoded in the commit record, or the base predecessor evidence for the first record in a durable prefix. |
| `record_checksum` | WAL record checksum from the decoded `TxCommit` frame. |
| `header_checksum` | WAL frame header checksum from the decoded `TxCommit` frame. |
| `chain_validation` | Must state that LSN order, `previous_lsn`, length, and checksum checks passed through `commit_record_lsn`. |
| `source_component` | Must name the component that accepted the evidence, such as `andromeda-tx`, `andromeda-wal`, `andromeda-storage`, or a recovery service. |
| `source_attempt_id` | Must identify the commit, startup, restore, or replay attempt when such an attempt id exists. |
| `generated_at` | Bounded timestamp or engine timestamp for evidence emission. It is not a durability source. |

### Optional projection fields

The following fields may be included when available. They support correlation
and explainability, but they do not replace WAL evidence.

| Field | Use |
| --- | --- |
| `procedure_contract_binding` | Binds the commit to the typed Procedure contract that admitted the work. |
| `invocation_id`, `request_id`, `session_id` | Correlate completion metadata and traces. |
| `commit_trace_id` | Links to a `CommitVisible` or transaction transition trace. |
| `audit_event_id` | Links to durable audit evidence. |
| `row_count_affected` | Records the bounded commit-log count when available. |
| `isolation_level` | Records the transaction isolation policy used by commit classification. |
| `manifest_version`, `snapshot_id` | Bind recovered evidence to the manifest and snapshot used by startup. |
| `storage_format_fingerprints` | Names the storage subformats validated before replay. |

### Validation

Validation must fail closed. A commit may become visible only after all
applicable checks pass.

| Check | Required outcome |
| --- | --- |
| Transaction identity | `transaction_id` is nonzero and matches every WAL, commit-log, state-machine, and projection field. |
| State transition | Live evidence moves only from `Committing` to `Committed`; recovery evidence reconstructs only terminal `Committed` status from durable WAL and must not claim an observed prior runtime state. |
| Record kind | `commit_record_lsn` resolves to a `TxCommit` WAL record for the transaction. |
| LSN nonzero | `commit_record_lsn`, `durable_commit_lsn`, and `durable_prefix_lsn` are nonzero. |
| Durability coverage | `durable_commit_lsn >= commit_record_lsn`; evidence below the commit record is rejected. |
| WAL prefix | The validated durable prefix includes the commit record and all earlier accepted records required for the transaction summary. |
| WAL chain | LSN order, `previous_lsn`, length, record checksum, and header checksum validate through `commit_record_lsn`. |
| Transaction summary | The transaction has a `TxBegin`, exactly one `TxCommit`, no `TxRollback`, and no conflicting terminal record in the accepted replay range. |
| Mutation ordering | Any redo-relevant mutation made visible by this evidence has an LSN lower than `commit_record_lsn` and is inside the same accepted durable prefix. |
| Recovery frontier | Reconstructed evidence is valid only when `commit_record_lsn <= recovery_frontier_lsn` and the frontier covers `required_wal_start_lsn`. |
| Storage format gate | Recovery must validate required storage format fingerprints before applying redo records. |
| Projection boundary | Result metadata, traces, audit, and Procedure Store records may carry the durable LSN only after commit evidence validates. |

If the WAL scan stops at a truncation or corrupt tail after
`commit_record_lsn`, the commit evidence may remain valid for that transaction
only when the accepted durable prefix still contains a complete, checksum-valid
transaction summary and recovery policy accepts the boundary. If the scan stops
on an LSN gap, duplicate or reordered LSN, or `previous_lsn` mismatch before or
at the commit record, normal commit evidence must be rejected and startup must
enter forensic handling or refuse.

### Replay reconstruction

Recovery reconstructs `DurableCommitEvidence v0` from durable artifacts in this
order:

1. Decode and validate the active `DatabaseManifest v0`.
2. Read the manifest `required_wal_start_lsn` and selected snapshot identity.
3. Scan WAL bytes from the required start until the last complete valid record
   or the first fail-closed boundary.
4. Validate WAL frame checksums, chain continuity, and storage format
   fingerprints before redo.
5. Group durable WAL records by `transaction_id`.
6. For each group, require `TxBegin` and exactly one `TxCommit`, reject a
   matching `TxRollback`, and record `commit_record_lsn`.
7. Set `durable_prefix_lsn` and `recovery_frontier_lsn` to the accepted durable
   recovery boundary for normal startup.
8. Accept `DurableCommitEvidence v0` only when
   `recovery_frontier_lsn >= commit_record_lsn`.
9. Mark the transaction `Committed` in the reconstructed transaction status
   table.
10. Replay only redo-relevant records whose transaction state is `Committed`.

Incomplete transactions, conflicting terminal records, terminal records beyond
the durable frontier, and records past a forensic chain break do not produce
commit evidence. They must remain invisible to MVCC readers and application
results.

### Relationship to WAL LSN, recovery frontier, and transaction state

The following fields must stay distinct.

| Concept | Meaning | Visibility rule |
| --- | --- | --- |
| `commit_record_lsn` | Exact LSN of the transaction `TxCommit` record. | Necessary but not sufficient for visibility. |
| `durable_commit_lsn` | Durable WAL prefix reported by the WAL owner when commit evidence was accepted. | Must be nonzero and cover `commit_record_lsn`. |
| `durable_prefix_lsn` | Highest LSN in the accepted checksum-valid WAL prefix used by this evidence. | Must not be lower than `commit_record_lsn`. |
| `required_wal_start_lsn` | Manifest recovery anchor for replay. | Recovery evidence must be reconstructed from a prefix that covers this anchor. |
| `recovery_frontier_lsn` | Highest LSN recovery accepts for normal visibility after manifest and WAL validation. | Recovered commit evidence is valid only at or before this frontier. |
| `Committing` | Runtime state after commit is requested and before durable WAL coverage is proven. | Prepared mutations remain invisible. |
| `Committed` | Runtime or recovered state after durable commit evidence validates. | MVCC and Procedure completion may expose the transaction according to policy. |
| `Disposed` | Cleanup state after terminal handling. | Does not add durability evidence and must not erase retained commit evidence. |

## What is not evidence

The following artifacts must not be treated as `DurableCommitEvidence v0`:

- a `TxCommit` record allocated in memory but not covered by the durable WAL
  prefix;
- a nonzero commit LSN without a durable prefix that covers it;
- an in-memory `TransactionStatus::Committed` flag;
- a dirty page flush, BufferPool state, cache entry, HotStore state, or
  temporary file;
- a WAL file header value that has not been cross-checked by a checksum-valid
  scan;
- a client acknowledgment, RPC completion frame, ResultStream terminal message,
  audit event, trace event, or Procedure Store record by itself;
- benchmark output, scenario evidence, GPU output, statistics, learned-model
  output, or analytical Map output;
- an operator statement, log line, or manually edited report;
- a rollback, compensation, or repair record for a different transaction.

## Validation

Documentation acceptance checks:

- The spec names the durable WAL prefix as the source of commit evidence.
- The spec requires `durable_commit_lsn >= commit_record_lsn`.
- The spec keeps `commit_record_lsn`, `durable_commit_lsn`,
  `durable_prefix_lsn`, and `recovery_frontier_lsn` distinct.
- The spec states that prepared mutations remain invisible while the
  transaction is `Committing`.
- The spec requires replay reconstruction from manifest plus durable WAL.
- The spec states that audit, trace, result metadata, RAM, temporary storage,
  benchmark output, and GPU output are not commit evidence.
- The spec does not introduce ad hoc SQL, gRPC, runtime JSON defaults,
  Application-surface administration, or native Rust struct serialization.

Future implementation work should keep or add targeted validation for:

```powershell
cargo test -p andromeda-tx --test v0_transaction_lifecycle
cargo test -p andromeda-tx --test tx_wal_replay_recovery
cargo test -p andromeda-tx --test commit_log_durability
cargo test -p andromeda-storage --test wal_scan_recovery_contract
cargo test -p andromeda-storage --test file_wal_recovery_contract
cargo test -p andromeda-exec --test v0_vertical_e2e
cargo test -p andromeda-exec --test c5_commit_rollback_lifecycle
```

### Validation matrix

| Scenario | Expected result |
| --- | --- |
| `TxCommit` LSN is covered by durable prefix and transaction has no conflicting terminal record. | Evidence validates and transaction may become `Committed`. |
| `durable_commit_lsn < commit_record_lsn`. | Reject evidence and keep transaction `Committing` or recovery-incomplete. |
| `durable_commit_lsn == 0`. | Reject evidence. |
| Commit record exists only after a truncated or corrupt tail boundary. | Reject evidence; transaction remains invisible. |
| WAL chain gap, duplicate, reorder, or `previous_lsn` mismatch occurs before the commit record. | Reject normal recovery and require forensic handling or refusal. |
| Both `TxCommit` and `TxRollback` exist for one transaction in the accepted prefix. | Reject conflicting terminal evidence. |
| Audit or trace says commit visible but WAL evidence is missing. | Reject the projection as non-authoritative. |
| Recovery frontier is below the commit record LSN. | Do not reconstruct commit evidence. |

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| A transaction appears `Committed` with no durable LSN. | Status was projected from RAM or forged metadata. | Reject the status and require durable WAL reconstruction. |
| Commit evidence uses the same field for record LSN and frontier LSN. | Commit position and recovery boundary were conflated. | Split `commit_record_lsn`, `durable_commit_lsn`, and `recovery_frontier_lsn`. |
| Recovery replays a mutation for an incomplete transaction. | Transaction summary did not require terminal commit evidence. | Skip the record and keep the transaction invisible. |
| Audit replay makes a commit visible. | Audit was treated as storage truth. | Use audit only as forensic correlation and require WAL-backed evidence. |
| A truncation report hides the last valid LSN. | Recovery boundary evidence is incomplete. | Record the last valid durable LSN and first failed boundary. |
| Product or domain adapter publishes prepared state early. | Adapter accepted mutation evidence without commit evidence. | Require `DurableCommitEvidence v0` before publication. |

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
- `crates/andromeda-tx/src/commit_log/entry.rs`
- `crates/andromeda-tx/src/commit_log/replay.rs`
- `crates/andromeda-tx/src/commit_log/manager/validation.rs`
- `crates/andromeda-wal/src/write_ahead_log/transaction.rs`
- `crates/andromeda-storage/src/recovery/planning.rs`
- `crates/andromeda-storage/src/recovery/startup/evidence.rs`
- `crates/andromeda-exec/src/business/product_stock/evidence.rs`
- `crates/andromeda-exec/src/local/runtime/events.rs`
