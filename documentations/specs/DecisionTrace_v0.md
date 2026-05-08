# DecisionTrace v0 Specification

## Purpose

Define the accepted documentation contract for `DecisionTrace v0`, the
versioned, bounded, audit-safe evidence emitted when Andromeda accepts,
rejects, defers, or explains a critical engine decision.

`DecisionTrace v0` is forensic and operational evidence. It is not database
truth, not a replacement for typed Procedure contracts, and not permission to
make a transaction visible without durable WAL evidence.

## Scope

This specification applies to decision traces produced by admission,
authorization, protocol, execution, optimizer, IO placement, GPU policy,
transaction, catalog, manifest, WAL, recovery, backup, HA/DR, and forensic
workflows.

It covers:

- versioned trace identity;
- bounded reason and evidence fields;
- actor, object, before-state, and after-state evidence;
- correlation identifiers for Procedure, request, transaction, WAL, catalog,
  and protocol review;
- audit-safe redaction and retention rules;
- durability and visibility constraints for mission-critical decisions;
- validation hooks for the current `andromeda-observe` event envelope.

## Current Implementation Status

The current `andromeda-observe` crate exposes a compact `DecisionTrace` with
`trace_id`, `CriticalDecisionKind`, and `reason`, plus `EventEnvelope`,
`EventCorrelation`, lifecycle sequence validation, and specialized event
variants for security, WAL, completion, recovery, IO, GPU, transaction, and
execution transitions.

This specification is the target documentation contract for decision evidence.
It does not claim that every field below is currently encoded in one concrete
Rust struct. Existing compact traces are valid only when the envelope,
correlation, and event-specific payload together preserve the required
semantics.

## Non-goals

This specification does not:

- introduce application-facing ad hoc SQL;
- bypass typed Procedure contracts or full contract binding evidence;
- make audit records, RAM, temp files, GPU output, or benchmark output
  database truth;
- place GPU work in commit, WAL, rollback, recovery, MVCC short-visibility,
  catalog publication, or security-critical paths;
- define the binary WAL, page, manifest, RPC, or audit journal format;
- expose Administration, backup, restore, failover, quorum, fencing, or
  forensic startup through the Application surface;
- introduce gRPC, runtime JSON defaults, generic command text, or untyped
  payload tunnels.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `documentations/specs/ProcedureContract_v0.md` for typed Procedure contract
  evidence.
- `documentations/specs/SecurityAdmission_v0.md` for pre-transaction security
  and resource-admission order.
- `documentations/specs/AuditLedger_v0.md` for durable audit evidence
  boundaries.
- `documentations/specs/WalRecord_v0.md` for durable WAL evidence.
- `documentations/specs/RecoveryReport_v0.md` for recovery and forensic-start
  outcomes.
- `crates/andromeda-observe/src/events/decision.rs`.
- `crates/andromeda-observe/src/events/envelope/validation.rs`.
- `crates/andromeda-observe/tests/v0_procedure_lifecycle.rs`.

## Procedure

### Version and envelope

Every `DecisionTrace v0` record must be wrapped in a versioned event envelope or
carry equivalent version evidence.

| Field | Required rule |
| --- | --- |
| `schema_version` | Must identify `DecisionTrace.v0`. New incompatible fields require a new version. |
| `event_id` | Must be non-zero and strictly ordered within the emitting sequence. |
| `trace_id` | Must be non-zero and must match the payload trace id. |
| `event_family` | Must classify the decision family without relying on display text. |
| `emitted_at` | Must be a bounded engine timestamp or monotonic sequence reference. |
| `producer` | Must identify the crate, subsystem, node, or service that emitted the record. |
| `correlation` | Must carry the identifiers required for the decision family. |

The current Rust envelope validates non-zero event and trace identity, payload
trace matching, secret-safe text, request/session requirements for
request-scoped events, transaction correlation, durable LSN correlation, and
catalog correlation for manifest decisions.

