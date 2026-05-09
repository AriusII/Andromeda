# Large Crate Extraction Consolidation

Last refreshed: 2026-05-09.

This file is the implementation ledger for the targeted extraction of
`andromeda-transaction`, `andromeda-storage`, `andromeda-quic`,
`andromeda-observe`, and `andromeda-catalog`.

## Current State

| Crate | Source files at targeted baseline | Source files now | Test files at targeted baseline | Test files now |
| --- | ---: | ---: | ---: | ---: |
| `andromeda-transaction` | 26 | 16 | 37 | 24 |
| `andromeda-storage` | 61 | 17 | 24 | 8 |
| `andromeda-quic` | 38 | 38 | 20 | 17 |
| `andromeda-observe` | 48 | 42 | 15 | 15 |
| `andromeda-catalog` | 34 | 23 | 17 | 9 |
| `andromeda-storage-placement` | 0 | 17 | 0 | 3 |

The current tracked extraction pass removes 4,546 lines and adds 2,056 lines
across the targeted owner set. The largest concrete reduction is
`andromeda-storage`: it is now a narrow compatibility/orchestration crate with
placement, operational profile, recovery, WAL, heap, index, and publication
logic moved or delegated to owner crates.

Workspace topology now tracks 89 crates. The new crate is
`andromeda-storage-placement`.

## Completed Work

### Transaction

- Moved the concrete commit log manager, replay reconstruction, validation,
  status storage, and unit tests into `andromeda-transaction-log`.
- Added `TransactionStatusStore` and `TransactionLogStatus` in
  `andromeda-transaction-log` so MVCC/status code can avoid pulling manager
  internals through `andromeda-transaction`.
- Moved commit log durability and gate tests to `andromeda-transaction-log`.
- Reduced `andromeda-transaction/src/commit_log.rs` to a compatibility adapter
  over the transaction-log owner.
- Kept transaction focused on lifecycle, allocation, state machine,
  `TransactionManager`, lock/savepoint orchestration, and integration gates.

### Storage

- Moved startup/recovery planning, fast/safe/forensic start, WAL replay,
  recovery traces, and file WAL recovery reports into `andromeda-recovery`.
- Collapsed `andromeda-storage/src/file_wal.rs` and
  `andromeda-storage/src/recovery.rs` to compatibility re-exports.
- Deleted the storage-owned recovery module tree.
- Demolished pure WAL facade files under `storage/src/write_ahead_log` and
  kept only storage-specific file/durability/record compatibility that is still
  exercised.
- Created `andromeda-storage-placement` and moved placement policy, IO budgets,
  operational profiles, cold publication planning, and owner tests into it.
- Removed `andromeda-storage`'s production dependency on
  `andromeda-storage-placement`; active callers now import the owner crate
  directly.
- Moved cold publication tests to `andromeda-storage-placement` and deleted the
  storage-side placement/profile/publication test suites.
- Kept only storage tests that still validate storage compatibility,
  page/layout API shape, heap replay integration, recovery gates, and WAL
  ownership invariants.

### Catalog

- Moved the concrete snapshot state and mutation application behavior into
  `andromeda-catalog-store`.
- Reduced `andromeda-catalog::CatalogSnapshot` to a compatibility wrapper over
  the store-owned snapshot type.
- Moved `DefinitionBatchPlan`, DDL migration reports, and SRPL dry-run
  integration into `andromeda-definition-batch` /
  `andromeda-srpl-definition-batch`.
- Deleted catalog batch stubs and old catalog-side DDL migration tests.
- Collapsed catalog recovery and WAL record facades so durable payload codecs
  and replay anomaly tests live in `andromeda-catalog-recovery`.
- Kept `CatalogSystemStore`, local mutation enum compatibility, and the
  catalog recovery apply target where cycles still make a full move unsafe.

### Observe

- Moved runtime-free query filters and exporter contracts into
  `andromeda-observability`.
- Moved durable audit query/permission DTO usage toward `andromeda-audit`.
- Reduced observe query/exporter modules to re-export and runtime mapping
  surfaces.
- Removed the broad observe dev-dependency on `andromeda-storage`; observe now
  dev-depends only on `andromeda-storage-placement` and
  `andromeda-storage-page` for IO pipeline event correlation tests.
- Kept `EventEnvelope`, `EventSink`, `EventEmitter`, `InMemoryEventSink`,
  event validation, and runtime correlation behavior in `andromeda-observe`.

### QUIC

- Previous extraction remains in place: runtime boundary checks live in
  `andromeda-cli` / `andromeda-quic-runtime-quinn`, and manifest/protobuf
  assertions live in RPC/procedure owners.
- No additional QUIC source move was needed in this pass.

### Topology And Doctrine

- Added `andromeda-storage-placement` to workspace membership and workspace
  dependency doctrine.
