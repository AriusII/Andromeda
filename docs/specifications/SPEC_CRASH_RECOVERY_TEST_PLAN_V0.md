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
| `RecoveryEvidenceId` | Stable id linking a crash scenario to the spec invariant it proves. |
| `CrashArtifactRef` | Stable path or digest for logs, reports, corpora, byte fixtures, and restored artifacts. |
| `DocumentedExclusion` | Typed explanation for a scenario that is not executable in the current implementation slice. |

## Invariants

- Every C5 durable mutation path has at least one positive and one rejection crash scenario before release promotion.
- Visible commit requires durable WAL evidence before the commit is observable.
- Recovery truth is a valid cold snapshot plus durable WAL since that snapshot.
- Recovery emits typed `RecoveryReport` evidence.
- A passing unit test without crash or replay evidence does not satisfy a durable release gate.
- `OpenModeDecision` must be retained for every recovery attempt that opens storage or rejects open.
- `ForensicOnly` is a startup result, not a service mode. It must block application connections.
- Planned commands are not evidence until the command, commit SHA, toolchain, observed output, and report artifacts are retained.
- Documented exclusions must be explicit and temporary; they cannot be counted as positive release evidence.

## Serialization

- Crash evidence records are retained as structured text or JSON with explicit field names.
- Binary artifacts referenced by the evidence must be named by digest or stable path.
- Native Rust layout must not be used as retained evidence format.

### CrashEvidenceRecordV0 schema

| Field | Required rule |
|---|---|
| `RecoveryEvidenceId` | Stable id in the form `P01-CR-<area>-<number>`. |
| `ScenarioId` | Stable scenario id, for example `CR-11-WAL`. |
| `SpecInvariant` | Exact invariant or rejection rule proved by the scenario. |
| `OwnerCrate` | Crate or documented owner responsible for executable evidence. |
| `Command` | Exact command line run. |
| `CommitSha` | Git commit SHA or explicit dirty-worktree marker for non-release evidence. |
| `Toolchain` | Rust toolchain and relevant test runner version. |
| `CrashPoint` | Precise operation boundary where failure was injected. |
| `DurableStateBefore` | Snapshot, manifest, WAL range, LSN, or audit state before failure. |
| `ObservedRecoveryDecision` | `Online`, `ReadOnly`, `ForensicOnly`, or `Reject`. |
| `RecoveryReport` | Path or digest for retained `RecoveryReportV0`; required unless startup rejects before durable scan. |
| `ReportAssertions` | Required fields and values asserted from the report. |
| `Artifacts` | Logs, byte fixtures, restored files, corpora, or screenshots by stable path or digest. |
| `ResidualRisk` | Remaining unproved behavior or `None`. |
| `Decision` | `Accepted`, `Blocked`, or `Excluded`. |

### Required artifact locations

| Artifact type | Required location pattern |
|---|---|
| Retained crash evidence | `.work/codex/p01-normative-specification-baseline/evidence/<ScenarioId>/` |
| Malformed WAL byte corpus | `crates/andromeda-recovery/tests/fixtures/wal_malformed/` or retained equivalent under `.work/codex/.../evidence/` |
| RecoveryReport JSON/text projection | `.work/codex/p01-normative-specification-baseline/evidence/<ScenarioId>/recovery-report.*` |
| Restore/PITR drill transcript | `.work/codex/p01-normative-specification-baseline/evidence/CR-11-BACKUP-PITR/` |
| HA/DR simulation transcript | `.work/codex/p01-normative-specification-baseline/evidence/CR-11-HADR/` |
| Documented exclusion | `.work/codex/p01-normative-specification-baseline/evidence/<ScenarioId>/exclusion.md` |

### Golden vectors and corpora

Crash/recovery readiness must reference golden vectors or malformed corpora for:

- WAL record frame and FileWal header durable-prefix parsing;
- page header/trailer and PageLSN replay decisions;
- manifest hash/root switch selection;
- SegmentIndex file validation or rebuild decision;
- `RecoveryReportV0` serialization/projection;
- malformed WAL scan stops for recoverable tail and forensic chain break.

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

## Crash scenario evidence matrix

