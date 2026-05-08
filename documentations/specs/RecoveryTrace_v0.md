# RecoveryTrace v0 Specification

## Purpose

Define the accepted documentation contract for `RecoveryTrace v0`, the
versioned, bounded, audit-safe event stream that explains recovery startup,
WAL replay boundaries, corruption boundaries, restore validation, and
forensic-start decisions.

`RecoveryTrace v0` is recovery and forensic evidence. It is not database truth,
not an operator repair script, and not permission to publish recovered state
without compatible manifests, validated storage artifacts, and durable WAL
evidence.

## Scope

This specification applies to recovery traces emitted by startup, crash
recovery, restore validation, WAL replay, manifest validation, corruption
inspection, durable audit replay, and forensic startup.

It covers:

- versioned recovery trace identity;
- startup and recovery attempt correlation;
- WAL, manifest, snapshot, catalog, and storage boundary evidence;
- corruption and truncation evidence;
- non-mutating forensic constraints;
- audit-safe retention and forensic hold behavior;
- links to `RecoveryReport v0`, `DecisionTrace v0`, and durable audit records.

## Current Implementation Status

The current `andromeda-observe` crate exposes compact recovery-related events,
including `RecoveryTrace` with `trace_id`, `last_durable_lsn`, and
`corruption_boundary_lsn`, `CorruptionBoundaryTrace`, `ManifestTrace`,
`WalEventTrace`, `CommitVisibleTrace`, `RollbackDurableTrace`, and lifecycle
sequence validation.

This specification is the target documentation contract for complete recovery
trace evidence. It does not claim that every required field is currently
encoded in one Rust struct. Current compact events are acceptable only when the
event payload and envelope correlation together preserve the required
semantics.

## Non-goals

This specification does not:

- define WAL, page, B-Tree, segment, manifest, snapshot, or audit journal byte
  formats;
- make traces, audit records, RAM, temp files, GPU output, or benchmark output
  database truth;
- authorize replay of unknown or incompatible storage formats;
- allow forensic startup to repair, rewrite, compact, checkpoint, publish, or
  switch manifests for inspected database artifacts;
- expose recovery, restore, backup, failover, promotion, quorum, fencing, or
  forensic startup through the Application surface;
- introduce ad hoc SQL, gRPC, runtime JSON defaults, generic command text, or
  untyped recovery payloads;
- replace `RecoveryReport v0`, backup manifests, PITR manifests, or HA/DR
  quorum evidence.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `documentations/specs/RecoveryReport_v0.md` for report outcomes and
  startup modes.
- `documentations/specs/WalRecord_v0.md` for durable WAL record evidence.
- `documentations/specs/AuditLedger_v0.md` for durable audit replay and
  forensic evidence.
- `documentations/specs/DecisionTrace_v0.md` for decision evidence rules.
- `documentations/specs/ProcedureInvocationTrace_v0.md` for invocation recovery
  comparison hooks.
- `crates/andromeda-observe/src/events/durability.rs`.
- `crates/andromeda-observe/src/events/envelope/validation.rs`.
- `crates/andromeda-observe/tests/v0_procedure_lifecycle.rs`.

## Procedure

### Version and identity

Every `RecoveryTrace v0` event must carry versioned identity and bounded
correlation.

| Field | Required rule |
| --- | --- |
| `schema_version` | Must identify `RecoveryTrace.v0`. |
| `event_id` | Must be non-zero and ordered within the recovery attempt sequence. |
| `trace_id` | Must be non-zero and must match the payload trace id. |
| `recovery_attempt_id` | Must be non-zero and stable for one startup, restore validation, or forensic attempt. |
| `database_id` | Required when a trace can be stored outside the database being inspected. |
| `node_id` | Required for clustered or HA/DR recovery evidence. |
| `mode` | Must record requested and effective recovery mode or link to a `RecoveryReport v0` that records both. |
| `producer` | Must identify the recovery, storage, catalog, audit, or manifest component that emitted the event. |

### Required trace shape

`RecoveryTrace v0` must preserve these field groups:

| Field group | Required evidence |
| --- | --- |
| Identity | Schema version, event id, trace id, recovery attempt id, producer, generated-at evidence. |
| Actor | System startup actor, operator request id, service principal, or incident id when present. |
| Object | Database, manifest, snapshot, WAL segment, page, segment, catalog object, audit ledger, or Procedure invocation being compared. |
| Before state | Requested mode, previous manifest, checkpoint or snapshot base, required WAL start LSN, prior durable LSN. |
| After state | Effective mode, replay boundary, last durable LSN, corruption boundary, forensic-only state, refusal, or recovered visibility state. |
| WAL evidence | First inspected LSN, last readable LSN, last valid record LSN, last durable LSN, target restore LSN when applicable. |
| Replay evidence | Records inspected, records applied, records skipped, transactions committed, transactions rolled back, incomplete transactions. |
| Corruption evidence | Boundary LSN or artifact id, corruption kind, checksum or chain failure, unknown-format boundary, truncation boundary. |
| Validation evidence | Manifest, snapshot, catalog, storage, audit-ledger, map, index, and format-gate validation results. |
| Audit links | Recovery decision trace ids, durable audit event ids, forensic hold id, incident id, and `RecoveryReport v0` id. |
| Safety | Secret-safe status, payload-body exclusion, non-mutating forensic flag when applicable. |

### Bounded fields

Recovery traces must use explicit bounds.

