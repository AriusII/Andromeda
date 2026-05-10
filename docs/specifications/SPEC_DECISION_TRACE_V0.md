# Specification: DecisionTrace v0

> **Status:** Normative V0 specification
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers
> **Language:** American English
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article

- Define the stable evidence shape for critical decisions.
- Separate explanatory traces from durable system truth.
- State invariants, errors, tests, and rejection criteria.

## Purpose

Define how critical decisions are observed and explained after the fact without making traces authoritative durable truth.

## Scope

This specification applies to Procedure admission, security, audit, transaction, recovery, optimizer, resource, map refresh, backup, restore, and HA/DR decisions.

## Non-goals

- It does not make traces a replacement for WAL, manifest, audit ledger, or recovery evidence.
- It does not authorize black-box decisions.
- It does not define a telemetry vendor format.

## Data structures

| Structure | Required role |
|---|---|
| `DecisionTrace` | Stable explanation record for one critical decision. |
| `DecisionKind` | Admission, permission, plan, commit, rollback, recovery, publish, failover, or resource decision. |
| `DecisionInputRef` | Versioned references to inputs such as ContractHash, CatalogVersion, PolicyVersion, StatsVersion, LSN, or manifest hash. |
| `DecisionOutcome` | Allow, deny, reject, choose, publish, rollback, poison, open, fail, or defer. |
| `DecisionReasonCode` | Stable typed reason code. |
| `DecisionEvidenceRef` | Link to audit, WAL, RecoveryReport, scenario evidence, or retained command evidence when applicable. |
| `DecisionTraceSchemaVersion` | Explicit schema version for trace validation and export compatibility. |
| `TraceAuthorityClass` | Classification stating whether the trace is transient, retained diagnostic evidence, or linked durable evidence. It is never authoritative truth. |

## Invariants

- Critical decisions emit typed trace evidence.
- DecisionTrace explains decisions; it never decides alone.
- DecisionTrace is non-authoritative. It cannot allow, deny, commit, publish, recover, restore, promote, or poison state by itself.
- Trace emission is evidence of explanation, not proof of durable state.
- Versioned inputs are explicit and stable.
- Reason codes are stable enough for tests and operator reports.
- Reason codes, evidence labels, explanations, and evidence count are bounded.
- Secret markers are rejected from trace exports and retained trace fields.
- Traces do not replace durable evidence for storage, transaction, recovery, or audit truth.
- Adaptive, optimizer, statistics, benchmark, scenario, analytics, and GPU traces remain advisory unless an owner-specific authority has independently accepted the decision.
- Security-policy decisions must link to audit evidence and `PolicyVersion`; generic DecisionTrace alone is insufficient durable security evidence.

## Serialization

- DecisionTrace records use explicit field names and versioned schemas.
- Native Rust layout must not be used as a persisted or exported trace format.
- Any exported trace must redact secrets and retain stable identifiers.
- Exported traces must include `DecisionTraceSchemaVersion`, `TraceId`, `DecisionKind`, `DecisionOutcome`, `DecisionReasonCode`, `VersionedInputRefs`, and `TraceAuthorityClass`.
- Exported traces must reject zero trace IDs, empty reason codes, over-limit explanations, zero evidence digests, and evidence count overflow.

## Authority and retention

DecisionTrace has three V0 retention classes:

| TraceAuthorityClass | Rule |
|---|---|
| `TransientExplanation` | May be used for live diagnostics only; loss does not affect durable state. |
| `RetainedDiagnosticEvidence` | May be retained for operator explanation or Procedure Store evidence; still does not become truth. |
| `LinkedDurableEvidence` | Carries references to WAL, audit ledger, manifest, RecoveryReport, Procedure Store, or command evidence owned by another component. The linked artifact remains the authority. |

The following claims require non-trace authority:

| Claim | Required authority |
|---|---|
| Transaction committed or rolled back | WAL and transaction state evidence. |
| Catalog publication visible | Catalog version, DefinitionBatch, WAL, and audit evidence. |
| Storage object valid | Snapshot, manifest, page, segment, or WAL evidence. |
| Recovery completed or refused | RecoveryReport, WAL scan evidence, manifest evidence, and audit where applicable. |
| Security allow or deny | SecurityAdmission result, `PolicyVersion`, policy digest, and audit evidence. |
| HA/DR promotion, fencing, backup, or restore | Owner-specific quorum, fencing, backup/restore, WAL, manifest, and audit evidence. |