- Updated topology and orphan gates so `andromeda-exec` can depend directly on
  `andromeda-storage-placement`.
- Replaced the old observe dev back-edge exception
  `observe -> storage` with the narrower temporary exception
  `observe -> storage-placement`.

## Compatibility Retained

- `andromeda-transaction` still exposes a commit-log compatibility adapter
  until all active callers import `andromeda-transaction-log` directly.
- `andromeda-storage` still re-exports selected recovery, file WAL, root WAL,
  heap, index, layout, manifest, and cold-store types where compatibility tests
  prove active use.
- `andromeda-storage` no longer re-exports placement policy or operational
  profile APIs.
- `andromeda-observe` still re-exports query/exporter DTO surfaces where
  runtime event APIs expose them.
- `andromeda-catalog` still owns `CatalogSystemStore`, the compatibility
  `CatalogSnapshot` wrapper, and local mutation enum methods.

## Remaining TODO

### Transaction

- Migrate all remaining callers from `andromeda_transaction::CommitLogManager`
  to `andromeda_transaction_log::CommitLogManager`.
- Remove the transaction commit-log adapter once no caller proves active use.
- Re-check recovery and MVCC callers before deleting any transaction-log
  compatibility aliases.

### Storage

- Continue direct caller migration from `andromeda-storage` to:
  `andromeda-wal`, `andromeda-recovery`, `andromeda-storage-heap`,
  `andromeda-storage-index`, `andromeda-storage-page`,
  `andromeda-manifest`, and `andromeda-storage-placement`.
- Remove `write_ahead_log::record` compatibility after the last storage tests
  and callers import `andromeda-wal`.
- Remove `file_wal` and `recovery` re-export facades after CLI/exec/demo
  callers import `andromeda-recovery` directly.
- Remove root `Lsn` and cold-store publication compatibility once callers use
  `andromeda-wal` and `andromeda-manifest`.
- Keep storage tests only for true storage orchestration and compatibility
  gates; do not add new owner behavior tests back into storage.

### Catalog

- Move `CatalogSystemStore` composition once store/recovery/catalog cycles are
  broken.
- Move the local catalog mutation enum or split a neutral mutation contract so
  `wal_record` compatibility methods can disappear.
- Delete the `CatalogSnapshot` wrapper after reverse deps accept
  `andromeda_catalog_store::CatalogSnapshot` directly.

### Observe

- Move IO pipeline correlation tests out of observe after the event mapping can
  be tested from an owner-neutral harness.
- Remove the temporary `observe -> storage-placement` dev back-edge once those
  tests move.
- Keep event runtime and envelope validation in observe; do not move sink or
  emitter runtime behavior to audit/observability unless cycles are resolved.

### QUIC

- Keep QUIC focused on transport lifecycle, stream/concurrency behavior,
  reconnect, 0-RTT policy, and surface-plane admission.
- Move any future manifest/protobuf/procedure assertions directly to
  `andromeda-rpc-codec`, `andromeda-rpc-protocol`,
  `andromeda-proto-wire`, or `andromeda-procedure-contract`.

## Suggested Next Worker Boundaries

- W01: remove transaction commit-log compatibility after caller migration.
- W02: migrate CLI/exec/demo recovery callers from storage file WAL facades to
  `andromeda-recovery`.
- W03: migrate remaining storage WAL record callers to `andromeda-wal` and
  delete the inline `write_ahead_log::record` compatibility module.
- W04: remove storage cold-store/manifest compatibility where callers can use
  `andromeda-manifest` directly.
- W05: break catalog mutation enum compatibility and move durable WAL methods
  fully to `andromeda-catalog-recovery`.
- W06: move observe IO pipeline correlation tests to an owner-neutral harness
  and remove the remaining observe placement dev back-edge.
- W07: final validation coordinator for topology, orphan invariants, diff
  hygiene, and workspace check.

## Validation Run In This Pass

- `cargo fmt --all` could not run on Windows because of path length error 206.
- Package formatting succeeded with targeted `cargo fmt -p ...` commands for
  touched crates.
- `git diff --check`
- `cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture`
- `cargo test -p andromeda-cli --test orphan_source_invariants -- --nocapture`
- `cargo check -p andromeda-exec --all-targets --locked`
- `cargo check --workspace --all-targets --all-features`
- `cargo test -p andromeda-storage -p andromeda-storage-placement --tests --locked`
- `cargo test -p andromeda-observe -p andromeda-observability -p andromeda-audit --tests --locked`
- `cargo test -p andromeda-exec --tests --locked`
- `cargo test -p andromeda-transaction -p andromeda-transaction-log -p andromeda-catalog -p andromeda-catalog-store -p andromeda-catalog-recovery -p andromeda-definition-batch --tests --locked`
