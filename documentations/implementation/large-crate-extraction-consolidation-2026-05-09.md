# Large Crate Extraction Consolidation - 2026-05-09

## Purpose

Consolidate the read-only analysis of the large Andromeda crates named in the work order and turn it into an execution-ready extraction map.

The goal is not to keep the large crates as permanent facades. The goal is to identify source files and behavior groups that can be moved into the existing owner crates, migrate call sites to those owner crates, and then remove compatibility facades once the workspace proves that no caller still needs them.

## Scope

This plan covers the ten large crates named by the owner: `andromeda-catalog`, `andromeda-core`, `andromeda-storage`, `andromeda-tx`, `andromeda-srpl`, `andromeda-observe`, `andromeda-exec`, `andromeda-bench`, `andromeda-proto`, and `andromeda-quic`.

The read-only analysis also treats `andromeda-proto-wire` as the eleventh practical extraction surface, because the wire validation file is large and sits on the same RPC/proto/QUIC boundary.

The current local workspace has 94 Cargo packages according to `cargo metadata --no-deps --format-version 1`. Several older documents still mention 96 crates and should be reconciled after the boundary work stabilizes.

## Non-goals

- Do not introduce an application-facing SQL surface.
- Do not bypass typed Procedure contracts.
- Do not move commit, WAL, rollback, MVCC visibility, catalog publication, or recovery behavior without owner tests and crash/recovery evidence.
- Do not treat benchmark, analytics, GPU, RAM, regression, scenario, or temporary state as database truth.
- Do not make a generic `common`, `utils`, `misc`, or broad integration crate.
- Do not keep compatibility facades as a final architecture. They are allowed only as temporary compile-stabilization steps with explicit removal criteria.
- Do not run execution moves in this read-only analysis phase. Code moves start only after the owner explicitly starts execution.

## Facade Removal Policy

The extraction strategy is intentionally two-phase:

1. Move files and tests into owner crates, temporarily preserving legacy paths only when needed to keep the workspace compiling.
2. Migrate imports and dependency edges to owner crates, then delete the legacy facade modules and source files.

Every temporary facade must have:

- A named owner crate.
- A caller migration target.
- A search command proving whether legacy imports remain.
- Owner-level tests that do not import through the large source crate.
- A final cleanup TODO that deletes the facade.

## Current Evidence

- The topology gate reports 94 workspace packages. Several older documents still mention 96 crates and should be reconciled after the boundary work stabilizes.
- Rust analyzer is loaded for the workspace and reports one workspace rooted at `C:\Users\Arius\RustroverProjects\Andromeda`.
- The workspace is heavily dirty because the write extraction wave moved files, deleted old facades, and updated owner crates. Treat this document as a consolidation for the current branch state, not as an accepted clean baseline.
- The current write wave used seven worker execution slices covering disk page storage, catalog recovery, transaction downstream/tests, SRPL facade reduction, proto structured/wire contracts, observe/audit/decision trace, and bench cleanup.
- Validation now writes into `target/`; successful gates are recorded in the "Write Wave Validation" section.

## Large Crate Size Baseline

These counts were taken from the current workspace by recursively counting Rust files under each crate directory, including crate-local tests.

| Crate | Rust files | Rust lines |
| --- | ---: | ---: |
| `andromeda-storage` | 363 | 46835 |
| `andromeda-exec` | 199 | 24605 |
| `andromeda-catalog` | 121 | 17077 |
| `andromeda-observe` | 119 | 15182 |
| `andromeda-quic` | 83 | 11541 |
| `andromeda-tx` | 105 | 11205 |
| `andromeda-srpl` | 58 | 7612 |
| `andromeda-proto` | 65 | 7125 |
| `andromeda-core` | 42 | 4257 |
| `andromeda-bench` | 35 | 3741 |
| `andromeda-proto-wire` | 7 | 2259 |

## Dependency Topology Snapshot

Observed high-level Cargo edges for the analyzed surfaces:

- `andromeda-catalog` currently depends on owner crates such as `andromeda-catalog-store`, `andromeda-catalog-recovery`, `andromeda-contract`, `andromeda-definition-batch`, `andromeda-plan-cache`, `andromeda-procedure-store`, `andromeda-procedure-contract`, `andromeda-scenario-evidence`, and `andromeda-statistics`. Main incoming crates: `andromeda-cli`, `andromeda-exec`, `andromeda-srpl`.
- `andromeda-core` is still a broad temporary foundation facade. Main incoming crates include durable kernel, security, RPC, execution, transaction, and CLI crates.
- `andromeda-storage` already depends on most storage owner crates: `andromeda-backup`, `andromeda-buffer-pool`, `andromeda-disk-page-store`, `andromeda-manifest`, `andromeda-recovery`, `andromeda-restore`, `andromeda-segment`, `andromeda-storage-heap`, `andromeda-storage-index`, `andromeda-storage-page`, and `andromeda-wal`. Main incoming crates include `andromeda-catalog`, `andromeda-exec`, `andromeda-observe`, `andromeda-cli`, `andromeda-admission`, and `andromeda-business-fixtures`.
- `andromeda-tx` already depends on the owner crates `andromeda-transaction`, `andromeda-transaction-log`, `andromeda-mvcc`, `andromeda-locking`, and `andromeda-savepoint`. Main incoming crates: `andromeda-exec`, `andromeda-execution-trace`.
- `andromeda-srpl` already depends on all SRPL owner crates plus `andromeda-catalog`, `andromeda-definition-batch`, `andromeda-optimizer`, and `andromeda-procedure-runtime`. Main incoming crates: `andromeda-bench`, `andromeda-cli`, `andromeda-exec`.
- `andromeda-observe` depends on `andromeda-audit`, `andromeda-observability`, `andromeda-core`, `andromeda-hardware`, and `andromeda-storage` as a dev/test harness. It is still a broad observability and durable audit hub.
- `andromeda-exec` depends on procedure runtime, result stream, SRPL, storage, transaction, QUIC, catalog, admission, security, IAM, audit, retry, and trace crates. It is the main integration crate to reduce after owner crates stabilize.
- `andromeda-bench` depends on bench harness/workload plus advisory evidence, regression, SRPL, WAL, observe, and storage-page crates. It must stay leaf/advisory.
- `andromeda-proto` depends on `andromeda-proto-wire`, contract-safe model crates, and generated Protobuf tooling. Main incoming crates include `andromeda-admission`, `andromeda-catalog`, `andromeda-exec`, `andromeda-protocol`, `andromeda-quic`, and `andromeda-result-stream`.
- `andromeda-proto-wire` depends on `andromeda-rpc-protocol`, `andromeda-procedure-contract`, `andromeda-structured-object`, and foundation types. Main incoming crates include `andromeda-proto`, `andromeda-protocol`, `andromeda-rpc`, and `andromeda-rpc-codec`.
- `andromeda-quic` depends on abstract protocol/security crates, not `andromeda-hadr`. Main incoming crates include `andromeda-cli`, `andromeda-exec`, and `andromeda-quic-runtime-quinn`.

## Completed Stabilization Slice

- Moved `ExtentId` ownership to `andromeda-storage-page` and reexported it from `andromeda-storage` extent compatibility paths.
- Exposed `andromeda-segment::segment_index` through `src/segment_index/mod.rs`.
- Converted `andromeda-storage::segment_index` into a compatibility facade over `andromeda-segment::segment_index`.
- Updated the `andromeda-segment` segment-index contract test to import the owner crate directly.
- Added the missing `sha2` dependency to `andromeda-segment` for segment-index digesting.
- Added a runtime-free `CompletionProtocolVersion` adapter type in `andromeda-rpc-protocol` so Procedure completion validation can use protocol-owned versions without coupling `andromeda-procedure-contract` to wire crates.
- Cleaned stale target artifacts that caused false `E0463` crate resolution failures after dependency and module movement.