| Field class | Required bound |
| --- | --- |
| Reason text | Maximum 512 UTF-8 bytes after sanitization. |
| Diagnostic excerpt | Maximum 256 sanitized UTF-8 bytes. |
| Artifact references | Maximum 64 references per recovery attempt event. |
| Corruption observations | Maximum 64 observations per event; later details belong in `RecoveryReport v0`. |
| Validation result list | Maximum 64 validation rows per event. |
| Checksums and digests | Fixed-size typed values only. |
| WAL or page data | Never inline raw bodies. Use LSNs, offsets, lengths, and digests. |

Recovery traces must reject or redact credentials, tokens, private keys, raw
payload bodies, raw WAL bodies, raw page bodies, raw manifest bodies, and
unbounded external error strings.

### Recovery modes

The trace must record or link to the effective mode.

| Mode | Trace requirement |
| --- | --- |
| `FastStart` | Record minimal gates, last durable LSN, and any skipped deep-check warnings. |
| `SafeStart` | Record reinforced catalog, manifest, storage, WAL, and audit validation gates. |
| `ForensicStart` | Record Application traffic blocked, mutation disabled, corruption boundary, and forensic hold evidence. |
| `RestoreValidation` | Record selected snapshot, backup, WAL archive, target LSN or timestamp, and isolated target evidence. |
| `RefuseStart` | Record first failed gate and safe operator action. |

Any mode that permits visibility must link the visibility decision to durable
WAL, compatible manifest, snapshot or checkpoint base, catalog validation, and
policy evidence.

### WAL and corruption boundaries

Recovery traces must distinguish readable, valid, durable, and applied
boundaries.

| Boundary | Required rule |
| --- | --- |
| `first_inspected_lsn` | First WAL LSN inspected by the attempt. |
| `last_readable_lsn` | Last LSN that could be read without an IO or format failure. |
| `last_valid_record_lsn` | Last complete record passing length, checksum, and predecessor checks. |
| `last_durable_lsn` | Durable boundary accepted for visibility or forensic comparison. Must be non-zero when startup claims recovery evidence. |
| `corruption_boundary_lsn` | First LSN that prevents safe replay, when known. |
| `truncation_boundary` | Offset or LSN where a tail truncation was detected. |

Middle corruption must stop safe replay at the corruption boundary. Tail
truncation may stop at the last complete valid record only when policy permits.
Unknown or incompatible formats must reject mutation and permit only read-only
forensic inspection when policy allows it.

### Non-mutating forensic constraints

`ForensicStart` and forensic recovery inspection must:

- block Application-surface traffic;
- block transaction creation, catalog publication, checkpoint publication,
  manifest switching, index rebuild publication, map refresh publication, and
  stats publication;
- avoid WAL append, undo, redo, repair, compaction, or mutation of inspected
  database artifacts;
- write forensic evidence only to an allowed external report sink or isolated
  audit path;
- preserve forensic hold for WAL, audit, snapshot, manifest, trace, and report
  artifacts;
- clearly label ephemeral indexes and summaries as evidence navigation, not
  database truth.

### Relationship to RecoveryReport

`RecoveryTrace v0` is an event stream. `RecoveryReport v0` is the summarized
operator and audit artifact for a recovery attempt.

Recovery traces must link to a report when a report is generated. A report may
summarize many traces, but it must not hide the earliest corruption boundary,
the last valid durable boundary, or the effective visibility decision.

### Retention and replay

Recovery traces for startup, corruption, restore, forensic, WAL boundary,
manifest validation, and visibility decisions are mission-critical evidence and
must follow recovery audit retention policy.

Replay of recovery traces may rebuild forensic indexes, reports, and
correlation views. It must not reapply WAL, re-execute Procedures, re-authorize
requests, repair inspected artifacts, or publish recovered state.

## Validation

Documentation acceptance checks:

- The spec describes `RecoveryTrace v0` as evidence, not database truth.
- The spec defines versioned identity, bounded fields, and audit-safe content.
- The spec distinguishes readable, valid, durable, and applied WAL boundaries.
- The spec requires corruption and truncation boundaries.
- The spec states that `ForensicStart` is non-mutating and blocks Application
  traffic.
- The spec links recovery traces to `RecoveryReport v0`, durable audit
  evidence, manifests, snapshots, and durable WAL.
- The spec does not introduce gRPC, runtime JSON defaults, ad hoc SQL, generic
  command text, or Application-surface recovery administration.

Current and future implementation work should keep targeted validation for:

```powershell
cargo test -p andromeda-observe --test v0_procedure_lifecycle
cargo test -p andromeda-observe --test durable_audit_query_contract
cargo test -p andromeda-storage --test recovery_contract
```

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Recovery trace says audit replay proves database truth. | Audit evidence was confused with storage truth. | Reword truth as compatible snapshot or checkpoint plus durable WAL. |
| Trace omits the first corruption boundary. | Later forensic observations hid the blocking boundary. | Record the earliest boundary that prevents safe replay. |
| Forensic startup publishes a manifest or checkpoint. | Forensic mode mutated inspected artifacts. | Reject the behavior and require isolated non-mutating inspection. |
| Last durable LSN is zero on recovery startup. | Recovery boundary evidence is incomplete. | Reject the trace or classify the attempt as refusal evidence. |
| Trace includes raw WAL or page bytes. | Diagnostic content is unbounded or sensitive. | Replace with LSNs, offsets, lengths, and fixed-size digests. |

## References

- `documentations/specs/RecoveryReport_v0.md`
- `documentations/specs/WalRecord_v0.md`
- `documentations/specs/AuditLedger_v0.md`
- `documentations/specs/DecisionTrace_v0.md`
- `documentations/specs/ProcedureInvocationTrace_v0.md`
- `crates/andromeda-observe/src/events/durability.rs`
- `crates/andromeda-observe/src/events/envelope/validation.rs`
- `crates/andromeda-observe/tests/v0_procedure_lifecycle.rs`