| Scenario | Evidence id | Spec invariant proved | Required report assertions | Existing/planned command | Release status rule |
|---|---|---|---|---|---|
| `CR-11-WAL` | `P01-CR-WAL-001` | WAL durable prefix stops at last valid LSN and invalid tail is rejected. | `LastValidWalLsn`, `DurablePrefixBytes`, `ScanStop`, `BoundaryKind`, `OpenModeDecision`. | `cargo test -p andromeda-recovery --test file_wal_recovery_contract --locked -- --nocapture` | Accepted only with malformed corpus and retained report. |
| `CR-11-TRANSACTION-COMMIT` | `P01-CR-TX-001` | Visible commit requires durable `TxCommit` before client-visible completion. | `ReplayRecords` includes committed transaction only when `TxCommit` is inside durable prefix. | `cargo test -p andromeda-transaction --test v0_transition_lifecycle --locked -- --nocapture` plus recovery replay evidence. | Blocks release if commit visibility is not crash-tested. |
| `CR-11-TRANSACTION-INCOMPLETE` | `P01-CR-TX-002` | Crash before durable commit leaves transaction invisible. | `IgnoredTransactions` contains reason `Incomplete`; no replay record publishes visibility. | `cargo test -p andromeda-recovery --test transaction_wal_bridge --locked -- --nocapture` | Accepted only with report assertions. |
| `CR-11-STORAGE` | `P01-CR-STORAGE-001` | Redo rebuilds page-visible state from durable WAL and page LSN, not RAM. | `ReplayRecords`, `LastValidWalLsn`, storage validation status. | `cargo test -p andromeda-recovery --test recovery_replay_heap_redo_contract --locked -- --nocapture` | Blocks release if page state can be proven only from RAM. |
| `CR-11-MANIFEST` | `P01-CR-MANIFEST-001` | Manifest switch is atomic and partial root switch is rejected or falls back to valid previous root. | `ManifestRef`, `RequiredWalStartLsn`, `OpenModeDecision`, manifest validation status. | `cargo test -p andromeda-recovery --test recovery_completeness_contract --locked -- --nocapture` | Accepted only with previous-root or rejection evidence. |
| `CR-11-CATALOG-PUBLICATION` | `P01-CR-CATALOG-001` | CatalogVersion is visible only with committed durable publication. | Catalog recovery report plus durable WAL commit evidence. | `cargo test -p andromeda-catalog --test catalog_store_contract --locked -- --nocapture` | Planned evidence must be marked non-release until retained artifacts exist. |
| `CR-11-MAP-PUBLICATION` | `P01-CR-MAP-001` | Map output is advisory until durable publication evidence exists. | Map publication decision, durable WAL fence, rejection or replay status. | `cargo test -p andromeda-maps --test map_publication_contract --locked -- --nocapture` | Use `DocumentedExclusion` if owner crate/test is not present. |
| `CR-11-FORENSIC-STARTUP` | `P01-CR-FORENSIC-001` | Forensic chain break cannot open Online/ReadOnly and must retain report before ForensicOnly. | `BoundaryKind = ForensicChainBreak`, `ForensicRequired = true`, `OpenModeDecision = ForensicOnly` or `Reject`, `ReportPersisted = true` for ForensicOnly. | `cargo test -p andromeda-recovery --test startup_modes_contract --locked -- --nocapture` | Blocks release if application connections are allowed in ForensicOnly. |
| `CR-11-BACKUP-PITR` | `P01-CR-BACKUP-001` | Restore/PITR opens from selected backup plus WAL range at target LSN. | `RestoreTrace`, `ManifestRef`, target LSN, `RecoveryReport`, open-mode decision. | `cargo test -p andromeda-restore --test restore_contract --locked -- --nocapture` plus retained drill transcript. | Unit tests alone are insufficient for production restore readiness. |
| `CR-11-HADR` | `P01-CR-HADR-001` | Promotion requires quorum, fencing, epoch, and durable WAL shipping evidence. | Promotion audit, durable WAL boundary, replica state, open-mode decision after failover. | `cargo test -p andromeda-hadr --test hadr_promotion_runtime_contract --locked -- --nocapture` | Reject self-promotion without quorum and fencing. |

## Crash points required for transaction and startup recovery

| Crash point | Required outcome |
|---|---|
| Before durable `TxBegin` | No transaction exists after recovery; no ignored transaction record is required. |
| After durable `TxBegin` before mutation WAL | Transaction is `Incomplete` and invisible. |
| After mutation WAL before `TxCommit` | Mutation records are skipped and transaction is listed in `IgnoredTransactions`. |
| After `TxCommit` append before durable flush | Commit is not visible; terminal record outside durable prefix is ignored. |
| After durable `TxCommit` before client ACK | Recovery replays committed transaction; visible state is reconstructed from durable WAL. |
| After client ACK before dirty page flush | Recovery replays committed transaction and page redo without trusting client ACK or RAM. |
| During manifest switch | Recovery selects the last valid root or rejects; no partial root becomes truth. |
| During forensic report persistence | Startup rejects accepted open mode unless report persistence completed. |

## Documented exclusions

A `DocumentedExclusion` must include:

- scenario id and evidence id;
- missing crate, missing harness, unavailable crash injector, or out-of-scope owner;
- why the excluded scenario cannot be counted as release evidence;
- owner and follow-up work item;
- expiration condition that converts the exclusion into executable evidence.

An exclusion is valid for P01 planning visibility only. It cannot satisfy C5 release readiness.

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
- CR-11-TRANSACTION-COMMIT durable visible commit tests.
- CR-11-TRANSACTION-INCOMPLETE crash-before-commit skip tests.
- CR-11-FORENSIC-STARTUP report-before-ForensicOnly tests.
- RecoveryReport schema/projection golden vector tests.
- OpenModeDecision rejection and degraded-mode tests.
- documented exclusion validation tests for planned commands.

## Rejection criteria

- Reject `release readiness without RecoveryReport`.
- Reject `visible commit without durable WAL evidence`.
- Reject `crash scenario without retained command`.
- Reject `test that proves only RAM state`.
- Reject `silent repair without open-mode decision`.
- Reject `planned command counted as retained evidence`.
- Reject `ForensicOnly with application connections enabled`.
- Reject `Online or ReadOnly after forensic chain break`.
- Reject `documented exclusion counted as positive evidence`.
- Reject `RecoveryReport missing Owner, Evidence, or Reject acceptance proof in linked specs`.

## Acceptance summary

Owner: Person 10 recovery/crash runner/forensic startup owns the P01 crash/recovery evidence schema, transaction/startup crash points, forensic startup rules, and the normative linkage from this spec to `docs/testing/CRASH_RECOVERY_TEST_PLAN.md`.

Evidence: P01 evidence must include one `CrashEvidenceRecordV0` per executable scenario with command, commit SHA, toolchain, crash point, durable state before failure, observed `OpenModeDecision`, retained `RecoveryReportV0`, required report assertions, artifacts, and residual risk; planned or absent scopes must carry `DocumentedExclusion` records and must not be counted as release evidence.

Reject: P01 must reject crash/recovery readiness when commit visibility is not crash-tested, when recovery lacks a retained report, when `ForensicOnly` enables application serving, when `Online` or `ReadOnly` follows a forensic chain break, when a test proves only RAM state, or when a planned command or documented exclusion is treated as positive evidence.