## Completed Extraction Slice

- Moved `andromeda-storage/src/extent/*` into `andromeda-segment/src/extent/*`.
- Moved `andromeda-storage/src/segment.rs` into `andromeda-segment/src/descriptor.rs`.
- Converted `andromeda-storage::extent` and `andromeda-storage::segment` into compatibility facades over `andromeda-segment`.
- Moved `andromeda-tx/src/commit_log/manager.rs`, `andromeda-tx/src/commit_log/manager/*`, and commit-log tests into `andromeda-transaction`.
- Removed duplicate local `andromeda-tx/src/lock_manager/*` implementation files after confirming ownership belongs to `andromeda-locking`.
- Converted `andromeda-tx::commit_log` and `andromeda-tx::lock_manager` into compatibility facades over owner crates.
- Moved the catalog plan-cache ScenarioEvidence bridge from `andromeda-catalog/src/plan_cache/*` into `andromeda-scenario-evidence/src/plan_cache_bridge/*`.
- Converted `andromeda-catalog::plan_cache` into a compatibility facade over `andromeda-scenario-evidence`.
- Moved `andromeda-quic/src/hadr_streams.rs` and `andromeda-quic/src/hadr_streams/*` into `andromeda-hadr`.
- Deleted the `andromeda-quic::hadr_streams` compatibility facade. HA/DR stream mapping coverage remains in `andromeda-hadr`; QUIC keeps only local reserved-range gate constants for Application route separation and does not depend on `andromeda-hadr`.
- Moved SRPL result metadata extraction from `andromeda-exec/src/result_metadata_extractor.rs` into `andromeda-procedure-runtime/src/result_metadata_extractor.rs`.
- Converted `andromeda-exec::result_metadata_extractor` into a compatibility facade over `andromeda-procedure-runtime`.
- Moved `InvocationTrace`, `MvccTrace`, and `ResourceTrace` ownership from `andromeda-observe/src/events/core_trace.rs` into `andromeda-observability/src/core_trace.rs`.
- Converted the `andromeda-observe` event path for core traces into a compatibility facade over `andromeda-observability`.
- Moved Procedure Store evidence role, registration, runtime counters, and runtime status from `andromeda-catalog` into `andromeda-procedure-store`.
- Converted the extracted Procedure Store paths in `andromeda-catalog` into owner-crate imports and compatibility reexports where needed.
- Moved remaining Procedure feedback records and in-memory feedback store from `andromeda-catalog` into `andromeda-procedure-store`.
- Converted `andromeda-catalog::procedure_feedback` into a compatibility facade over `andromeda-procedure-store`.
- Moved statistics publication evidence, trace, switch, and switch error ownership from `andromeda-catalog` into `andromeda-statistics`.
- Converted `andromeda-catalog::statistics::publication` into a compatibility facade over `andromeda-statistics`.
- Moved the deterministic `StorageFormatManifest` hash primitive from `andromeda-storage/src/manifest/hash.rs` into `andromeda-manifest/src/storage_format_hash.rs`.
- Kept `andromeda-storage` responsible only for adapting storage-local fingerprints into the manifest hash primitive.
- Moved runtime-free manifest database, format, format-version, and snapshot primitives from `andromeda-storage` into `andromeda-manifest`.
- Converted `andromeda-storage::manifest` and `andromeda-storage::format_version` into compatibility facades over `andromeda-manifest`, with storage-local publication adapters still local.
- Moved backup checkpoint manager ownership from `andromeda-storage` into `andromeda-backup`.
- Converted `andromeda-storage::backup::checkpoint_manager` into a compatibility facade over `andromeda-backup`.
- Moved transaction allocator, commit protocol, transaction manager, manager core, lock coordinator, records, and manager tests from `andromeda-tx` into `andromeda-transaction`.
- Converted `andromeda-tx` allocator, commit protocol, and manager paths into compatibility reexports over `andromeda-transaction`.
- Split `andromeda-proto-wire/src/generated_validation.rs` into domain modules under `andromeda-proto-wire/src/generated_validation/*`, preserving the public module surface.
- Moved runtime-free principal permission, surface-scope, and security contract vocabulary from `andromeda-core` into `andromeda-security-contract`.
- Converted the extracted `andromeda-core::principal` paths into compatibility facades over `andromeda-security-contract`.
- Moved benchmark CRUD data, metrics, and scenario contracts from `andromeda-bench` into `andromeda-bench-workload`.
- Moved benchmark runner counters, latency evidence, and synthetic latency utilities from `andromeda-bench` into `andromeda-bench-harness`.
- Converted `andromeda-bench` CRUD and runner paths into compatibility facades over the owner crates where callers still need historical imports.
- Moved runtime-free SRPL DefinitionBatch bridge diagnostics and source evidence from `andromeda-srpl` into `andromeda-definition-batch/src/srpl_bridge.rs`.
- Converted the extracted SRPL DefinitionBatch bridge paths in `andromeda-srpl` into compatibility reexports while keeping SRPL compiler/catalog orchestration local.

## Completed Write Extraction Wave - 2026-05-09

Storage and disk page store:

- Moved `andromeda-storage/src/disk_manager/{atomic_write,error,extent_map,file,integrity,interface,page_store}.rs` into `andromeda-disk-page-store/src/`.
- Moved disk-manager durability/integration tests from `andromeda-storage/tests` into `andromeda-disk-page-store/tests`.
- Reduced `andromeda-storage/src/disk_manager/mod.rs` to the remaining storage-facing compatibility boundary.
- Removed the new disk-page-store dependency on `andromeda-core`; it now uses `andromeda-error` plus storage/segment/WAL owner crates.

Catalog and definition batch:

- Moved catalog WAL record DTOs and replay report DTOs into `andromeda-catalog-recovery`.
- Moved `DefinitionBatch` ownership into `andromeda-definition-batch`.
- Reduced catalog batch/WAL/recovery type paths to adapters over owner crates where the live `CatalogSnapshot` or `CatalogMutationPlan` still makes catalog the correct runtime owner.

Transaction:

- Moved MVCC tests to `andromeda-mvcc`, deadlock/locking tests to `andromeda-locking`, pure savepoint tests to `andromeda-savepoint`, and commit/WAL/manager/2PL/savepoint integration tests to `andromeda-transaction`.
- Reduced `andromeda-tx` to owner reexports plus the remaining compatibility test.
- `andromeda-execution-trace` imports transaction owner crates directly.
- `andromeda-exec` currently keeps the topology-approved `andromeda-tx` boundary because the dependency doctrine does not yet allow direct `exec -> andromeda-transaction`, `exec -> andromeda-transaction-log`, or `exec -> andromeda-mvcc` runtime edges.

SRPL and optimizer:

- Moved catalog-free validation diagnostics into `andromeda-srpl-binder`.
- Moved source enrichment diagnostics into `andromeda-srpl-diagnostics`.
- Moved lexer/parser validation gates into `andromeda-srpl-parser`.
- Deleted old SRPL facade files for lowering diagnostics, lowering validation, procedure model, and source location; the SRPL root keeps only inline compatibility reexports where callers still use the historical path.
- Fixed the optimizer diagnostics contract to use a genuinely foldable emit expression now that `SrplValueIr::bool` is already an IR constant.

Proto, RPC, and QUIC:

