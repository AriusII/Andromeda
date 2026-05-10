# Specification: CrashRecoveryTestPlan v0

> **Status:** Normative V0 specification
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers
> **Language:** American English
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article

- Define the normative crash/recovery evidence model.
- State required scenarios, records, and rejection criteria.
- Connect the test plan to `docs/testing/CRASH_RECOVERY_TEST_PLAN.md`.

## Purpose

Define the minimum crash/recovery proof required before durable-state implementation phases can claim exit evidence.

## Scope

This specification covers WAL, storage page replay, manifest switch, catalog publication, map publication, forensic startup, backup/PITR, and HA/DR crash or failure scenarios.

## Non-goals

- It does not claim production recovery readiness.
- It does not replace retained release evidence.
- It does not make RAM, traces, benchmarks, GPU output, or ResultStream payloads system truth.

## Data structures

| Structure | Required role |
|---|---|
| `CrashScenarioId` | Stable scenario identifier, for example `CR-11-WAL`. |
| `CrashPoint` | Precise operation boundary where failure is injected. |
| `DurableStateBefore` | Snapshot, manifest, WAL range, or audit evidence before failure. |
| `RecoveryReport` | Typed recovery report emitted after restart or forensic scan. |
| `OpenModeDecision` | Online, ReadOnly, ForensicOnly, or Reject decision. |
| `CrashEvidenceRecord` | Retained command, commit SHA, toolchain, observed result, and artifacts. |

## Invariants

- Every C5 durable mutation path has at least one positive and one rejection crash scenario before release promotion.
- Visible commit requires durable WAL evidence before the commit is observable.
- Recovery truth is a valid cold snapshot plus durable WAL since that snapshot.
- Recovery emits typed `RecoveryReport` evidence.
- A passing unit test without crash or replay evidence does not satisfy a durable release gate.

## Serialization

- Crash evidence records are retained as structured text or JSON with explicit field names.
- Binary artifacts referenced by the evidence must be named by digest or stable path.
- Native Rust layout must not be used as retained evidence format.

## State transitions

```text
ScenarioDefined -> EvidenceCommandRun -> RecoveryObserved -> ReportRetained -> Accepted
ScenarioDefined -> EvidenceCommandRun -> RecoveryObserved -> GapRecorded -> Blocked
```

Invalid or missing evidence keeps the scenario blocked.

## Error model

| Error family | Use |
|---|---|
| StorageError | WAL, page, segment, manifest, or corruption failure. |
| TransactionError | Commit, rollback, or isolation recovery failure. |
| SystemError | Poisoned startup, forensic-only mode, or restore requirement. |
| ContractError | Evidence record missing required scenario fields. |

## Security model

Crash/recovery evidence must not expose secrets. Security and audit scenarios must retain principal, policy version, result, and redacted reason evidence.

## Observability

At minimum, retained evidence must include:

```text
ScenarioId
Command
CommitSha
Toolchain
CrashPoint
DurableStateBefore
ObservedRecoveryDecision
RecoveryReport when applicable
ResidualGap when applicable
```

## Recovery behavior

Recovery must either rebuild state, reject unsafe state, or enter a documented degraded open mode. Silent best-effort repair is not acceptable for C5 paths.

## Compatibility

| Change | Default status |
|---|---|
| Add scenario | Additive |
| Remove scenario | Breaking for release evidence |
| Weaken required evidence | Breaking |
| Rename scenario id | Breaking unless aliased |
| Change recovery decision semantics | Breaking |

## Tests

- CR-11-WAL durable prefix tests.
- CR-11-STORAGE heap/page redo tests.
- CR-11-MANIFEST root switch tests.
- CR-11-CATALOG-PUBLICATION tests.
- CR-11-BACKUP-PITR restore tests.
- CR-11-HADR promotion/fencing tests.

## Rejection criteria

- Reject `release readiness without RecoveryReport`.
- Reject `visible commit without durable WAL evidence`.
- Reject `crash scenario without retained command`.
- Reject `test that proves only RAM state`.
- Reject `silent repair without open-mode decision`.

## Acceptance summary

This specification is acceptable when the crash matrix in `docs/testing/CRASH_RECOVERY_TEST_PLAN.md` is complete, machine-checkable, and mapped to owner crate evidence commands.
