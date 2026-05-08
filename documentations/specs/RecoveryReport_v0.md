# RecoveryReport v0 Specification

## Purpose

Define the accepted documentation contract for `RecoveryReport v0`, the
structured recovery and forensic-start evidence emitted after startup, crash
recovery, restore validation, or corruption inspection.

`RecoveryReport v0` is recovery evidence. It is not database truth by itself,
not an operator repair script, and not permission to make a commit visible
without durable WAL evidence.

## Scope

This specification applies to recovery documentation for database startup,
restore validation, WAL replay, corruption boundary detection, and forensic
startup.

It covers:

- startup and recovery modes;
- forensic-start constraints;
- replay evidence and WAL boundary evidence;
- corruption, truncation, and unknown-format handling;
- recovery outcomes;
- non-mutating forensic inspection rules;
- links to `RecoveryTrace`, `AuditLedger v0`, manifests, snapshots, and durable
  WAL evidence.

## Current Implementation Status

The repository contains recovery and audit vocabulary for WAL replay,
completion recovery, durable audit replay behavior, forensic decisions, and
format-gate validation. This specification is the target documentation contract
for the report shape. It does not claim that every field listed here is already
encoded in one concrete runtime packet.

Future implementation work must either implement this report directly or map
existing recovery evidence into an equivalent typed packet without weakening the
field requirements, forensic constraints, or validation gates.

## Non-goals

This specification does not:

- define the WAL record byte format;
- define storage page, B-Tree, manifest, or segment binary layouts;
- authorize replay of unknown or incompatible storage formats;
- allow forensic startup to repair, rewrite, compact, or publish recovered
  state;
- make RAM, temp files, GPU output, benchmark output, or audit records database
  truth;
- replace backup, restore, PITR, or HA/DR manifests;
- expose recovery or forensic startup through the Application surface;
- introduce gRPC, runtime JSON defaults, ad hoc SQL, generic command text, or
  untyped recovery payloads.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `documentations/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md` for recovery
  order, WAL truncation constraints, recovery reports, and corruption response.
- `documentations/04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md` for forensic startup
  and incident response.
- DEC-025 for restore orchestration and `SafeStart` or `ForensicStart` recovery
  stage vocabulary.
- DEC-032 for storage format gates and read-only forensic handling of unknown or
  drifted storage artifacts.
- DEC-033 for durable audit ledger replay behavior, forensic hold, and
  corruption boundary evidence.
- `documentations/specs/AuditLedger_v0.md` for audit evidence boundaries.

## Procedure

### Startup modes

`RecoveryReport v0` must record the requested mode, the effective mode, and the
reason for any downgrade or refusal.

| Mode | Application traffic | Replay behavior | Required report behavior |
| --- | --- | --- | --- |
| `FastStart` | Allowed only after minimal gates pass | Replay complete valid WAL needed for visibility | Emit normal startup evidence and any skipped deep-check warnings. |
| `SafeStart` | Allowed only after reinforced gates pass | Replay complete valid WAL and verify stronger catalog, manifest, and storage invariants | Emit validation evidence for every reinforced gate. |
| `ForensicStart` | Blocked | Inspection-only unless a compatible, explicit forensic replay scan is documented as non-mutating | Emit a forensic report, corruption boundary evidence, and application-block evidence. |
| `RestoreValidation` | Blocked until restore is accepted | Replay from selected snapshot and WAL archive into an isolated target | Emit snapshot, manifest, WAL archive, and target LSN or timestamp evidence. |
| `RefuseStart` | Blocked | None | Emit refusal reason, first failed gate, and operator-safe next action. |

`FastStart` and `SafeStart` must reject unknown or incompatible storage formats
before mutation. `ForensicStart` may inspect unknown or drifted artifacts
read-only with replay disabled and must say that no recovered state was
published.

### Required report shape

`RecoveryReport v0` must be a bounded, typed evidence object. Documentation may
use field names that match local implementation naming, but it must preserve the
following semantics:

| Field group | Required evidence |
| --- | --- |
| Identity | `RecoveryReportId`, `DatabaseId`, `RecoveryAttemptId`, node id when clustered, generated-at timestamp, schema version. |
| Mode | Requested start mode, effective start mode, downgrade or refusal reason, application traffic state. |
| Source state | Active or selected manifest, previous manifest when used, snapshot id, backup id when restore-driven, storage format fingerprints. |
| WAL boundaries | Required WAL start LSN, first inspected LSN, last readable LSN, last valid record LSN, last durable LSN, target restore LSN when applicable. |
| Replay evidence | Records inspected, records applied, records skipped, transactions committed, transactions rolled back, incomplete transactions, idempotence evidence. |
| Corruption evidence | First corruption LSN or artifact id, corruption kind, checksum or chain failure, truncation boundary, unknown-format boundary, affected object ids when known. |
| Validation evidence | Manifest validation, snapshot validation, catalog invariant validation, storage invariant validation, map or index consistency policy results. |
| Audit and trace links | `RecoveryTrace` ids, durable audit event ids, forensic hold id, operator request id, incident id when present. |
| Outcome | Effective database state, visibility decision, degraded status, required operator action, retention or forensic hold state. |

The report must not include secret-bearing values, raw credentials, private
keys, unbounded payload bytes, or full page/WAL bodies. It may reference
artifact digests, offsets, LSNs, object ids, and bounded diagnostic excerpts.

### Replay evidence

Replay evidence must distinguish inspected records from applied records. A
record can be readable but not applied when the mode is forensic, the record is
past a corruption boundary, the storage format is incompatible, or the report is
for restore validation only.

The report must record:

- the WAL segment range opened;
- the checkpoint or cold snapshot used as the replay base;
- whether WAL chain, length, checksum, and predecessor checks passed;
- the last LSN that was complete and valid;
- whether undo or rollback was applied for incomplete transactions;
- whether any page, manifest, catalog, index, map, or audit replay validation
  failed after WAL replay.

Replay must not make a transaction visible unless durable WAL evidence proves
the commit boundary. Audit or trace records can explain the decision, but they
do not replace storage truth.

### Corruption and truncation

`RecoveryReport v0` must classify corruption and truncation without hiding the
boundary.

| Condition | Detection evidence | Required outcome |
| --- | --- | --- |
| WAL tail truncation | Partial record length, missing trailer, checksum failure at tail | Stop at the last complete valid record and mark degraded if data may be missing. |
| WAL middle corruption | Checksum, length, chain, or predecessor mismatch before the tail | Stop replay at the corruption boundary; require `ForensicStart`, restore, or operator policy decision. |
| Unknown WAL payload format | Payload format fingerprint mismatch | Reject mutation; allow only read-only forensic inspection when policy permits. |
| Manifest corruption | Hash, CRC, signature, generation, or root-pointer failure | Try a previous valid manifest when policy permits; otherwise refuse start or enter forensic mode. |
| Snapshot corruption | Snapshot hash or manifest binding failure | Try a previous snapshot plus matching WAL when available; otherwise restore or refuse start. |
| Page or segment corruption | Page trailer, block hash, segment manifest hash, or page LSN failure | Reconstruct from WAL or snapshot only when compatible and validated; otherwise preserve forensic evidence. |
| Catalog invariant failure | Catalog object, version, contract, or dependency inconsistency | Open `ForensicOnly` or refuse startup; do not publish catalog state. |

If multiple boundaries exist, the report must record the earliest boundary that
prevents safe replay and may also record later forensic observations as
non-authoritative evidence.

### Non-mutating forensic constraints

`ForensicStart` is an inspection mode. It must:

- block Application-surface traffic;
- block transaction creation, catalog publication, HA/DR promotion, and
  administrative repair actions unless a separate Administration-surface policy
  explicitly authorizes a bounded operation outside this report;
- avoid WAL append, checkpoint publication, manifest switching, index rebuild
  publication, map refresh publication, and stats publication;
- avoid undo, redo, or repair that changes the source artifacts under
  inspection;