- Deleted `andromeda-proto/src/{completion,errors,manifest,structured}.rs` and moved the public surface to owner crate reexports.
- Reduced `andromeda-proto/src/generated_validation/mod.rs` to Prost adapters while runtime-free validation lives in `andromeda-proto-wire`.
- Moved completion envelope version ownership to `andromeda-rpc-protocol` and exposed it through `andromeda-proto-wire`.
- Deleted pure QUIC facades `backpressure.rs`, `protocol_invariants.rs`, and `rpc.rs`; `andromeda-quic` now reexports directly from `andromeda-rpc-protocol` and `andromeda-rpc`.

Observe, audit, decision trace, and bench:

- Moved durable audit DTOs into `andromeda-audit`.
- Moved critical decision vocabulary into `andromeda-observability` so `andromeda-observe` stays inside the allowed foundation/diagnostic boundary; `andromeda-decision-trace` reexports the same type for advisory decision-trace callers.
- Reduced `andromeda-observe` decision and durable-audit event files to boundary adapters where event envelopes still live in observe.
- Moved `CrudWorkloadResult` into `andromeda-scenario-evidence`.
- Moved advisory flat JSON codec into `andromeda-scenario-evidence` and migrated `andromeda-regression` off its duplicate local copy.
- Moved bounded benchmark evidence into `andromeda-bench-harness`; deleted obsolete bench CRUD and flat-json facade files.

## Write Wave Implementation - 2026-05-09

This section materializes the write-worker reports that feed the completed extraction wave above. The reports are append-friendly worker artifacts, not final architecture approval. They record the implementation surface, validation evidence already captured in this document, and the remaining cleanup TODOs that should drive the next extraction wave.

Worker report inventory:

| Worker report | Slice | Status |
| --- | --- | --- |
| `documentations/implementation/worker-storage-disk-page-store-2026-05-09.md` | Storage disk-manager ownership and disk-page-store tests | Implemented; storage keeps a compatibility boundary. |
| `documentations/implementation/worker-catalog-definition-batch-2026-05-09.md` | Catalog WAL/recovery DTOs and DefinitionBatch ownership | Implemented; live catalog snapshot and mutation-plan adapters remain. |
| `documentations/implementation/worker-transaction-downstream-tests-2026-05-09.md` | Transaction owner tests, MVCC/locking/savepoint split, downstream imports | Implemented; `andromeda-exec` still uses the approved `andromeda-tx` boundary. |
| `documentations/implementation/worker-srpl-optimizer-2026-05-09.md` | SRPL diagnostics/parser facade reduction and optimizer diagnostic contract | Implemented; historical root reexports remain where callers still need them. |
| `documentations/implementation/worker-proto-rpc-quic-2026-05-09.md` | Proto structured/wire contracts, RPC protocol ownership, QUIC facade cleanup | Implemented; frame/stream/typed-envelope caller migration remains. |
| `documentations/implementation/worker-observe-audit-decision-trace-2026-05-09.md` | Durable audit DTOs, decision vocabulary, observe adapters | Implemented; event-envelope and principal-binding coupling remain. |
| `documentations/implementation/worker-bench-evidence-cleanup-2026-05-09.md` | Benchmark workload/harness cleanup and advisory evidence movement | Implemented; benchmark compatibility imports and advisory-only gates remain. |
| `documentations/implementation/worker-topology-gates-map-2026-05-09.md` | Topology and orphan-source gate map | Observed companion report; not modified by this consolidation pass. |

Cross-worker implementation notes:

- No compatibility facade should be treated as final. Each remaining facade needs direct caller migration evidence and a deletion TODO.
- The current successful validation set is recorded in "Write Wave Validation"; these reports do not add fresh validation beyond that evidence.
- The next write wave should prefer caller migration and facade deletion before moving deeper C5 behavior.
- Documentation references that still point to read-only worker analysis should be updated after the worker reports settle and no concurrent worker is writing the same sections.

## Write Wave Validation

Successful gates after the write wave:

```powershell
cargo check -p andromeda-disk-page-store -p andromeda-exec -p andromeda-observe -p andromeda-observability -p andromeda-decision-trace -p andromeda-tx --all-targets --all-features
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
cargo test -p andromeda-cli --test orphan_source_invariants -- --nocapture
cargo check --workspace --all-targets --all-features
cargo test -p andromeda-disk-page-store -p andromeda-catalog-recovery -p andromeda-definition-batch -p andromeda-audit -p andromeda-decision-trace --all-targets --all-features
cargo test -p andromeda-mvcc -p andromeda-locking -p andromeda-savepoint -p andromeda-transaction --all-targets --all-features
cargo test -p andromeda-proto -p andromeda-proto-wire -p andromeda-rpc-protocol -p andromeda-bench-workload --all-targets --all-features
cargo test -p andromeda-quic -p andromeda-srpl -p andromeda-srpl-parser -p andromeda-srpl-binder -p andromeda-srpl-diagnostics --all-targets --all-features
```

Known remaining warnings:

- `andromeda-observe/src/principal_binding/bridge.rs` has two unused conversion helpers.
- `andromeda-storage/src/backup/plan.rs` has one unused `validate_wal_segment_chain` helper.
- `cargo fmt --all` still hits Windows `os error 206`; package-scoped `cargo fmt -p ...` was used for modified crates.

## TODO / SUB TODO / DEPENDENCIES

### Storage Kernel

TODO: Reduce `andromeda-storage` from broad C5 owner to storage integration crate, with owner crates holding manifest, disk page store, buffer pool, heap, index, backup, restore, WAL, and recovery behavior.

Candidate moves:

| Source group | Target crate | Notes |
| --- | --- | --- |
| `src/manifest.rs`, `src/manifest/format.rs`, `src/manifest/snapshot.rs`, `src/format_version.rs` | `andromeda-manifest` | Done for runtime-free database, format, snapshot, and format-version primitives; keep storage-local publication adapters local until placement and publication are decoupled. |
| `src/disk_manager/*` | `andromeda-disk-page-store` | Done for file/page-store/atomic-write/extent-map/integrity/contracts and owner tests; storage keeps only the current compatibility boundary. |
| `src/buffer_pool/*` | `andromeda-buffer-pool` | Use `andromeda-storage-page::PageStore`; do not depend on concrete `DiskPageStore`. |
| `src/heap_row_encoder/*`, `src/heap/mod.rs`, `src/write_ahead_log/heap_redo.rs` | `andromeda-storage-heap` | Prefer heap owner for `heap_redo` to avoid `andromeda-wal -> andromeda-storage-page` cycles. |
| `src/btree/*`, `src/btree_format_validation/*` | `andromeda-storage-index` | Keep `btree_key_codec.rs` local until storage `Datum` to index `KeyDatum` conversion is isolated. |
| `src/backup/artifact_store/*`, `checkpoint_manager`, `execution_plan`, `scheduler`, `wal_archive_integration` | `andromeda-backup` | `checkpoint_manager` is moved; artifact execution, execution plan, scheduler, and WAL archive integration remain. |
| `src/restore_orchestration/*` | `andromeda-restore` | Direction should be `restore -> backup`, never `backup -> restore`. |
| `src/write_ahead_log/shipping/*`, `src/write_ahead_log/compaction/*` | `andromeda-wal` or `andromeda-hadr` depending on ownership | Keep concrete HADR shipping separate from WAL record invariants. |
| `src/recovery/*`, `src/file_wal/{recovery,report}.rs` | `andromeda-recovery` | Move only after manifest, heap redo, WAL adapters, and catalog replay traits are owner-ready. |

SUB TODO:

