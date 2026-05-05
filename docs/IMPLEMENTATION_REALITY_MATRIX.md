# Implementation Reality Matrix

Date: 2026-05-05

Purpose: classify current Andromeda components by their real implementation
level after the V0.5 cleanup and recent V1.0-oriented work. This is a working
audit document, not a decision record.

## Classification

| Level | Meaning |
| --- | --- |
| Interface only | Public types/traits exist, but no meaningful behavior is implemented. |
| Scaffold | Command or module shape exists and validates inputs, but blocks or delegates to a missing runner. |
| Mocked behavior | Tests or development runtime use explicit mock/in-memory placeholders. |
| In-memory functional | Behavior works against in-memory state with tests. |
| File-backed functional | Behavior persists to files or WAL-like artifacts with tests. |
| Network-backed functional | Behavior uses real network/runtime transport in tests or implementation. |
| Recovery-backed functional | Crash/restart/replay semantics are modeled and tested. |
| Production-ready | Runtime is durable, observable, bounded, integrated, and not scaffolded. |

## Component Matrix

| Component | Current level | Evidence in tree | Remaining blocker |
| --- | --- | --- | --- |
| Workspace root governance | In-memory/documentation functional | Root is reduced to crates, docs, instructions, registries, schemas, workflows, Cargo files, README, PDF. | Keep generated reports out of root; finish docs/decisions audit. |
| `andromeda-core` IAM primitives | In-memory functional | `principal.rs`, `principal_integration.rs`, certificate fingerprint derivation, permission sets, clocks. | Clarify `SessionToken` as audit evidence vs auth credential; increase principal derivation space for V1. |
| `andromeda-proto` schema governance | File-backed functional | Versioned `.proto` files under `crates/andromeda-proto/proto`, descriptor/build tests, generated validation. | Split generated wrapper from validation as code grows; formal migration policy. |
| Catalog server boundary | Mocked behavior | `crates/andromeda-catalog/src/server.rs` exposes `CatalogServerTrait` and `MockCatalogServer`. | Wire to real catalog store and QUIC/proto request path. |
| Catalog DefinitionBatch/WAL | In-memory/recovery contract functional | `batch/*`, `wal_record/*`, catalog tests. | Full migration execution/recovery and schema lock integration. |
| Catalog DDL migration evidence | In-memory functional | `batch/ddl_migration.rs`, `tests/catalog_ddl_migration.rs`. | Decide whether V1 owns execution, rollback, and data-bearing migrations. |
| Storage WAL codec/scan | File-backed/recovery contract functional | `file_wal/*`, `wal_codec/*`, WAL scan/recovery tests. | Integrate all heap/BTree redo records and manifest format fingerprints. |
| Storage buffer pool | In-memory functional | `buffer_pool/*`, dirty/pin/WAL durability modules. | Prove real disk flush and WAL durability fence in integration tests. |
| Storage heap page | In-memory functional | `heap.rs`, heap page/delete/insert tests. | Golden page bytes, CRC/torn-write validation, WAL redo records. |
| Storage row encoder | In-memory functional | `heap_row_encoder.rs`. | Golden bytes across schemas and rejection of malformed variable offsets. |
| Storage B-Tree key codec | In-memory functional | `btree_key_codec.rs`, key codec tests. | Full B-Tree node invariants, split/merge/range/recovery tests. |
| Storage B-Tree engine | In-memory functional, needs audit | `btree.rs`, `btree_engine.rs`, property tests, concurrency contract. | Verify any removed invariant/e2e suites were replaced by equivalent coverage. |
| Storage recovery format gate | Recovery contract functional | `format_version.rs`, `recovery/startup.rs`, DEC-032. | Store observed fingerprints in durable manifest before redo. |
| `andromeda-tx` WAL boundary | In-memory functional | `wal_adapter.rs`, local `lsn.rs`, commit log tests. | Durable rollback, WAL-sourced recovery, commit idempotence under concurrency. |
| `andromeda-tx` locks/deadlock/GC | In-memory functional | lock manager/protocol/history, deadlock detection, gc modules/tests. | Consolidate module naming and prove integration with storage/catalog locks. |
| `andromeda-quic` frame/stream contract | Network-backed contract functional | frame codec/sequence/protocol tests, `real_quinn_network.rs`. | Reconnect runtime, pooling, large result stream flow control. |
| `andromeda-quic` reconnect | Scaffold/contract functional | `reconnect.rs` policy and tests. | Wire to Quinn runtime and request retry semantics. |
| `andromeda-exec` admission/IAM | In-memory functional | admission, principal resolver/evaluator, pre-transaction audit tests. | Durable runtime binding for all rejection/audit paths. |
| `andromeda-exec` SRPL dispatch | Interface/in-memory functional | `srpl_dispatch.rs`, executor validation gates. | Replace interface-only metadata tests with real metadata extraction and multi-procedure behavior. |
| `andromeda-srpl` compiler/parser | In-memory functional | parser/compiler/lowering/interpreter modules and fuzz/property tests. | Optimizer, richer diagnostics, full result metadata integration. |
| `andromeda-cli` admin commands | Scaffold to in-memory functional | HADR/backup/restore/catalog/benchmark command modules and CLI tests. | Bind commands to real runtime services rather than mocks/scaffolds. |
| CLI diagnostic JSON | In-memory functional | `cmd_machine_output.rs` helpers and tests. | Centralize all command-specific diagnostic JSON output under one helper module. |
| `andromeda-bench` | Contract functional | `crates/andromeda-bench` bounded workloads, budgets, evidence tests. | Connect `cmd_benchmark run` behind an explicit feature or runtime runner. |
| `andromeda-observe` audit events | In-memory/durable contract functional | `events/durable_audit.rs`, query contract, audit tests. | Add actual durable sink implementation and restart query evidence. |
| Audit query | In-memory functional | `query.rs` over `InMemoryEventSink`. | File/WAL-backed query source and CLI query command. |

## Decision Record Audit

Current decision records are compact and mostly sequential:

- `DEC-011` to `DEC-032` are present under `docs/decisions/`.
- Suffix records such as `DEC-020b`, `DEC-022b`, and `DEC-024b` document scoped protocol/stream additions rather than duplicate numbers.
- Newer implementation areas now present in code but not fully reflected as durable decisions include benchmark runner ownership, CLI diagnostic JSON policy, reconnect/pool policy, durable audit sink implementation, and transaction rollback recovery.

Recommended follow-up:

| Action | Target |
| --- | --- |
| Add or update a decision for CLI diagnostic JSON boundaries. | Avoid repeated debate about `serde_json` vs manual diagnostic helpers. |
| Add a decision for benchmark runner ownership. | Clarify `andromeda-bench` vs CLI feature gating. |
| Extend DEC-032 or add a child decision for durable format manifest storage. | The recovery gate needs a manifest source before redo. |
| Add a transaction recovery decision update. | Make commit/rollback recovery from WAL explicit. |
| Add QUIC reconnect/pool decision. | Separate policy contract from Quinn runtime behavior. |

## Highest-Risk Reality Gaps

1. CLI surfaces can look complete while still returning mocks or blocked scaffold errors.
2. SRPL dispatcher tests still need to prove real metadata extraction and real multi-procedure execution.
3. TX commit/rollback visibility must be recoverable from WAL, not only memory.
4. Heap and B-Tree page changes must become redo-idempotent before V1.
5. Buffer pool flush must be fenced by durable WAL LSN before disk writes.
6. Durable audit currently has strong contracts, but the sink/query path must survive restart.
