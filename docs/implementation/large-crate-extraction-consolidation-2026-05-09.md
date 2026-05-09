# Large Crate Extraction Consolidation

Last refreshed: 2026-05-09.

This file is the implementation ledger for the targeted extraction of
`andromeda-transaction`, `andromeda-storage`, `andromeda-quic`,
`andromeda-observe`, and `andromeda-catalog`.

## Current State

| Crate | Source files before | Source files now | Test files before | Test files now |
| --- | ---: | ---: | ---: | ---: |
| `andromeda-transaction` | 26 | 25 | 37 | 32 |
| `andromeda-storage` | 61 | 60 | 24 | 18 |
| `andromeda-quic` | 38 | 38 | 20 | 17 |
| `andromeda-observe` | 48 | 47 | 15 | 15 |
| `andromeda-catalog` | 34 | 33 | 17 | 10 |

The tracked diff for this pass removes substantially more code from the five
target crates than it adds there. The additions are concentrated in owner
crates: `andromeda-transaction-log`, `andromeda-recovery`,
`andromeda-wal`, `andromeda-hadr`, `andromeda-storage-heap`,
`andromeda-storage-index`, `andromeda-audit`, `andromeda-observability`,
`andromeda-rpc-codec`, `andromeda-rpc-protocol`,
`andromeda-procedure-contract`, `andromeda-catalog-store`,
`andromeda-catalog-recovery`, and `andromeda-definition-batch`.

## Completed Work

### Transaction

- Moved savepoint behavior tests to `andromeda-savepoint`.
- Moved MVCC visibility and snapshot validation tests to `andromeda-mvcc`.
- Moved lock history and strict 2PL owner assertions to `andromeda-locking`.
- Moved transaction WAL replay record mapping and adapter replay errors to
  `andromeda-transaction-log`.
- Deleted the local transaction WAL replay module and retained a thin
  compatibility re-export because recovery still imports replay APIs through
  the transaction facade.
- Reduced transaction tests to manager lifecycle, integration gates,
  compatibility, and orchestration behavior.

### Storage

- Moved commit log tests to `andromeda-wal`.
- Moved WAL shipping reclaimability tests to `andromeda-hadr`.
- Moved heap/product-stock and heap redo payload contracts to
  `andromeda-storage-heap`.
- Moved BTree durable promotion contracts to `andromeda-storage-index`.
- Moved undo chain types and recovery completeness redo/undo contracts to
  `andromeda-recovery`.
- Removed unused storage dev-dependencies after test migration.
- Kept storage compatibility facades for WAL, recovery, publication, heap,
  index, and layout where active tests or callers still prove usage.

### QUIC

- Moved runtime boundary checks to `andromeda-cli` and
  `andromeda-quic-runtime-quinn`.
- Moved catalog manifest codec/projection tests to `andromeda-rpc-codec`.
- Moved procedure execute protobuf decode assertions to `andromeda-rpc-codec`.
- Moved manifest permission contract assertions to
  `andromeda-procedure-contract`.
- Deleted obsolete QUIC procedure route tests that only covered owner contract
  behavior.
- Kept QUIC focused on connection lifecycle, surface planes, stream
  concurrency, reconnect, 0-RTT policy, and runtime-free route admission.

### Observe

- Moved durable-audit DTO validation into `andromeda-audit`.
- Moved runtime-free exporter DTOs and protocol trace DTOs to
  `andromeda-observability`.
- Removed the local observe exporter DTO module after adding compatibility
  re-exports.
- Kept `EventEnvelope`, `EventSink`, `EventEmitter`,
  `InMemoryEventSink`, validation, correlation, and runtime sink behavior in
  `andromeda-observe`.
- Kept observe storage/page dev-dependencies because IO placement tests still
  use those owners directly.

### Catalog

- Moved procedure digest/contract assertions to
  `andromeda-procedure-contract`.
- Moved store manifest, publication receipt, snapshot gate, and store boundary
  assertions to `andromeda-catalog-store`.