- write forensic evidence only to an allowed external report sink or isolated
  audit path that does not mutate the inspected database truth;
- preserve forensic hold for relevant WAL, audit, snapshot, manifest, and trace
  artifacts.

Forensic inspection may build ephemeral in-memory indexes over evidence to make
the report readable. Those indexes are not truth and must be discarded or stored
outside the inspected database state.

### Outcomes

`RecoveryReport v0` must use one final outcome and may include secondary
warnings.

| Outcome | Meaning | Visibility decision |
| --- | --- | --- |
| `RecoveredOnline` | Recovery completed and invariants passed. | Database may open according to policy. |
| `RecoveredReadOnly` | Recovery completed but policy or degraded status prevents writes. | Read-only access may open on an allowed surface. |
| `RecoveredDegraded` | Safe boundary found but evidence shows missing tail, unavailable replica, or non-critical rebuild needed. | Visibility depends on explicit policy and traceable operator decision. |
| `ForensicOnly` | Inspection evidence exists but normal recovery is unsafe or not authorized. | Application traffic remains blocked. |
| `RestoreRequired` | Local artifacts cannot establish safe truth. | Restore or PITR is required. |
| `Refused` | Startup failed before safe inspection or recovery could complete. | Database remains closed. |

Any outcome that permits visibility must link to the evidence that made it safe:
snapshot, manifest, durable WAL boundary, catalog validation, policy decision,
and relevant `RecoveryTrace` ids.

## Validation

Documentation acceptance checks:

- The spec describes `RecoveryReport v0` as evidence, not database truth.
- The spec includes `FastStart`, `SafeStart`, `ForensicStart`,
  `RestoreValidation`, and refusal behavior.
- The spec states that `ForensicStart` blocks Application-surface traffic.
- The spec states that forensic inspection is non-mutating and does not publish
  repaired state.
- The spec distinguishes inspected WAL records from applied WAL records.
- The spec requires corruption and truncation boundaries to be recorded.
- The spec links recovery outcomes to `RecoveryTrace`, durable audit evidence,
  manifests, snapshots, and durable WAL.
- The spec does not introduce gRPC, runtime JSON defaults, ad hoc SQL, or
  Application-surface administration.

Future implementation work should add or keep targeted validation for:

```powershell
cargo test -p andromeda-storage --test recovery_contract
cargo test -p andromeda-storage --test crash_recovery_impl
cargo test -p andromeda-observe --test durable_audit_query_contract
cargo test -p andromeda-observe --test audit_family_contract
```

These commands are not required for documentation-only acceptance.

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Report says recovered state is true because audit replay succeeded. | Audit evidence was confused with storage truth. | Reword truth as cold snapshot plus durable WAL, with audit as forensic evidence. |
| `ForensicStart` applies redo or publishes a manifest. | Forensic mode mutated inspected artifacts. | Reject the behavior and require isolated inspection with no publication. |
| WAL truncation is reported without last valid LSN. | Boundary evidence is incomplete. | Add last readable LSN, last valid record LSN, and first failed offset or LSN. |
| Unknown storage format opens under `SafeStart`. | Format gate was bypassed. | Reject normal startup and allow only read-only forensic inspection when policy permits. |
| Outcome permits writes after catalog invariant failure. | Catalog validation was not fail-closed. | Downgrade to `ForensicOnly`, `RecoveredReadOnly`, or `Refused` according to policy. |
| Report includes raw page or WAL bodies. | Diagnostic payload is unbounded or sensitive. | Replace raw bytes with digests, offsets, LSNs, and bounded sanitized excerpts. |

## References

- `documentations/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md`
- `documentations/04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md`
- `documentations/governance/decisions/DEC-025-f6-restore-orchestration.md`
- `documentations/governance/decisions/DEC-032-storage-format-gate.md`
- `documentations/governance/decisions/DEC-033-durable-audit-ledger.md`
- `documentations/specs/AuditLedger_v0.md`
- `crates/andromeda-exec/src/services/completion/recovery.rs`
- `crates/andromeda-observe/src/events/durable_audit/`
