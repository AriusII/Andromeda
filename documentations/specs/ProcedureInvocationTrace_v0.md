# ProcedureInvocationTrace v0 Specification

## Purpose

Define the accepted documentation contract for `ProcedureInvocationTrace v0`,
the versioned, bounded, audit-safe trace that explains a typed Procedure
invocation from admission through terminal completion, rejection, rollback,
commit visibility, or recovery comparison.

`ProcedureInvocationTrace v0` is lifecycle evidence. It is not an application
SQL surface, not a substitute for a Procedure contract, and not database truth
without durable WAL and catalog evidence.

## Scope

This specification applies to traces emitted by the Application Procedure
invocation path and by internal execution components that project the same
contract-first lifecycle.

It covers:

- Procedure contract and catalog binding evidence;
- request, session, invocation, protocol, and transaction correlation;
- pre-transaction admission and rejection;
- security audit linkage;
- resource and IO admission;
- transaction binding, WAL flush, commit visibility, rollback, and completion;
- recovery comparison hooks for a prior invocation;
- bounded payload, result, and error summaries;
- retention and forensic replay boundaries.

## Current Implementation Status

The current `andromeda-observe` crate exposes `InvocationTrace`,
`ExecutionTransitionTrace`, `TransactionTransitionTrace`,
`InMemoryEventSequence`, `ProcedureLifecycleTrace`, `EventEnvelope`, and
specialized trace variants for security audit, IO budget decisions, WAL,
commit visibility, rollback durability, completion, and recovery startup.

The `crates/andromeda-observe/tests/v0_procedure_lifecycle.rs` test exercises a
current compact lifecycle sequence. This specification is the target contract
for a complete Procedure invocation trace. Existing compact events are
acceptable only when the envelope and correlated event sequence together prove
the required semantics.

## Non-goals

This specification does not:

- introduce ad hoc SQL, dynamic table names, dynamic predicates, shape-shifting
  returns, or implicit null semantics;
- bypass typed Procedure contracts or `ContractHash` verification;
- define SRPL grammar, type checking, or Procedure compilation;
- define RPC frame bytes, Protobuf schemas, StructuredObject payload bytes, or
  WAL record bytes;
- expose Administration, backup, restore, failover, promotion, quorum, fencing,
  WAL shipping, or forensic startup through the Application surface;
- make audit traces, RAM state, temp files, GPU output, benchmark output, or
  result summaries database truth;
- store raw input payloads, raw result payloads, credentials, or unbounded error
  bodies in invocation traces.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `documentations/specs/ProcedureContract_v0.md` for contract binding.
- `documentations/specs/SecurityAdmission_v0.md` for pre-transaction admission.
- `documentations/specs/DecisionTrace_v0.md` for decision evidence rules.
- `documentations/specs/AuditLedger_v0.md` for durable audit evidence.
- `documentations/specs/WalRecord_v0.md` for WAL durability rules.
- `documentations/specs/RecoveryTrace_v0.md` for recovery comparison hooks.
- `crates/andromeda-observe/src/events/sequence.rs`.
- `crates/andromeda-observe/src/events/sequence/lifecycle.rs`.
- `crates/andromeda-observe/src/events/transition.rs`.
- `crates/andromeda-observe/tests/v0_procedure_lifecycle.rs`.

## Procedure

### Version and identity

Every `ProcedureInvocationTrace v0` must carry versioned identity and stable
correlation.

| Field | Required rule |
| --- | --- |
| `schema_version` | Must identify `ProcedureInvocationTrace.v0`. |
| `event_id` | Must be non-zero and strictly increasing within the invocation trace sequence. |
| `trace_id` | Must be non-zero and unique for the emitted event. |
| `invocation_id` | Must be non-zero once assigned. Pre-invocation route failures may omit it only when they explicitly state `NoInvocation`. |
| `request_id` | Must be non-zero for request-scoped events. |
| `session_id` | Must be non-zero for request-scoped events. |
| `surface` | Must identify the listener surface that admitted or rejected the request. |
| `producer` | Must identify the execution, protocol, transaction, or observability component that emitted the event. |

### Contract binding

