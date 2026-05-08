# AuditLedger v0 Specification

## Purpose

Define the accepted documentation contract for `AuditLedger v0`, the durable
audit evidence boundary used for security, administrative, catalog, recovery,
cluster, backup, restore, and forensic decisions.

`AuditLedger v0` is forensic and authorization evidence. It is not database
truth, not a replacement for WAL-backed storage recovery, and not a transaction
commit-path dependency.

## Scope

This specification applies to the durable audit record layer and documentation
that references audit evidence for Lot 5 acceptance.

It covers:

- append-only record-layer behavior;
- payload checksum and checksum-chain evidence;
- principal and correlation binding;
- event family classification;
- retention compaction caveats;
- replay and inspection boundaries;
- operator wording for current CLI audit tooling.

## Non-goals

This specification does not:

- claim an implemented Admin RPC audit read endpoint;
- make audit records database truth;
- place audit appends on the transaction commit path;
- define an immutable byte-for-byte journal across retention compaction;
- define full indexing, distributed audit search, or long-retention query
  performance guarantees;
- authorize global audit disable switches;
- replace WAL, cold snapshots, storage manifests, recovery reports, or PITR
  evidence;
- introduce gRPC, runtime JSON defaults, ad hoc SQL, generic command text, or
  untyped audit payloads.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- DEC-033 for durable audit ledger architecture, event families, checksum-chain
  validation, and no global audit disable.
- DEC-040 for audit evidence boundaries and storage truth separation.
- DEC-041 for security contract boundary language.
- `documentations/specs/SecurityAdmission_v0.md` for pre-transaction security
  admission evidence.
- Current operator tooling vocabulary: `audit inspect`, `audit verify`, and
  `audit compact`.

## Procedure

### Record model

An `AuditLedger v0` record must carry bounded, typed evidence:

| Evidence | Required content |
| --- | --- |
| Record identity | Event id, trace id, durable audit family, sequence number. |
| Principal binding | Principal id when known, certificate fingerprint when applicable, surface, permission, request id, session id. |
| WAL evidence | Record LSN, durable LSN, payload checksum. |
| Chain evidence | Previous chain checksum and current chain checksum for journal continuity. |
| Retention evidence | Retention boundary, policy hold, compaction eligibility. |
| Replay behavior | Forensic-only replay, decision-index rebuild, or corruption boundary behavior. |
| Event payload summary | Sanitized event kind and bounded decision metadata. |

Audit records must not store secret-bearing payloads, raw credentials, private
keys, or unbounded request bodies.

### Event families

`AuditLedger v0` uses durable audit families to keep evidence reviewable:

| Family | Typical source | Visibility rule |
| --- | --- | --- |
| Security decision | Authentication, authorization, surface admission | Required before accepted security decision visibility where the owning runtime proves the durable path. |
| Admin decision | Administration surface operations | Required before accepted administrative decision visibility where the owning runtime proves the durable path. |
| Catalog decision | DefinitionBatch and catalog publication decisions | Required before accepted catalog decision visibility where the owning runtime proves the durable path. |
| Admission decision | Protocol and execution admission | Must preserve correlation and fail-closed outcome evidence. |
| HA/DR decision | Quorum, fencing, promotion, WAL shipping decisions | Must preserve cluster decision evidence. |
| Backup decision | Backup plan and execution boundary | Must preserve backup evidence and retention context. |
| Restore decision | Restore and PITR boundary | Must preserve target LSN or timestamp evidence. |
| Forensic decision | Forensic startup and hold decisions | Must preserve why application traffic was blocked or allowed. |
| Recovery decision | Recovery startup, replay, and corruption boundaries | Must not re-authorize or re-execute application work. |
| Generic audit | Non-critical forensic events | Must remain bounded and classified. |

### Append and durability rules

The audit record layer is append-only. Each appended record must produce payload
checksum evidence. Durable journal files also carry predecessor and current chain
checksums so replay can detect missing, reordered, or corrupted records.

Critical decision visibility may depend on durable audit evidence when the
owning implementation claims that path. This does not make audit appends the
transaction commit path. Transaction commit visibility remains governed by
durable WAL, and database truth remains the latest valid cold snapshot plus
durable WAL from that snapshot.

Audit replay must be forensic. It must not re-execute Procedures, re-authorize
requests, advance catalog publication, or make storage state visible.

### Retention compaction

Retention compaction may rewrite retained audit records into a compacted journal
and rethread checksum-chain evidence. Compaction must preserve retained record
payload checksum evidence and retention policy evidence.

Documentation must not describe the physical journal bytes as immutable across
compaction. The append-only guarantee applies to the record layer and retained
evidence, not to byte identity after policy-governed compaction.

### Operator wording

Current operator-facing audit documentation must use:

```text
audit inspect
audit verify
audit compact
```

Do not describe an Admin RPC audit read endpoint as implemented until code
evidence proves that endpoint and its authorization policy.

## Validation

Documentation acceptance checks:

- The spec cross-references DEC-040 and DEC-041.
- The spec describes `AuditLedger v0` as append-only at the record layer and
  checksum chained.
- The spec states that retention compaction may rethread chain evidence while
  preserving retained payload checksums.
- The spec does not claim audit ledger records are database truth.
- The spec does not place audit appends on the transaction commit path.
- The spec does not claim an implemented Admin RPC audit read endpoint.
- The spec uses current CLI operator wording only.

Future implementation work should add or keep targeted validation for:

```powershell
cargo test -p andromeda-observe --test durable_audit_sink_contract
cargo test -p andromeda-observe --test durable_audit_query_contract
cargo test -p andromeda-observe --test audit_family_contract
```

These commands are not required for WR5-DOC documentation-only acceptance.

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Replay accepts a missing predecessor link. | Chain validation did not fail closed. | Reject replay at the corruption boundary and preserve forensic evidence. |
| Compaction loses retained payload checksum evidence. | Compaction rewrote records without carrying retained checksum evidence. | Reject compaction output and restore from the previous retained ledger source. |
| Audit text says audit is database truth. | DEC-033 or DEC-040 wording drift. | Reword as forensic evidence and cite cold snapshot plus durable WAL truth. |
| Audit text says audit is on the commit path. | Decision visibility was confused with transaction commit visibility. | Reword to durable decision evidence outside the transaction commit path. |
| Operator docs imply an implemented Admin RPC audit read endpoint. | Acceptance wording exceeded current evidence. | Reword to current CLI audit tooling until endpoint evidence exists. |
| Audit record includes secret-bearing fields. | Payload sanitization failed. | Reject the record and emit sanitized failure evidence. |

## References

- `docs/adr/ADR-0012-quic-rpc-no-grpc.md`
- `documentations/governance/decisions/DEC-033-durable-audit-ledger.md`
- `documentations/governance/decisions/DEC-040-rpc-quic-security-boundary.md`
- `documentations/governance/decisions/DEC-041-security-contract-boundary.md`
- `documentations/specs/SecurityAdmission_v0.md`
- `crates/andromeda-observe/src/events/durable_audit/`
- `crates/andromeda-observe/tests/durable_audit_sink_contract.rs`
- `crates/andromeda-observe/tests/durable_audit_query_contract.rs`
- `crates/andromeda-observe/tests/audit_family_contract/`