- Move owner tests for extents, segments, page codecs, WAL record bounds, and segment indexes out of `andromeda-storage` before deleting compatibility paths.
- Keep reducing the remaining storage-local manifest adapters; runtime-free manifest database, format, snapshot, and format-version primitives are now in `andromeda-manifest`.
- Migrate remaining callers off `andromeda_storage::disk_manager::*`; disk manager implementation and tests are now owner-owned by `andromeda-disk-page-store`.
- Extract `BufferPoolManager`, guards, dirty tracking, and WAL durability observer traits into `andromeda-buffer-pool`.
- Extract heap row encoders and product-stock row test fixtures into `andromeda-storage-heap` or `andromeda-business-fixtures` depending on whether they are storage format or business fixture data.
- Extract BTree node contracts, in-memory index engine, range cursor, format validation, and tests into `andromeda-storage-index`.
- Extract remaining backup artifact store, filesystem artifact writer, payload cursors, archive digest, execution plan, scheduler, and WAL archive integration into `andromeda-backup`; checkpoint manager is already owner-owned.
- Extract restore checksum, validation, preflight, replay plan, and orchestration DTOs into `andromeda-restore`.
- Extract recovery planning, startup decisions, safe/fast/forensic start, WAL replay driver, catalog replay selection, undo chain, and recovery traces into `andromeda-recovery` only through traits that avoid `andromeda-recovery -> andromeda-storage`.
- Delete `andromeda-storage` facades after `rg "andromeda_storage::(Extent|Segment|Page|Buffer|BTree|Backup|Restore|Recovery)"` shows callers have migrated to owner crates.

DEPENDENCIES:

- Owner crates: `andromeda-storage-page`, `andromeda-wal`, `andromeda-segment`, `andromeda-manifest`, `andromeda-disk-page-store`, `andromeda-buffer-pool`, `andromeda-backup`, `andromeda-restore`, `andromeda-recovery`, `andromeda-storage-heap`, `andromeda-storage-index`.
- Watch edges: `andromeda-storage -> andromeda-core`, `andromeda-storage -> andromeda-observe`, `andromeda-storage -> andromeda-hadr`.
- Required gates: WAL owner tests, page codec tests, segment-index contract tests, manifest recovery floor tests, disk-page atomic write tests, buffer-pool WAL-before-flush tests, backup artifact tests, restore preflight tests, crash/recovery matrix.

Execution order:

1. Stabilize and then remove already-pure facades for segment, page, and WAL owner tests.
2. Manifest and storage format primitives moved; remove facades after call-site migration.
3. Disk page store moved; remove the storage compatibility boundary after caller migration.
4. Move buffer pool.
5. Move heap encoder and heap redo.
6. Move index/BTree.
7. Backup checkpoint manager moved; continue with artifact execution, scheduler, execution plan, and WAL archive integration.
8. Move restore.
9. Move recovery.
10. Reduce `andromeda-storage` to durable storage integration and then remove unused facades.

### Catalog And Procedure Contracts

TODO: Make `andromeda-catalog` a catalog runtime owner, not a holder for every contract, plan, stats, Procedure Store, WAL, and recovery concern.

Candidate moves:

| Source group | Target crate | Notes |
| --- | --- | --- |
| `src/batch/definition.rs`, portable parts of `src/batch/mutation.rs` | `andromeda-definition-batch` and `andromeda-catalog-store` | `DefinitionBatch` moved to `andromeda-definition-batch`; `DefinitionBatchPlan` remains in catalog while it embeds `CatalogMutationPlan`. |
| `src/wal_record/codec.rs`, `src/wal_integration.rs`, recovery-facing mutation records | `andromeda-catalog-recovery` | Runtime-free WAL record/replay report DTOs moved; catalog remains the adapter while replay applies to live catalog snapshots. |
| `src/recovery.rs`, `src/recovery/*` | `andromeda-catalog-recovery` | Use a trait such as `CatalogRecoveryApplyTarget`; do not import `CatalogSnapshot` into recovery. |
| `src/procedure_store/*`, `src/procedure_feedback/*` | `andromeda-procedure-store` | Feedback records/store are moved; finish any remaining Procedure Store runtime and delete facades after imports migrate. |
| `src/statistics/publication/*` | `andromeda-statistics` | Publication evidence/trace/switch/error are moved; keep live catalog activation/version publication in catalog until owner gates pass. |
| `src/publication_subscription/runtime.rs` and generic registry logic | `andromeda-catalog-recovery` or `andromeda-catalog-store` | Keep catalog-specific subscriber IDs and aliases local. |
| Catalog diff tests and Procedure compatibility diff logic | `andromeda-catalog-diff` | Add owner tests and remove catalog-only compatibility coverage after migration. |

SUB TODO:

- Continue replacing local catalog WAL record adapters with `andromeda-catalog-recovery` types where the live catalog snapshot is not required.
- Finish any remaining Procedure Store runtime and tests in `andromeda-procedure-store`; feedback records and feedback store are already owner-owned.
- Finish statistics publication validation and call-site migration; publication switch, evidence, trace, and switch errors are already owner-owned.
- Extract catalog recovery replay through an application trait so recovery does not import catalog runtime state.
- Move owner tests before deleting catalog facades: procedure store contract, stats publication switch, catalog recovery replay, DefinitionBatch dry-run/apply, catalog diff.
- Keep `CatalogSnapshot`, `CatalogSystemStore`, `CatalogServerRuntime`, live snapshot validation, and live catalog publication in `andromeda-catalog` until state ownership is explicitly split.
- Remove `andromeda-catalog -> andromeda-proto` after protocol-facing schema manifests move to contract/proto owner crates.

DEPENDENCIES:

- Owner crates: `andromeda-contract`, `andromeda-procedure-contract`, `andromeda-definition-batch`, `andromeda-procedure-store`, `andromeda-plan-cache`, `andromeda-statistics`, `andromeda-catalog-store`, `andromeda-catalog-recovery`, `andromeda-catalog-diff`, `andromeda-scenario-evidence`.
- Watch edges: `andromeda-catalog -> andromeda-proto`, `andromeda-catalog -> andromeda-observe`, `andromeda-catalog -> andromeda-storage` as dev dependency.
- Required gates: ContractHash golden vectors, DefinitionBatch dry-run/apply tests, catalog WAL replay tests, procedure store tests, plan invalidation tests, statistics publication tests, catalog diff tests.

Execution order:

1. Catalog WAL record/replay report DTO ownership stabilized around `andromeda-catalog-recovery`; remaining work is live replay application decoupling.
2. Procedure feedback moved; finish remaining Procedure Store runtime and facade removal.
3. Statistics publication switch/evidence/trace moved; finish validation and facade removal.
4. Move publication/subscription generic registry behavior.
5. Extract recovery replay behind an apply-target trait.
6. Migrate `andromeda-srpl` and `andromeda-exec` off catalog facades.
7. Delete catalog compatibility facades after owner imports are direct.

### Transaction Kernel

TODO: Convert `andromeda-tx` into a temporary migration shell over owner crates, then remove it from normal execution dependencies where possible.

Candidate moves:

| Source group | Target crate | Notes |
| --- | --- | --- |
| `src/allocator.rs` | `andromeda-transaction` | Done; `andromeda-tx` reexports during migration. |
| `src/manager.rs`, `src/manager/*` | `andromeda-transaction` | Done for manager core, record, lock coordinator, and manager tests; `andromeda-tx` reexports during migration. |
| `src/commit_protocol.rs` | `andromeda-transaction` | Done; durable commit protocol now lives with transaction state. |
| `src/lock_protocol.rs` | `andromeda-locking` or remove if documentation-only | Avoid `andromeda-locking -> andromeda-transaction` cycles. |
| MVCC tests under `andromeda-tx/tests` | `andromeda-mvcc` | Done for MVCC/GC/reclamation/isolation owner tests. |
| Locking/deadlock tests under `andromeda-tx/tests` | `andromeda-locking` | Done for deadlock and lock-manager owner tests. |
| Savepoint tests under `andromeda-tx/tests` | `andromeda-savepoint` and `andromeda-transaction` | Pure write-set tests moved to savepoint; manager-integrated savepoint tests moved to transaction. |
| WAL/replay tests | `andromeda-transaction`, `andromeda-transaction-log`, or execution/recovery bridge crates | Done for current commit/WAL/replay/adapter tests; physical WAL remains outside `andromeda-transaction`. |