Accepted Procedure invocation must be contract-first. A trace for an accepted
invocation must include or link to:

| Evidence | Required rule |
| --- | --- |
| `procedure_id` | Must be non-zero. |
| `catalog_object_id` | Must be non-zero and stable through the sequence. |
| `catalog_version` | Must be non-zero and stable through the sequence unless a documented retry restarts admission. |
| `contract_hash` | Must be non-zero and must match the published Procedure contract. |
| `stats_version` | Required when plan or optimizer evidence depends on statistics. |
| `policy_version` | Required when authorization evidence depends on a policy digest. |
| `protocol_layout` | Required when frame or result-stream layout evidence is reviewed. |

The trace must not accept a Procedure by name alone. A name can be diagnostic
context, but it is not binding evidence.

### Lifecycle order

`ProcedureInvocationTrace v0` uses fail-closed lifecycle order.

| Step | Required evidence | Transaction evidence |
| --- | --- | --- |
| Contract admission accepted | Request/session, contract hash, catalog version, catalog object id | Must be absent. |
| Security audited and authorized | Surface, certificate or principal evidence, permission, policy version, audit outcome | Must be absent. |
| Resource or IO admitted | Requested budget, allowed budget, pipeline and stage | Must be absent. |
| Transaction bound | Transaction id, invocation id, request/session correlation | May appear only after admission and authorization. |
| WAL appended or flushed | Transaction id, appended LSN, durable LSN for flush | Required for durability claims. |
| Commit visible | Transaction id and durable commit LSN matching prior WAL flush | Required. |
| Rollback durable | Transaction id and durable rollback LSN matching prior WAL evidence | Required. |
| Completion emitted | Completion code, committed flag, durable LSN when committed or durably rolled back | Required for terminal result. |
| Recovery compared | Recovery attempt id or recovery trace id, last durable LSN, optional corruption boundary | Required only after terminal completion or during forensic review. |

Pre-transaction rejection can occur before transaction binding. It must emit a
typed rejection and terminal completion with no transaction id and no durable
LSN.

### Required trace shape

`ProcedureInvocationTrace v0` must preserve these field groups:

| Field group | Required evidence |
| --- | --- |
| Identity | Schema version, event id, trace id, invocation id when assigned, producer, emitted-at evidence. |
| Actor | Principal, certificate fingerprint, surface, permission family, and policy version when an IAM decision exists. |
| Procedure object | Procedure id, catalog object id, contract hash, catalog version, stats version when applicable. |
| Protocol object | Protocol version, stream id, stream role, frame type, payload kind, sequence number. |
| Before state | Previous invocation phase, previous transaction phase, previous durable boundary, or pre-transaction state. |
| After state | Next invocation phase, terminal outcome, completion code, committed flag, rollback flag, or rejection effect. |
| Payload summary | Input shape id, argument count, bounded byte lengths, and digest references only. |
| Result summary | Result stream ids, metadata-before-payload status, row-count metadata, completion code, and bounded diagnostic references. |
| Correlation | Request id, session id, invocation id, transaction id, durable LSN, catalog identifiers, protocol correlation. |
| Audit links | Security audit trace id, durable audit event id when emitted, decision trace ids, and retention or forensic hold id. |

### Bounded and audit-safe payload policy

Invocation traces must not store raw inputs, raw outputs, full StructuredObject
payloads, payload bodies, SQL text, credentials, certificates, tokens, private
keys, or unbounded error strings.

Allowed payload evidence:

- typed input descriptor id or input shape digest;
- bounded argument count;
- bounded encoded length;
- fixed-size payload digest when needed for replay comparison;
- result stream id and stable result metadata;
- exact row-count metadata when required by the Procedure contract;
- bounded sanitized diagnostic code and reason.

Default v0 bounds:

| Field class | Required bound |
| --- | --- |
| Reason text | Maximum 512 UTF-8 bytes after sanitization. |
| Error detail | Maximum 256 UTF-8 bytes after sanitization. |
| Payload or result digests | Fixed-size typed digest references only. |
| Result stream summaries | Maximum 32 streams per terminal completion trace. |
| Audit or decision links | Maximum 32 links per invocation event. |
| Protocol sequence references | Fixed numeric fields, not free-form maps. |

