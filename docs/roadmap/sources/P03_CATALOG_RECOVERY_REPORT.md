# P03 Catalog Recovery Report Evidence

Date: 2026-05-10

## Objective

Retain executable evidence for catalog recovery, publication report validation, Administration/HA-DR audience restrictions, durable LSN evidence, and HADR replay observability.

## Code Evidence

| Area | Evidence |
| --- | --- |
| Recovery report shape | `crates/andromeda-catalog-recovery/src/replay_report.rs:14` defines `CatalogRecoveryReport` with recovered batches, incomplete batches, skipped anomalous batches, anomalies, and final visible catalog version. |
| Mutation replay entry point | `crates/andromeda-catalog-recovery/src/mutation_replay.rs:46` replays catalog mutation records into a recovery target and returns `CatalogRecoveryTargetOutcome` with report evidence. |
| Complete batch replay | `crates/andromeda-catalog-recovery/tests/mutation_replay_anomalies.rs:60` proves committed batches replay into a generic target. |
| Incomplete/tampered replay rejection | `crates/andromeda-catalog-recovery/tests/mutation_replay_anomalies.rs:154`, `:241`, `:326`, and `:360` cover missing apply records, reordered apply records, hash mismatch, duplicate apply indexes, and sparse apply indexes. |
| Publication audience gate | `crates/andromeda-catalog-recovery/src/publication/report.rs:90` rejects publication reports not addressed to Administration/HA. `crates/andromeda-catalog-recovery/src/publication/audience.rs:4` adds an executable non-authorized audience variant. |
| Durable publication report gate | `crates/andromeda-catalog-recovery/tests/catalog_publication_subscription_runtime_contract.rs:152` rejects missing or zero durable evidence. |
| Audience negative test | `crates/andromeda-catalog-recovery/tests/catalog_publication_subscription_runtime_contract.rs:183` proves non Administration/HA publication reports are rejected. |
| Replay/invalidation mismatch test | `crates/andromeda-catalog-recovery/tests/catalog_publication_subscription_runtime_contract.rs:198` rejects stale plan invalidation version and mismatched recovery replay target version. |
| Duplicate evidence rejection | `crates/andromeda-catalog-recovery/src/publication/runtime.rs:322` preflights duplicate visible publication evidence before mutating registry state; `crates/andromeda-catalog-recovery/tests/catalog_publication_subscription_runtime_contract.rs:228` proves the runtime rejects the second visible publication. |
| HADR restored version evidence | `crates/andromeda-catalog-recovery/tests/catalog_publication_subscription_runtime_contract.rs:277` proves replay summary and HADR subscriber replay evidence restore catalog version, record count, durable LSN, and audit trace id. |
| Fail-closed replay checks | `crates/andromeda-catalog-recovery/tests/catalog_publication_subscription_runtime_contract.rs:377` and `:400` reject skipped catalog versions and terminal conflicts. |

## Executable Commands

```powershell
cargo test -p andromeda-catalog-recovery --test catalog_publication_subscription_runtime_contract --locked -- --nocapture
cargo test -p andromeda-catalog-recovery --test mutation_replay_anomalies --locked -- --nocapture
```

Person 14 ran:

```powershell
cargo test -p andromeda-catalog-recovery --test catalog_publication_subscription_runtime_contract --locked -- --nocapture
```

Result: passed, 10 tests.

## Release Gate

Go only if catalog recovery and publication reports reject missing durable evidence, wrong audience, replay/invalidation mismatch, duplicate visible publication evidence, version gaps, and terminal conflicts. HADR replay evidence must include restored version, expected record count, durable evidence, and audit trace identity.
