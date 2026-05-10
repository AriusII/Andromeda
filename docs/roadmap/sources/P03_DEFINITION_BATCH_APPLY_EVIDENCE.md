# P03 DefinitionBatch Apply Evidence

Date: 2026-05-10

## Objective

Retain executable evidence for DefinitionBatch planning, transactional apply ordering, rollback/fail-closed behavior, all-or-nothing publication, and durable publication identity. P03 requires DefinitionBatch to remain the controlled catalog mutation path.

## Code Evidence

| Area | Evidence |
| --- | --- |
| Planned version and contract hash | `crates/andromeda-definition-batch/tests/alter_drop_compat.rs:127` proves create dry-run emits the planned catalog version and a canonical nonzero procedure `ContractHash`. |
| Version monotonicity | `crates/andromeda-definition-batch/tests/alter_drop_compat.rs:154` and `:238` prove compatible procedure changes preserve identity and advance exactly one catalog version. |
| Dependency graph evidence | `crates/andromeda-definition-batch/tests/alter_drop_compat.rs:190` proves dependency-bearing procedure shape feeds the dependency graph hash. |
| Dry-run idempotence | `crates/andromeda-definition-batch/tests/alter_drop_compat.rs:222` proves repeated dry-run returns identical `source_hash` and `dependency_graph_hash`. |
| Ordered source hash | `crates/andromeda-definition-batch/tests/definition_batch_contract.rs:149` proves source hash binds ordered DefinitionBatch source while dependency graph hash canonicalizes graph edges. |
| Duplicate rejection | `crates/andromeda-definition-batch/tests/definition_batch_contract.rs:237` rejects duplicate object ids and names during dry-run. |
| Lifecycle evidence | `crates/andromeda-definition-batch/tests/definition_batch_contract.rs:305` proves deprecation emits planned lifecycle evidence. |
| Shape drift evidence | `crates/andromeda-definition-batch/tests/definition_batch_contract.rs:369` and `:413` prove source hash changes for procedure contract shape drift and ProcedureId drift. |
| WAL apply envelope | `crates/andromeda-catalog/tests/catalog_store_contract/mutation_wal.rs:1` covers ordered Begin/Apply/Commit records, dense apply indexes, and storage WAL kind mapping. |
| Transactional apply boundary | `crates/andromeda-catalog/src/store.rs:107` implements `apply_definition_batch_durably`; `crates/andromeda-catalog/tests/catalog_store_contract/mutation_wal.rs:320` and `:364` prove append or flush failure remains rollback/fail-closed and all-or-nothing with no visible half-catalog. |
| Recovery anomaly coverage | `crates/andromeda-catalog-recovery/tests/mutation_replay_anomalies.rs:154`, `:241`, `:326`, and `:360` reject incomplete apply tails, reordered apply records, stale batch hashes, duplicate apply indexes, and sparse apply indexes. |

## Executable Commands

```powershell
cargo test -p andromeda-definition-batch --test alter_drop_compat --locked -- --nocapture
cargo test -p andromeda-definition-batch --test definition_batch_contract --locked -- --nocapture
cargo test -p andromeda-catalog-recovery --test mutation_replay_anomalies --locked -- --nocapture
```

Person 14 focused validation on the owned publication runtime contract:

```powershell
cargo test -p andromeda-catalog-recovery --test catalog_publication_subscription_runtime_contract --locked -- --nocapture
```

Result: passed, 10 tests.

## Release Gate

Go only if DefinitionBatch apply evidence remains ordered, dense, hash-bound, and replay-rejectable when the stream is incomplete, duplicated, sparse, reordered, or hash-tampered. A dry-run report alone is not sufficient publication evidence.