Generic DecisionTrace must not satisfy durable-retention requirements for these claims.

## State transitions

```text
DecisionStarted -> InputsBound -> OutcomeRecorded -> EvidenceLinked
DecisionStarted -> Rejected
```

Missing inputs or unstable reason codes keep the decision rejected.

DecisionTrace construction must not bypass owner validation. Optimizer plan decisions require complete `ProcedureId`, `ContractHash`, `CatalogVersion`, `StatsVersion`, and `PolicyVersion` bindings. Security-policy decisions require `PolicyVersion` and audit evidence references. Storage, transaction, recovery, and HA/DR decisions require evidence references to their owner artifacts when a durable or visible claim is made.

## Error model

| Error family | Use |
|---|---|
| ContractError | Missing versioned input, reason code, or outcome shape. |
| PermissionError | Trace export or inspection surface is not authorized. |
| SystemError | Trace emission unavailable for a C4/C5 decision. |

## Security model

Traces can include principal and permission references, but must redact secrets. Security decisions must link to audit evidence and policy version.

Trace query and export are permissioned operations. A trace exporter must reject secret-bearing reason codes, evidence labels, evidence digests rendered as all zero, explanations containing sensitive markers, and filters that would leak principal secrets.

## Observability

At minimum, critical decisions must include:

```text
TraceId
DecisionKind
DecisionOutcome
DecisionReasonCode
VersionedInputRefs
EvidenceRefs when applicable
```

For C4/C5 paths, a trace is acceptable only when the owner component also emits the durable, recovery, security, or audit evidence required by that component. A generic DecisionTrace can explain why evidence was used, ignored, stale, rejected, or disabled, but it cannot be the evidence being explained.

## Recovery behavior

DecisionTrace is not recovery truth. After recovery, traces can explain selected decisions, but durable state is reconstructed from snapshot, manifest, WAL, audit, and recovery evidence.

Recovery must tolerate missing transient traces. Retained traces may be replayed for post-incident explanation only after their linked durable artifacts validate. A trace that conflicts with WAL, manifest, audit ledger, or RecoveryReport is rejected as explanatory evidence and never overrides the durable artifact.

## Compatibility

| Change | Default status |
|---|---|
| Add optional evidence reference | Additive |
| Add required decision input | Breaking |
| Rename reason code | Breaking unless aliased |
| Remove decision kind | Breaking |
| Change outcome semantics | Breaking |

## Tests

- stable reason code tests.
- missing versioned input rejection tests.
- zero TraceId, empty reason, too-long explanation, too-many-evidence, and zero evidence digest rejection tests.
- security trace redaction tests.
- audit linkage tests for permission decisions.
- non-authoritative trace recovery tests.
- trace conflict with durable evidence rejection tests.
- optimizer complete binding tests for ProcedureId, ContractHash, CatalogVersion, StatsVersion, and PolicyVersion.
- advisory evidence used, ignored, stale, rejected, fallback, and disabled tests.

## Rejection criteria

- Reject `critical decision without DecisionTrace`.
- Reject `trace as durable truth`.
- Reject `trace as security authority`.
- Reject `trace as recovery authority`.
- Reject `trace as commit or publication authority`.
- Reject `unstable reason code`.
- Reject `missing versioned input reference`.
- Reject `optimizer plan trace without complete version binding`.
- Reject `permission decision trace without PolicyVersion and audit evidence reference after policy consultation`.
- Reject `secret-bearing trace export`.
- Reject `DecisionTraceSchemaVersion inferred from exporter context`.

## Acceptance summary

Owner: `andromeda-decision-trace` owns the runtime-free DecisionTrace contract; storage, WAL, catalog, audit, security admission, recovery, backup/restore, and HA/DR owners keep authority for their durable decisions.

Evidence: acceptance requires schema-version tests, stable reason-code tests, bounded field tests, complete version-binding tests, audit-linkage tests for permission decisions, non-authoritative recovery tests, secret-redaction tests, and conflict-with-durable-evidence rejection tests.

Reject: acceptance is refused when a trace is used as durable truth, security authority, recovery authority, commit authority, catalog publication authority, backup/restore authority, or HA/DR authority; when versioned inputs or `PolicyVersion` evidence are missing; or when `DecisionTraceSchemaVersion` is inferred instead of explicit.