SUB TODO:

- Migrate remaining callers off the `andromeda-tx` compatibility reexports for `TransactionIdAllocator`, `TransactionManager`, `TransactionRecord`, `TransactionLockCoordinator`, and `CommitProtocol`.
- Keep the direct owner dependencies in `andromeda-transaction` minimal; it now depends directly on the required locking/savepoint owner crates.
- Keep `InvocationWal` and replay payload traits in `andromeda-transaction-log`; do not make `andromeda-transaction` depend on `andromeda-storage` or physical `andromeda-wal`.
- Keep `andromeda-exec` on the topology-approved `andromeda-tx` boundary until the dependency doctrine allows direct owner edges or an accepted bridge crate is introduced.
- `andromeda-execution-trace` imports transaction owner crates directly; keep it off `andromeda-tx`.
- `andromeda-tx` now has only compatibility reexports plus its API compatibility test; delete it only after downstream topology and call sites no longer need the migration shell.
- Remove `andromeda-tx` from downstream `Cargo.toml` files only after the topology gate permits the replacement edges.

DEPENDENCIES:

- Owner crates: `andromeda-transaction`, `andromeda-transaction-log`, `andromeda-mvcc`, `andromeda-locking`, `andromeda-savepoint`, `andromeda-wal`, `andromeda-recovery`.
- Watch edges: avoid `andromeda-transaction -> andromeda-storage`; avoid `andromeda-locking -> andromeda-transaction` unless the dependency direction is explicitly accepted.
- Required gates: durable commit evidence tests, rollback evidence tests, MVCC isolation/anomaly tests, lock manager tests, savepoint rollback tests, transaction WAL adapter tests, recovery replay tests.

Execution order:

1. Commit protocol moved.
2. Allocator and transaction manager moved.
3. Transaction manager tests moved.
4. MVCC, locking, savepoint, and WAL tests moved to owner crates.
5. `andromeda-execution-trace` migrated off `andromeda-tx`; `andromeda-exec` remains on `andromeda-tx` pending topology redesign.
6. Delete `andromeda-tx` facades only after no downstream runtime crate requires the topology-approved bridge.

### SRPL And Procedure Runtime

TODO: Keep `andromeda-srpl` as a short-lived compatibility crate while moving compiler, diagnostics, execution adapter, and test behavior into owner crates.

Candidate moves:

| Source group | Target crate | Notes |
| --- | --- | --- |
| `andromeda-srpl/src/binder.rs` | `andromeda-srpl-binder` | Already a facade; migrate callers then delete. |
| `andromeda-srpl/src/execution_adapter/mod.rs` | `andromeda-srpl-execution-adapter` | Already a facade; migrate callers then delete. |
| `andromeda-srpl/src/interpreter/mod.rs` | `andromeda-srpl-interpreter` | Already a facade; migrate callers then delete. |
| `andromeda-srpl/src/source_location/mod.rs` | `andromeda-srpl-diagnostics` | File deleted; SRPL root keeps an inline source-location compatibility module over diagnostics owner spans. |
| `andromeda-srpl/src/procedure_model.rs` | `andromeda-srpl-ast`, `andromeda-srpl-cardinality`, `andromeda-srpl-ir` | File deleted; SRPL root keeps inline compatibility reexports while callers migrate. |
| `andromeda-srpl/src/lowering/diagnostics.rs` | `andromeda-srpl-diagnostics` | Done for source enrichment diagnostics. |
| `andromeda-srpl/src/lowering/validation.rs` | `andromeda-srpl-binder` | Done for catalog-free validation diagnostics. |
| `andromeda-exec/src/srpl_adapters/*` | `andromeda-srpl-execution-adapter` | Requires replacing `andromeda_core` imports with narrower `andromeda_error`/foundation crates. |
| `andromeda-exec/tests/metadata_extraction_contract/*` | `andromeda-procedure-runtime` tests | Metadata extractor is already runtime-owned. |
| `andromeda-exec/tests/result_stream_backpressure/*` | `andromeda-result-stream` tests | Result stream owner tests should not import through exec. |
| Exec business test constants/fixtures | `andromeda-business-fixtures` | Move fixtures first; runtime business remains in exec until storage/tx/quic coupling is reduced. |

SUB TODO:

- Continue migrating tests and imports to owner crates before deleting the remaining `andromeda-srpl` root compatibility modules.
- Move metadata extraction tests to `andromeda-procedure-runtime`.
- Move result stream backpressure tests to `andromeda-result-stream`.
- Extract concrete SRPL adapter value/environment/backpressure/transaction-context code from `andromeda-exec` into `andromeda-srpl-execution-adapter` if it can stay free of exec runtime.
- Keep `srpl_dispatch.rs` in `andromeda-exec` unless `andromeda-procedure-runtime` intentionally becomes SRPL-aware; generic runtime should not depend on `andromeda-srpl-interpreter` by accident.
- Keep `definition_batch_bridge/dry_run.rs` and `procedure_definition.rs` in `andromeda-srpl` until a dedicated adapter crate exists; do not make `andromeda-definition-batch` depend on full parser/binder/lowering/catalog.
- Split `andromeda-srpl-binder/src/catalog_plan.rs` into catalog view, operation binding, validation, and fixtures.
- Split `andromeda-srpl-execution-adapter/src/contracts.rs` into row bounds, operation context, request types, and validation.

DEPENDENCIES:

- Owner crates: `andromeda-srpl-lexer`, `andromeda-srpl-parser`, `andromeda-srpl-ast`, `andromeda-srpl-binder`, `andromeda-srpl-ir`, `andromeda-srpl-lowering`, `andromeda-srpl-interpreter`, `andromeda-srpl-diagnostics`, `andromeda-srpl-cardinality`, `andromeda-srpl-execution-adapter`, `andromeda-srpl-test-fixtures`, `andromeda-procedure-runtime`, `andromeda-result-stream`, `andromeda-business-fixtures`.
- Watch edges: avoid `andromeda-srpl-binder -> andromeda-catalog`; avoid `andromeda-definition-batch -> andromeda-srpl` full stack; avoid `andromeda-result-stream -> andromeda-quic` or `-> andromeda-storage`.
- Required gates: parser diagnostics, lowering tests, binder contract tests, SRPL facade compatibility tests, exec SRPL integration tests, result-stream ordering tests, procedure runtime metadata tests.

Execution order:

1. Migrate tests/imports to owner crates without moving logic.
2. Move metadata tests to `andromeda-procedure-runtime`.
3. Move ResultStream tests to `andromeda-result-stream`.
4. Move `andromeda-exec/src/srpl_adapters/*` if dependency cleanup is narrow enough.
5. `andromeda-srpl` is reduced further; remaining compatibility is concentrated in root reexports and catalog/DefinitionBatch bridges.
6. Delete facades after `bench`, `cli`, `exec`, and tests stop importing through `andromeda-srpl`.

### Execution Engine

TODO: Reduce `andromeda-exec` to Procedure orchestration and integration, not ownership of protocol, SRPL language model, result stream, security vocabulary, business fixtures, or transaction semantics.

Candidate moves:

| Source group | Target crate | Notes |
| --- | --- | --- |
| `src/result_metadata_extractor.rs` | `andromeda-procedure-runtime` | Already moved; remove exec facade after caller migration. |
| `src/result_stream.rs`, owner-safe parts of `src/result.rs` | `andromeda-result-stream` | Do not move `result/frames.rs` if it pulls QUIC/storage into result-stream. |
| `src/dispatch/procedure.rs` | `andromeda-procedure-runtime` | Mostly aliases; migrate imports to runtime. |
| `src/services/completion/*` | `andromeda-execution-trace` and/or `andromeda-procedure-runtime` | Keep transaction terminal evidence contract-safe. |
| `src/services/permission_audit_emitter/*` | `andromeda-audit` or `andromeda-security` | Avoid cycles with `observe` and `security` until those dependencies are inverted. |
| `src/srpl_adapters/*` | `andromeda-srpl-execution-adapter` | See SRPL section. |
| `src/wal_evidence.rs` | `andromeda-transaction-log` bridge or execution-trace bridge | Do not make transaction owners depend on exec. |
| Test/business fixtures | `andromeda-business-fixtures` | Move constants/sample data first; keep live executor in exec. |

SUB TODO:

- Migrate direct imports from `andromeda_exec::result_metadata_extractor` to `andromeda-procedure-runtime`.
- Migrate direct imports from `andromeda_exec::result_stream` and owner-safe result metadata to `andromeda-result-stream`.
- Move metadata and result-stream tests to owner crates.
- Keep `andromeda-exec -> andromeda-tx` for now because topology rejects direct `exec -> transaction/mvcc/transaction-log`; use direct owner imports only after topology is redesigned.
- Replace remaining `andromeda-exec -> andromeda-quic` imports only where topology permits it. Current topology rejects a direct `andromeda-exec -> andromeda-rpc-protocol` edge, so frame-only V0 imports still go through the QUIC compatibility surface until an allowed protocol bridge exists.
- Keep concrete local vertical runtime and business executor in `andromeda-exec` until storage/tx/catalog/protocol edges are reduced.
- Extract business fixtures without moving the runtime store/executor if they depend on live WAL/storage/tx.

DEPENDENCIES:

- Owner crates: `andromeda-procedure-runtime`, `andromeda-result-stream`, `andromeda-admission`, `andromeda-iam`, `andromeda-security`, `andromeda-audit`, `andromeda-execution-trace`, `andromeda-srpl-execution-adapter`, `andromeda-transaction`, `andromeda-transaction-log`, `andromeda-rpc`, `andromeda-rpc-codec`, `andromeda-rpc-protocol`, `andromeda-business-fixtures`.
- Watch edges: avoid `procedure-runtime -> exec`; avoid `result-stream -> quic/storage`; avoid `transaction -> exec`; remove `exec -> quic` except explicit runtime transport entrypoints.
- Required gates: admission-before-transaction tests, runtime contract tests, completion audit tests, retry semantics tests, result-stream metadata-before-payload tests, recovery visibility gates.

Execution order:

1. Migrate metadata/result-stream imports and tests to owners.
2. Move SRPL adapters if possible.
3. Transaction owner tests and `execution-trace` imports moved; `exec` remains on `andromeda-tx` pending topology-approved direct edges.
4. Continue migrating non-transport protocol imports off `andromeda-quic` only after dependency topology allows the target edge or an intermediate owner bridge is introduced.
5. Extract business fixtures.
6. Delete exec compatibility facades once direct owner imports are clean.

### RPC, Proto, And QUIC

TODO: Keep runtime-free wire contracts separate from concrete QUIC runtime behavior, and remove `andromeda-quic` facades after callers import protocol owners directly.

Candidate moves:

| Source group | Target crate | Notes |
| --- | --- | --- |
| `andromeda-proto-wire/src/generated_validation.rs` | Internal modules under `andromeda-proto-wire/src/generated_validation/*` | Done; monolith deleted and public reexports preserved. |
| `andromeda-proto/src/structured.rs` | `andromeda-structured-object` | File deleted; `andromeda-proto` reexports the owner type while callers migrate. |
| `andromeda-proto/src/manifest.rs`, `completion.rs`, `errors.rs`, `generated_validation/*` | `andromeda-proto-wire`, `andromeda-rpc-protocol`, `andromeda-procedure-contract` depending on type | Completion/errors/manifest facades deleted and generated validation reduced to Prost adapters; keep `.proto` generation in `andromeda-proto` until a dedicated generation owner exists. |
| `andromeda-quic/src/backpressure.rs`, `protocol_invariants.rs`, `rpc.rs`, `hadr_streams.rs`, frame/stream/typed-envelope facades in `lib.rs` | `andromeda-rpc-protocol`, `andromeda-rpc-codec`, `andromeda-rpc`, `andromeda-hadr` | `hadr_streams`, `backpressure`, `protocol_invariants`, and `rpc` pure facades deleted; continue migrating frame/stream/typed-envelope callers before deleting remaining facades. |
| `andromeda-quic/src/catalog_manifest_resolution/{frame,manifest,validation}.rs` | `andromeda-rpc-codec` and `andromeda-procedure-contract` | QUIC should keep only transport gateway/runtime surface. |
| `andromeda-quic/src/procedure_gateway/validation.rs` | `andromeda-rpc-codec` | Decode/projection of `RpcExecuteRequest` is codec/protocol work. |
| `andromeda-quic/src/procedure_gateway/admission.rs` | `andromeda-iam` or `andromeda-admission` through neutral DTOs | Do not add direct `andromeda-quic -> andromeda-admission`. |
| `andromeda-quic-runtime-quinn/src/*` | Stay in `andromeda-quic-runtime-quinn` | This is the only owner for Quinn/Rustls/Tokio/rcgen runtime. |

SUB TODO:

- Keep the new `andromeda-proto-wire/src/generated_validation/*` domain split stable while migrating protocol callers; the monolith has been deleted.
- Keep `andromeda-proto-wire` from depending on `andromeda-proto`.
- Migrate callers off remaining `andromeda_proto` compatibility reexports for structured-object and protocol-safe types.
- Migrate callers off `andromeda_quic::frame`, `andromeda_quic::stream`, and `andromeda_quic::typed_envelope`; `andromeda_quic::hadr_streams`, `backpressure`, `protocol_invariants`, and `rpc` source facades have been deleted.
- Replace local QUIC `CatalogProcedureManifest` model with a contract-owned or codec-owned projection.
- Remove IAM/admission authorization from `andromeda-quic`; QUIC should produce transport evidence and leave application authorization to IAM/admission layers.
- Decide the fate of `andromeda-protocol`: delete as a redundant facade or define a narrow runtime-free integration purpose.

DEPENDENCIES:

- Owner crates: `andromeda-rpc-protocol`, `andromeda-rpc-codec`, `andromeda-rpc`, `andromeda-proto-wire`, `andromeda-proto`, `andromeda-procedure-contract`, `andromeda-structured-object`, `andromeda-quic-runtime-quinn`, `andromeda-hadr`, `andromeda-security-contract`, `andromeda-iam`, `andromeda-admission`.
- Watch edges: avoid `proto-wire -> proto`; avoid `proto -> protocol` cycles; avoid `hadr -> quic`; avoid `quic -> admission` unless dependency topology is explicitly redesigned.
- Required gates: no-gRPC/no-runtime-JSON scans, malformed frame tests, ResultStream sequence tests, mTLS/admission tests, backpressure tests, surface separation tests, Quinn runtime tests.

Execution order:

1. `andromeda-proto-wire::generated_validation` split completed; preserve public behavior while continuing caller migrations.
2. Proto completion/errors/manifest/structured source facades deleted; continue migrating callers off remaining root compatibility reexports.
3. HADR/backpressure/RPC/protocol-invariant QUIC facades deleted; continue migrating callers off remaining frame/stream/typed-envelope compatibility paths.
4. Move catalog manifest codec/projection from QUIC to RPC/procedure owner crates.
5. Remove procedure gateway admission from QUIC or move it behind neutral DTOs.
6. Delete facades with no remaining callers.
7. Validate runtime Quinn separately.

