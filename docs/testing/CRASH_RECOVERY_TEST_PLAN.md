# Crash recovery test plan

> **Status:** Testing guidance  
> **Audience:** Maintainers, QA, engine developers, release owners  
> **Baseline:** Rust 1.95.0

## In this article

- Define the purpose of this test area.
- State required evidence.
- Provide acceptance checks.

## Scope

This document applies to Andromeda documentation, implementation planning, and release readiness.

## Required coverage

| Area | Requirement |
|---|---|
| `before durable TxBegin` | Must have explicit test evidence or a documented exclusion. |
| `after TxBegin before mutation WAL` | Must have explicit test evidence or a documented exclusion. |
| `after mutation WAL before TxCommit` | Must have explicit test evidence or a documented exclusion. |
| `after durable TxCommit before client ACK` | Must have explicit test evidence or a documented exclusion. |
| `after client ACK before dirty page flush` | Must have explicit test evidence or a documented exclusion. |
| `during manifest switch` | Must have explicit test evidence or a documented exclusion. |
| `during Map delta apply` | Must have explicit test evidence or a documented exclusion. |

## Crash/Recovery Scenario Matrix

| Scenario | Scope | Crash or failure point | Durable state to prove | Required validation evidence | Residual risk to record |
|---|---|---|---|---|---|
| CR-11-WAL | WAL scan and replay | Torn or incomplete WAL tail | Last valid LSN is retained and invalid tail is rejected | `cargo test -p andromeda-recovery --test file_wal_recovery_contract --locked -- --nocapture` | Retain malformed byte corpus and tail boundary notes. |
| CR-11-STORAGE | Heap and page redo | Crash after durable record before page flush | Redo rebuilds page-visible state without hidden RAM truth | `cargo test -p andromeda-recovery --test recovery_replay_heap_redo_contract --locked -- --nocapture` | Retain page format and heap replay coverage gaps. |
| CR-11-MANIFEST | Manifest switch | Crash during manifest publication | Recovery selects a valid root and rejects partial switch state | `cargo test -p andromeda-recovery --test recovery_completeness_contract --locked -- --nocapture` | Retain manifest generation and previous-root evidence. |
| CR-11-CATALOG-PUBLICATION | Catalog publication | Crash before catalog commit evidence | Visible CatalogVersion requires committed durable publication | `cargo test -p andromeda-catalog --test catalog_store_contract --locked -- --nocapture` | Retain incomplete DefinitionBatch and stale publication cases. |
| CR-11-MAP-PUBLICATION | Map publication | Crash during active map switch | Map output remains advisory until durable publication evidence exists | `cargo test -p andromeda-maps --test map_publication_contract --locked -- --nocapture` | Retain rebuild and rollback evidence. |
| CR-11-FORENSIC-STARTUP | Forensic startup | Corruption or uncertain WAL chain at open | Startup chooses ForensicOnly or ReadOnly before Online | `cargo test -p andromeda-recovery --test startup_modes_contract --locked -- --nocapture` | Retain operator opening decision and RecoveryReport. |
| CR-11-BACKUP-PITR | Backup and PITR | Restore to target LSN after backup artifact selection | Restore plan validates artifact, WAL range, checksum, and open mode | `cargo test -p andromeda-restore --test restore_contract --locked -- --nocapture` | Retain full drill artifact, target LSN, and RestoreTrace. |
| CR-11-HADR | HA/DR promotion | Primary crash, partition, or promotion race | Quorum, fencing, promotion, and replica repointing are auditable | `cargo test -p andromeda-hadr --test hadr_promotion_runtime_contract --locked -- --nocapture` | Retain cluster simulation transcript and audit chain. |


## General rules

- Tests are evidence, not ceremony.
- C5 paths require crash or recovery evidence when durable state is affected.
- Fuzz untrusted or semi-trusted byte parsers.
- Property-test codecs, ordering, hashes, and state machines.
- Do not normalize flaky tests through blind retries.

## Minimum evidence record

```text
TestId
Component
Criticality
Input model
Expected behavior
Observed behavior
TraceId when applicable
RecoveryReport when applicable
Decision
```

## Rejection criteria

Reject release readiness when:

- commit visibility is not crash-tested;
- recovery does not emit RecoveryReport;
- WAL parser has no malformed-input tests;
- RPC payload length is allocated before validation;
- a GPU path has no CPU fallback;
- a security decision lacks audit evidence.