- Moved alter/drop compatibility behavior to `andromeda-definition-batch`.
- Moved mutation replay anomaly tests and generic recovery apply helpers to
  `andromeda-catalog-recovery`.
- Deleted catalog-side recovery decode facade and old batch compatibility
  test suite.
- Kept `CatalogSnapshot`, `CatalogSystemStore`, and DDL migration tests in
  `andromeda-catalog` because they still bind catalog publication, WAL, and
  snapshot semantics.

## Documentation Alignment

- Updated release gate and runbook commands that still pointed at old storage
  or QUIC test locations.
- Recovery completeness and file WAL recovery commands now point at
  `andromeda-recovery`.
- WAL durability and retention commands now point at `andromeda-wal`.
- HA/DR, backup, restore, protocol, and RPC codec gates now point at their
  owner crates.

## Compatibility Retained

- `andromeda-transaction` still re-exports transaction WAL replay APIs until
  recovery callers import `andromeda-transaction-log` directly.
- `andromeda-storage` still re-exports recovery undo, WAL, heap, index,
  publication, and layout APIs while compatibility tests prove active callers.
- `andromeda-observe` still re-exports audit and observability DTOs where
  runtime event APIs expose them.
- `andromeda-catalog` still owns snapshot/system store surfaces and DDL
  migration tests.
- No new crate was created in this pass.

## TODO

- Transaction:
  - Migrate recovery callers from `andromeda-transaction` replay facade to
    `andromeda-transaction-log`.
  - After caller migration, delete transaction WAL replay compatibility
    re-exports and rerun topology gates.

- Storage:
  - Continue caller migration away from storage WAL, heap, index, and recovery
    facades toward owner crates.
  - Split remaining storage tests between true storage orchestration and owner
    crates when `api_compat_reexports`, layout, placement, and publication
    compatibility no longer prove active use.

- QUIC:
  - Keep procedure route tests in QUIC only for transport admission and surface
    separation.
  - Move any future manifest/protobuf assertions directly to RPC/procedure
    owners.

- Observe:
  - Move IO placement and GPU policy tests out of observe after the event
    correlation layer no longer owns their runtime wiring.
  - Remove storage/page dev-dependencies only after those tests are fully
    transferred.

- Catalog:
  - Extract the `DefinitionBatchPlan` report boundary before moving DDL
    migration tests.
  - Migrate remaining publication/WAL tests into catalog-store or
    catalog-recovery once snapshot mutation surfaces are owner-neutral.

## Suggested Next Worker Boundaries

- W01: transaction replay facade removal after recovery import migration.
- W02: storage WAL facade caller migration and `api_compat_reexports`
  reduction.
- W03: storage layout/publication facade reduction.
- W04: observe IO placement and GPU tests to storage/hardware owners.
- W05: catalog DDL migration boundary extraction into definition-batch and
  catalog-recovery.
- W06: validation coordinator for topology, orphan invariants, and workspace
  check after W01-W05.

## Validation Run In This Pass

- `git diff --check`
- `cargo test -p andromeda-transaction -p andromeda-savepoint -p andromeda-mvcc -p andromeda-locking -p andromeda-transaction-log -p andromeda-recovery --tests --locked`
- `cargo test -p andromeda-storage -p andromeda-wal -p andromeda-hadr -p andromeda-storage-heap -p andromeda-storage-index -p andromeda-recovery --tests --locked`
- `cargo test -p andromeda-quic -p andromeda-quic-runtime-quinn -p andromeda-rpc-codec -p andromeda-rpc-protocol -p andromeda-proto-wire -p andromeda-procedure-contract --tests --locked`
- `cargo test -p andromeda-observe -p andromeda-audit -p andromeda-observability -p andromeda-hardware --tests --locked`
- `cargo test -p andromeda-catalog -p andromeda-catalog-store -p andromeda-catalog-recovery -p andromeda-definition-batch -p andromeda-procedure-contract --tests --locked`

Final workspace gates are tracked separately in the coordinating run output.