### Bounded fields

Implementations must set explicit maximum sizes. A future implementation may
choose smaller limits, but it must not be unbounded.

| Field class | Required bound |
| --- | --- |
| Reason text | Maximum 512 UTF-8 bytes after sanitization. |
| Reason code | Stable numeric or string code from a versioned vocabulary. |
| Evidence references | Maximum 32 references per trace. |
| Evidence reference label | Maximum 64 UTF-8 bytes. |
| Evidence digest | Fixed-size digest or typed hash, never a raw body. |
| Object identifiers | Typed ids or canonical names with a documented length limit. |
| Metrics | Fixed numeric fields with units, not free-form maps. |
| Diagnostic excerpt | Maximum 256 sanitized UTF-8 bytes and never a payload body. |

Text fields must be rejected or redacted if they contain credentials, bearer
tokens, private key material, passwords, raw payload bodies, unbounded SQL text,
or generic command text.

### Required trace shape

`DecisionTrace v0` must preserve these semantics across the payload and
envelope:

| Field group | Required evidence |
| --- | --- |
| Identity | `schema_version`, `event_id`, `trace_id`, producer, decision family, and stable decision kind. |
| Actor | Principal id, certificate fingerprint, surface, or system actor when applicable. |
| Object | Procedure, catalog object, transaction, WAL boundary, manifest, frame, stream, policy, storage artifact, or recovery attempt. |
| Before state | Previous phase, previous version, observed policy, offered protocol version, requested budget, or prior durable boundary when meaningful. |
| After state | Accepted, denied, deferred, degraded, selected, committed-visible, rolled back, or forensic-only outcome. |
| Reason | Non-empty reason code and bounded human-readable explanation. |
| Evidence | Bounded typed references to contract hash, catalog version, policy version, stats version, durable LSN, checksum, plan class, resource budget, or recovery boundary. |
| Correlation | Request id, session id, invocation id, transaction id, durable LSN, protocol fields, and catalog identifiers required for the family. |
| Retention | Event family, retention class, forensic hold id when present, and compaction eligibility. |
| Safety | Secret-safe status and payload-body exclusion. |

### Decision families

Decision families must use stable codes. Display names are convenience labels
only.

| Family | Required correlation | Visibility rule |
| --- | --- | --- |
| Procedure contract | Request id, session id, `ContractHash`, `CatalogVersion`, catalog object id | Must reject before transaction creation when contract evidence does not match. |
| Security admission | Request id, session id, surface, principal or certificate evidence, policy version | Must be emitted for accepted IAM decisions and IAM denials before transaction creation. |
| Protocol admission | Protocol version, stream id, frame type, payload kind, request or session id as scoped | Must reject typed frame, version, and stream failures without opening an untyped payload tunnel. |
| Optimizer or plan | Procedure contract binding, stats version, plan class, bounded evidence version | Must remain observable, bounded, versioned, explainable, and disableable. |
| Resource or IO | Pipeline, stage, requested budget, allowed budget, selected tier | Must not treat RAM or temp storage as truth. |
| GPU policy | Pipeline, declared availability, policy, accepted flag | Must never place GPU output on a critical commit, recovery, catalog, security, or MVCC visibility path. |
| Transaction or WAL | Transaction id, appended LSN, durable LSN, previous and next phase | Commit visibility requires durable WAL evidence first. |
| Catalog or manifest | Catalog version, object id, manifest epoch, WAL anchor | Publication requires explicit durable and validation evidence. |
| Recovery or forensic | Recovery attempt, durable LSN, corruption boundary, mode | Forensic decisions must preserve non-mutating inspection and block Application traffic when required. |

### Audit-safe content

Decision traces must not include:

- raw request payloads or result payloads;
- SQL text or generic command text as an application-facing surface;
- private keys, certificates, bearer tokens, passwords, API keys, or secrets;
- full WAL records, page bodies, manifest bodies, or snapshot bodies;
- unbounded error strings copied from external systems;
- personally identifying free text when a typed principal id or fingerprint is
  sufficient.