### Rejection rules

Rejections must identify the stage and effect without leaking secrets.

| Rejection | Required effect |
| --- | --- |
| Frame, protocol, stream role, payload kind, or version rejection | No Procedure dispatch and no transaction creation. |
| Contract mismatch or unknown Procedure | No transaction creation. |
| Surface violation | No transaction creation and no cross-plane fallback. |
| Security denial | Mandatory security audit evidence and no transaction creation. |
| Resource or IO budget rejection | No transaction creation unless the transaction had already been explicitly bound by a later internal stage, which is not allowed for v0 admission. |
| Execution cancellation before transaction | Terminal completion with `committed = false` and no durable LSN. |
| Execution failure after WAL flush | Durable rollback or recovery evidence must explain the terminal state. |

Authorization denial must use typed security audit evidence for IAM decisions.
A legacy authorization-denied event by itself is not sufficient lifecycle
evidence for a v0 Procedure invocation.

### Durability and completion

Completion evidence must be explicit:

- committed completion requires non-zero durable LSN evidence;
- visible commit requires prior WAL flush evidence for the same transaction and
  durable LSN;
- durable rollback requires rollback LSN evidence;
- terminal pre-transaction rejection must not carry transaction id or durable
  LSN correlation;
- recovery comparison must not make new application work visible.

Trace evidence can explain a completion. It cannot replace durable WAL,
catalog, manifest, or storage truth.

### Retention and replay

`ProcedureInvocationTrace v0` retention must preserve accepted invocations,
security denials, contract rejections, completion records, durable transaction
boundaries, and recovery comparisons according to audit policy.

Replay of invocation traces may rebuild observability indexes, produce reports,
and compare recovery output. It must not re-execute Procedures, re-authorize
requests, publish catalog changes, or make storage state visible.

## Validation

Documentation acceptance checks:

- The spec requires contract-first Procedure invocation with `ContractHash` and
  `CatalogVersion` evidence.
- The spec states that pre-transaction rejection has no transaction id and no
  durable LSN.
- The spec requires security audit evidence for IAM authorization decisions.
- The spec states that visible commit requires durable WAL evidence first.
- The spec defines versioned, bounded, audit-safe payload and result summaries.
- The spec does not introduce ad hoc SQL, gRPC, runtime JSON defaults, or
  untyped payload tunnels.

Current and future implementation work should keep targeted validation for:

```powershell
cargo test -p andromeda-observe --test v0_procedure_lifecycle
cargo test -p andromeda-observe --test protocol_correlation_contract
cargo test -p andromeda-exec --test remote_invoke_network_e2e
cargo test -p andromeda-proto --test invocation_boundary_contract
```

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Trace says a Procedure was invoked by name only. | Contract binding evidence is missing. | Add Procedure id, contract hash, catalog version, and catalog object id. |
| Security denial carries transaction id. | Denial occurred after transaction creation or correlation leaked. | Reject the trace and move denial before transaction binding. |
| Committed completion has no durable LSN. | Completion evidence was not tied to WAL. | Reject the completion and require matching WAL flush and commit-visible evidence. |
| Trace stores raw input or result bodies. | Payload summary policy was bypassed. | Replace raw data with shape ids, lengths, and digests. |
| Replay of invocation traces executes a Procedure. | Forensic replay was confused with runtime execution. | Restrict replay to indexes, reports, and recovery comparison. |

## References

- `documentations/specs/ProcedureContract_v0.md`
- `documentations/specs/SecurityAdmission_v0.md`
- `documentations/specs/DecisionTrace_v0.md`
- `documentations/specs/AuditLedger_v0.md`
- `documentations/specs/WalRecord_v0.md`
- `documentations/specs/RecoveryTrace_v0.md`
- `crates/andromeda-observe/src/events/sequence.rs`
- `crates/andromeda-observe/src/events/sequence/lifecycle.rs`
- `crates/andromeda-observe/src/events/transition.rs`
- `crates/andromeda-observe/tests/v0_procedure_lifecycle.rs`