### Observability, Audit, Benchmarks, And Core

TODO: Prevent observability and benchmark evidence from becoming database truth while extracting identity, audit, trace, and benchmark contracts into owner crates.

Candidate moves:

| Source group | Target crate | Notes |
| --- | --- | --- |
| `andromeda-core/src/principal/{permission,surface_scope,contract}.rs` | `andromeda-security-contract` | Done for runtime-free permission, surface scope, and security contract vocabulary; core reexports remain temporarily. |
| `andromeda-core/src/principal/{certificate*,id,identity,role,session,status,permission_set,registry/**}.rs` | `andromeda-iam` | Only after removing or narrowing `andromeda-iam -> andromeda-core`. |
| `andromeda-observe/src/events/decision.rs` generic `DecisionTrace` pieces | `andromeda-observability` and `andromeda-decision-trace` | Critical decision shape moved to `andromeda-observability` to satisfy observe topology; `andromeda-decision-trace` reexports it for advisory decision-trace callers. |
| `andromeda-observe/src/events/durable_audit/{identity,failure,family,wal_evidence,replay_query,replay_record,sink_report,policy_requirement}.rs` | `andromeda-audit` | Runtime-free DTOs moved to `andromeda-audit`; file sink/journal format remains in observe until `TraceEvent/EventEnvelope` coupling is abstracted. |
| `andromeda-observe/src/principal_binding/*` | `andromeda-iam` and then `andromeda-security` | Avoid `observe -> security` while `security -> observe` still exists. |
| `andromeda-observe/src/events/{protocol,protocol_rejection,transition,sequence}.rs` | `andromeda-execution-trace` or `andromeda-observability` | Defer until `andromeda-execution-trace -> observe` is inverted. |
| `andromeda-bench/src/crud/{data,metrics,scenario}.rs` | `andromeda-bench-workload` | Done; `andromeda-bench` keeps temporary compatibility reexports. |
| `andromeda-bench/src/runner/{counters,latency,synthetic}.rs` | `andromeda-bench-harness` | Done; runner helpers now live in harness owner crate. |
| `andromeda-bench/src/flat_json.rs` | `andromeda-scenario-evidence` | Done; duplicate `andromeda-regression` flat JSON codec deleted and regression now imports scenario evidence. |
| `andromeda-bench/src/*_benchmark.rs` | Stay in `andromeda-bench` initially | Leaf benchmarks depend on WAL/storage/SRPL/observe; do not pollute harness/workload owners. |

SUB TODO:

- Migrate call sites from `andromeda-core` principal facades to `andromeda-security-contract`; runtime-free permission/scope/contract vocabulary is now owner-owned.
- Break `andromeda-iam -> andromeda-core` by moving IAM-owned principal identity/session/registry types into `andromeda-iam` and making `andromeda-core` a facade only during migration.
- Keep the critical decision shape in `andromeda-observability`; evolve `andromeda-decision-trace` versioned contracts without adding an `observe -> decision-trace` edge.
- Durable audit DTOs are now in `andromeda-audit`; next move is file sink/journal abstraction after event envelope coupling is split.
- Keep durable audit file sink and journal mutation local until envelope and trace-event dependencies are abstracted.
- Move `principal_binding` authorization/runtime behavior toward IAM/security after dependency inversion.
- Migrate benchmark callers off remaining `andromeda-bench` compatibility reexports; CRUD result and flat JSON have moved to scenario evidence, bounded runner evidence has moved to bench harness.
- Keep `andromeda-bench`, `andromeda-regression`, `andromeda-scenario-evidence`, and GPU/IO placement traces advisory-only and unable to select database truth alone.

DEPENDENCIES:

- Owner crates: `andromeda-observability`, `andromeda-audit`, `andromeda-decision-trace`, `andromeda-execution-trace`, `andromeda-scenario-evidence`, `andromeda-regression`, `andromeda-bench-harness`, `andromeda-bench-workload`, `andromeda-core`, `andromeda-iam`, `andromeda-security-contract`, `andromeda-security`, `andromeda-hardware`, `andromeda-gpu`.
- Watch edges: avoid `observe -> execution-trace` while `execution-trace -> observe` exists; avoid `observe -> security` while `security -> observe` exists; avoid `core -> iam` while `iam -> core` exists; avoid `audit -> observe` if durable audit keeps `TraceEvent/EventEnvelope` dependencies.
- Required gates: audit durable sink tests, decision trace tests, execution trace tests, IAM/security contract tests, benchmark advisory-only tests, GPU exclusion topology tests.

Execution order:

1. Stabilize topology tests and named temporary exceptions.
2. Core security vocabulary extracted to `andromeda-security-contract`; migrate callers and delete core facades later.
3. Break `iam -> core`, then move principal identity/session/registry types to `andromeda-iam`.
4. Critical decision trace shape moved to `andromeda-observability` with `andromeda-decision-trace` reexports; continue versioned decision-trace extraction there.
5. Durable audit DTOs moved to `andromeda-audit`; defer sink/journal until envelope abstractions are ready.
6. Move `principal_binding` after IAM/security dependency direction is clean.
7. Benchmark workload, harness utilities, CRUD result, and advisory flat JSON moved; migrate remaining callers and delete bench facades later.
8. Delete compatibility facades once downstream imports are direct.

## Global Execution Order

1. Freeze topology evidence: run dependency topology and orphan-source tests before code moves.
2. Migrate tests and imports to owner crates where owner code already exists.
3. Remove or shrink pure compatibility facades that already point to owner crates.
4. Execute storage foundations in order: manifest, disk-page-store, buffer-pool, heap, index, backup, restore, recovery.
5. Execute catalog foundations in order: catalog recovery/WAL DTOs, procedure-store, statistics, publication subscription, catalog recovery replay, catalog diff.
6. Execute transaction foundations in order: commit protocol, allocator, transaction manager, owner tests, downstream import migration, `andromeda-tx` facade deletion.
7. Execute SRPL/exec foundations in order: metadata tests, result-stream tests, SRPL adapters, SRPL facade reduction, exec bridge reduction.
8. Execute RPC/proto/QUIC foundations in order: proto-wire split, proto facade migration, quic facade migration, catalog manifest/procedure gateway projection moves, Quinn runtime validation.
9. Execute observability/core/bench foundations in order: security contract vocabulary, IAM principal ownership, decision trace, durable audit DTOs, benchmark workload/harness.
10. Run full workspace validation, clean stale artifacts only when `E0463` points to stale `target/debug/deps` paths, and then delete facades proven unused by `rg`.

## Validation

Minimum topology and global gates after each major execution wave:

```powershell
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
cargo test -p andromeda-cli --test orphan_source_invariants -- --nocapture
cargo check --workspace --all-targets --all-features
```

Storage-family gates:

```powershell
cargo check -p andromeda-storage -p andromeda-segment -p andromeda-storage-page -p andromeda-manifest -p andromeda-disk-page-store -p andromeda-buffer-pool -p andromeda-backup -p andromeda-restore -p andromeda-wal -p andromeda-recovery -p andromeda-storage-heap -p andromeda-storage-index --all-targets --all-features
cargo test -p andromeda-storage-page --all-targets
cargo test -p andromeda-segment --test segment_index_contract
cargo test -p andromeda-wal --all-targets
cargo test -p andromeda-storage-heap --all-targets
cargo test -p andromeda-storage-index --all-targets
cargo test -p andromeda-backup --all-targets
cargo test -p andromeda-restore --all-targets
cargo test -p andromeda-recovery --all-targets
```

Catalog-family gates:

```powershell
cargo check -p andromeda-catalog -p andromeda-catalog-recovery -p andromeda-catalog-store --all-targets --all-features
cargo check -p andromeda-procedure-store -p andromeda-procedure-contract -p andromeda-plan-cache -p andromeda-scenario-evidence --all-targets --all-features
cargo check -p andromeda-statistics -p andromeda-catalog-diff -p andromeda-definition-batch --all-targets --all-features
cargo test -p andromeda-catalog-recovery --all-targets --all-features
cargo test -p andromeda-procedure-store --all-targets --all-features
cargo test -p andromeda-statistics --all-targets --all-features
```

Transaction-family gates:

```powershell
cargo check -p andromeda-transaction -p andromeda-tx -p andromeda-mvcc -p andromeda-locking -p andromeda-savepoint -p andromeda-transaction-log --all-targets --all-features
cargo check -p andromeda-exec -p andromeda-execution-trace -p andromeda-result-stream --all-targets --all-features
cargo test -p andromeda-transaction --all-targets
cargo test -p andromeda-mvcc --all-targets
cargo test -p andromeda-locking --all-targets
cargo test -p andromeda-savepoint --all-targets
rg "andromeda_tx::|use andromeda_tx|andromeda-tx" crates tests tools -g Cargo.toml -n
```

SRPL/exec/result-stream gates:

```powershell
cargo check -p andromeda-srpl -p andromeda-exec -p andromeda-procedure-runtime -p andromeda-result-stream -p andromeda-srpl-execution-adapter --all-targets --all-features --locked
cargo test -p andromeda-procedure-runtime --lib --locked
cargo test -p andromeda-result-stream --lib --locked
cargo test -p andromeda-exec --test metadata_extraction_contract --locked
cargo test -p andromeda-exec --test result_stream_backpressure --locked
cargo test -p andromeda-exec --test srpl_adapter_contract --locked
cargo test -p andromeda-srpl --test api_compat_reexports --locked
cargo test -p andromeda-srpl --test compiler_pipeline_e2e --locked
cargo test -p andromeda-srpl --test definitionbatch_compat --locked
cargo tree -i andromeda-srpl -e normal
cargo tree -i andromeda-exec -e normal
```

RPC/proto/QUIC gates:

```powershell
cargo check -p andromeda-proto -p andromeda-proto-wire -p andromeda-rpc-protocol -p andromeda-rpc-codec -p andromeda-rpc -p andromeda-quic -p andromeda-quic-runtime-quinn -p andromeda-hadr -p andromeda-security-contract -p andromeda-security -p andromeda-iam -p andromeda-admission --all-targets --all-features
cargo test -p andromeda-rpc-protocol --tests
cargo test -p andromeda-proto --tests
cargo test -p andromeda-quic --test protocol_stability_contract
cargo test -p andromeda-quic --test protobuf_projection_contract
cargo test -p andromeda-quic --test procedure_gateway_route
cargo test -p andromeda-quic --test transport_contract
cargo test -p andromeda-quic-runtime-quinn --test real_quinn_network
cargo test -p andromeda-quic-runtime-quinn --test reconnect_quinn_admission_contract
```

Observability/core/bench gates:

```powershell
cargo check -p andromeda-core -p andromeda-iam -p andromeda-security-contract -p andromeda-security --all-targets --all-features
cargo check -p andromeda-observability -p andromeda-decision-trace -p andromeda-audit -p andromeda-observe --all-targets --all-features
cargo check -p andromeda-bench -p andromeda-bench-harness -p andromeda-bench-workload -p andromeda-scenario-evidence -p andromeda-regression --all-targets --all-features
cargo test -p andromeda-observe --test durable_audit_sink_contract -- --nocapture
cargo test -p andromeda-observe --test durable_audit_retention_contract -- --nocapture
cargo test -p andromeda-observe --test audit_family_contract -- --nocapture
cargo test -p andromeda-bench --test scenario_evidence_boundary -- --nocapture
cargo test -p andromeda-bench --test regression_detection -- --nocapture
cargo test -p andromeda-hardware --test gpu_policy_contract -- --nocapture
```

Rust formatting after code execution waves:

```powershell
rustfmt --edition 2024 <modified-rust-files>
```

Known validation caveats:

- `cargo fmt --all --check` fails on Windows with `os error 206` because the command line is too long for this workspace.
- `cargo fmt -p ... --check` reports pre-existing formatting diffs in packages beyond files modified by extraction slices.
- Some rustfmt settings in `rustfmt.toml` require nightly or are unknown to stable rustfmt; stable rustfmt emits warnings.
- `cargo check` can report false `E0463` crate resolution failures after Cargo/module moves when stale artifacts remain under `target/debug/deps`.

## Troubleshooting

If `cargo check` reports `E0463: can't find crate for andromeda_*` after Cargo or module moves, inspect whether the rustc command already includes the `--extern` path. If it does, the failure is usually a stale `target/debug/deps` artifact. Use targeted artifact removal for the named crate only.

If a proposed move creates a Cargo cycle, do not add a broad integration crate. Instead:

- Move only runtime-free DTOs first.
- Introduce a small trait in the lower owner crate.
- Keep the concrete adapter in the higher integration crate.
- Migrate tests to prove owner behavior without importing through the source large crate.

## Risks

- Moving recovery, manifest, page-store, WAL, transaction, or catalog publication behavior without crash/recovery gates can violate the durable truth model.
- Compatibility facades can hide reverse dependencies and make owner tests accidentally exercise the old crate.
- Removing facades before call-site migration can turn a controlled extraction into broad import churn.
- Documentation drift can cause workers to follow old crate-count or scaffold assumptions.
- `andromeda-execution-trace`, `andromeda-observe`, `andromeda-scenario-evidence`, and `andromeda-regression` sit close to decision evidence. Keep evidence observable and versioned, not authoritative.
- Benchmark, regression, scenario, GPU, and analytics evidence must remain advisory-only and disableable.
- Pulling `andromeda-catalog`, `andromeda-storage`, `andromeda-tx`, or concrete QUIC runtime into low-level owner crates will re-create the large-crate problem under a different name.

## References

- `crates/AGENTS.md`
- `docs/architecture/CARGO_DEPENDENCY_MATRIX.md`
- `docs/MODULES_INVENTORY.md`
- `documentations/architecture/engine-crate-mapping-2026-05-08.md`
- `documentations/architecture/dependency-edge-matrix-2026-05-08.md`
- `documentations/implementation/target-crate-gap-ledger-2026-05-08.md`
- `documentations/implementation/roadmap-execution-plan-2026-05-08.md`
- `documentations/implementation/worker-storage-disk-page-store-2026-05-09.md`
- `documentations/implementation/worker-catalog-definition-batch-2026-05-09.md`
- `documentations/implementation/worker-transaction-downstream-tests-2026-05-09.md`
- `documentations/implementation/worker-srpl-optimizer-2026-05-09.md`
- `documentations/implementation/worker-proto-rpc-quic-2026-05-09.md`
- `documentations/implementation/worker-observe-audit-decision-trace-2026-05-09.md`
- `documentations/implementation/worker-bench-evidence-cleanup-2026-05-09.md`
- `documentations/implementation/worker-topology-gates-map-2026-05-09.md`
- Worker analysis: storage read-only extraction map, 2026-05-09.
- Worker analysis: catalog/procedure read-only extraction map, 2026-05-09.
- Worker analysis: transaction read-only extraction map, 2026-05-09.
- Worker analysis: SRPL/exec read-only extraction map, 2026-05-09.
- Worker analysis: RPC/proto/QUIC read-only extraction map, 2026-05-09.
- Worker analysis: observability/core/bench read-only extraction map, 2026-05-09.