Allowed evidence includes typed ids, bounded reason codes, fixed-size digests,
LSNs, offsets, catalog versions, contract hashes, policy versions, stats
versions, protocol numeric fields, and sanitized excerpts.

### Durability and visibility

Decision traces can explain why a state transition was accepted. They do not
create durable truth by themselves.

| Decision | Required durable evidence |
| --- | --- |
| Commit visible | Non-zero durable commit LSN that matches the transaction and envelope correlation. |
| Rollback durable | Non-zero durable rollback LSN that matches the transaction and envelope correlation. |
| WAL flush | Appended LSN and durable LSN evidence. |
| Manifest switch | Manifest epoch plus WAL anchor evidence. |
| Recovery startup | Last durable LSN and optional corruption boundary evidence. |
| Catalog publication | Catalog version, object id, publication WAL evidence, and contract hash when Procedure-related. |

Audit evidence may be required for accepted administrative or security
decisions, but transaction commit visibility remains governed by durable WAL.

### Retention and replay

`DecisionTrace v0` retention must be policy-driven and auditable:

- security, administration, catalog publication, recovery, forensic, HA/DR,
  backup, restore, WAL, and commit visibility decisions are retained according
  to mission-critical audit policy;
- low-risk operational decisions may be compacted after their payload checksum,
  summary, and correlation references are retained;
- forensic hold blocks deletion and compaction that would destroy evidence;
- replay of decision traces may rebuild indexes or reports, but must not
  re-execute Procedures, re-authorize requests, publish catalog state, or make
  storage state visible.

## Validation

Documentation acceptance checks:

- The spec describes `DecisionTrace v0` as evidence, not database truth.
- The spec defines version evidence, bounded field limits, and secret-safe
  content rules.
- The spec includes actor, object, before-state, after-state, correlation, and
  retention evidence.
- The spec states that visible commit requires durable WAL evidence.
- The spec states that GPU decisions are outside critical commit, WAL,
  recovery, MVCC visibility, catalog publication, and security paths.
- The spec does not introduce gRPC, runtime JSON defaults, ad hoc SQL, generic
  command text, or untyped payload tunnels.

Current and future implementation work should keep targeted validation for:

```powershell
cargo test -p andromeda-observe --test v0_procedure_lifecycle
cargo test -p andromeda-observe --test audit_family_contract
cargo test -p andromeda-observe --test protocol_correlation_contract
```

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Trace reason includes a token or payload body. | Raw diagnostic text was copied into the trace. | Reject or redact the trace and replace the value with a bounded reason code and digest. |
| Commit-visible decision lacks durable LSN evidence. | Decision evidence was confused with WAL truth. | Reject the decision until a matching durable commit LSN is present. |
| Procedure contract decision carries only a name. | Contract binding evidence is incomplete. | Add `ContractHash`, `CatalogVersion`, and catalog object id correlation. |
| GPU decision is used to authorize a critical transition. | GPU output crossed a forbidden boundary. | Move GPU output to advisory analytics and require CPU or durable evidence for the critical path. |
| Replay of decision traces changes database state. | Replay was implemented as execution instead of forensic indexing. | Limit replay to report or index reconstruction outside database truth. |

## References

- `documentations/specs/ProcedureContract_v0.md`
- `documentations/specs/SecurityAdmission_v0.md`
- `documentations/specs/AuditLedger_v0.md`
- `documentations/specs/WalRecord_v0.md`
- `documentations/specs/RecoveryReport_v0.md`
- `crates/andromeda-observe/src/events/decision.rs`
- `crates/andromeda-observe/src/events/envelope/validation.rs`
- `crates/andromeda-observe/src/events/envelope/secret_safety.rs`
- `crates/andromeda-observe/tests/v0_procedure_lifecycle.rs`
