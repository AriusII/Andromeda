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

## Invariants

- Critical decisions emit typed trace evidence.
- DecisionTrace explains decisions; it never decides alone.
- Versioned inputs are explicit and stable.
- Reason codes are stable enough for tests and operator reports.
- Traces do not replace durable evidence for storage, transaction, recovery, or audit truth.

## Serialization

- DecisionTrace records use explicit field names and versioned schemas.
- Native Rust layout must not be used as a persisted or exported trace format.
- Any exported trace must redact secrets and retain stable identifiers.

## State transitions

```text
DecisionStarted -> InputsBound -> OutcomeRecorded -> EvidenceLinked
DecisionStarted -> Rejected
```

Missing inputs or unstable reason codes keep the decision rejected.

## Error model

| Error family | Use |
|---|---|
| ContractError | Missing versioned input, reason code, or outcome shape. |
| PermissionError | Trace export or inspection surface is not authorized. |
| SystemError | Trace emission unavailable for a C4/C5 decision. |

## Security model

Traces can include principal and permission references, but must redact secrets. Security decisions must link to audit evidence and policy version.

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

## Recovery behavior

DecisionTrace is not recovery truth. After recovery, traces can explain selected decisions, but durable state is reconstructed from snapshot, manifest, WAL, audit, and recovery evidence.

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
- security trace redaction tests.
- audit linkage tests for permission decisions.
- non-authoritative trace recovery tests.

## Rejection criteria

- Reject `critical decision without DecisionTrace`.
- Reject `trace as durable truth`.
- Reject `unstable reason code`.
- Reject `missing versioned input reference`.
- Reject `secret-bearing trace export`.

## Acceptance summary

This specification is acceptable when owner crates can explain critical decisions with stable typed evidence while preserving WAL, manifest, audit, and recovery as the actual truth sources.
