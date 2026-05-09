# Large Crate Extraction Consolidation - 2026-05-09

## Purpose

Consolidate the read-only analysis of the large Andromeda crates named in the work order and turn it into an execution-ready extraction map.

The goal is not to keep the large crates as permanent facades. The goal is to identify source files and behavior groups that can be moved into the existing owner crates, migrate call sites to those owner crates, and then remove compatibility facades once the workspace proves that no caller still needs them.

## Scope

This plan covers the ten large crates named by the owner: `andromeda-catalog`, `andromeda-core`, `andromeda-storage`, `andromeda-tx`, `andromeda-srpl`, `andromeda-observe`, `andromeda-exec`, `andromeda-bench`, `andromeda-proto`, and `andromeda-quic`.

The read-only analysis also treats `andromeda-proto-wire` as the eleventh practical extraction surface, because the wire validation file is large and sits on the same RPC/proto/QUIC boundary.

The current local workspace has 96 Cargo packages according to `cargo metadata --no-deps --format-version 1`.

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

- The topology gate reports 96 workspace packages after the current extraction wave.
- Rust analyzer is loaded for the workspace and reports one workspace rooted at `C:\Users\Arius\RustroverProjects\Andromeda`.
- The workspace is heavily dirty because multiple write extraction waves moved files, deleted old facades, added owner crates, and updated dependency doctrine. Treat this document as a consolidation for the current branch state, not as an accepted clean baseline.
- The first write wave used seven execution slices covering disk page storage, catalog recovery, transaction downstream/tests, SRPL facade reduction, proto structured/wire contracts, observe/audit/decision trace, and bench cleanup.
- The second write wave used 22 worker execution slices covering storage caller cleanup/recovery, catalog runtime/recovery/proto detachment, QUIC owner migration, exec inventory/SRPL/transaction cleanup, observe audit/security/transition movement, core principal extraction, and transaction owner imports.
- Validation now writes into `target/`; successful gates are recorded in the "Write Wave Validation" and "Executed 22-Worker WRITE Wave" sections.

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

## Read-Only Re-Analysis Wave - 2026-05-09

This section consolidates the dedicated read-only re-analysis wave for the seven current god-crate surfaces: `andromeda-core`, `andromeda-quic`, `andromeda-exec`, `andromeda-observe`, `andromeda-transaction`, `andromeda-storage`, and `andromeda-catalog`.

The phase is intentionally analysis-only. The workers wrote only markdown reports, and this section prepares the next WRITE wave without moving code yet.

### Read-Only Report Inventory

| Report | Slice | Consolidated outcome |
| --- | --- | --- |
| `documentations/implementation/readonly-global-god-crates-map-2026-05-09.md` | Global map | Confirms the seven-crate size baseline, dependency blockers, and an 18-22 worker WRITE wave. |
| `documentations/implementation/readonly-worker-core-iam-foundation-map-2026-05-09.md` | Core/IAM/foundation | Recommends a low-level `andromeda-principal` owner before turning `andromeda-core` into a true facade. |
| `documentations/implementation/readonly-worker-storage-facade-callers-map-2026-05-09.md` | Storage facade callers | Maps external `andromeda_storage::` callers and deletion readiness for WAL/page/segment/disk/buffer/index/backup/restore facades. |
| `documentations/implementation/readonly-worker-storage-recovery-wal-map-2026-05-09.md` | Storage recovery/WAL | Splits neutral recovery DTOs/traits toward `andromeda-recovery`, `andromeda-catalog-recovery`, and `andromeda-wal`. |
| `documentations/implementation/readonly-worker-storage-placement-layout-map-2026-05-09.md` | Storage placement/layout | Keeps placement/profile/cold policy in storage for now; only a later combined placement/profile crate may be justified. |
| `documentations/implementation/readonly-worker-storage-orphan-topology-map-2026-05-09.md` | Storage orphans/topology | Finds 8 real storage WAL orphan files and 15 stale orphan exceptions to clean first. |
| `documentations/implementation/readonly-worker-catalog-facade-callers-map-2026-05-09.md` | Catalog facade callers | Maps `andromeda_catalog::` external callers and facade deletion candidates across exec, SRPL, CLI, fuzz, and tests. |
| `documentations/implementation/readonly-worker-catalog-runtime-snapshot-map-2026-05-09.md` | Catalog runtime/snapshot | Keeps `CatalogSnapshot` as live-state owner; moves generic replay/publication/runtime DTO boundaries outward. |
| `documentations/implementation/readonly-worker-catalog-proto-resolution-map-2026-05-09.md` | Catalog/proto resolution | Removes the remaining catalog runtime status dependency on generated proto mappings by moving mapping tests to protocol/rpc owners. |
| `documentations/implementation/readonly-worker-quic-protocol-gateway-map-2026-05-09.md` | QUIC/protocol/gateway | Migrates frame/typed-envelope/backpressure callers to owner crates before deleting QUIC compatibility reexports. |
| `documentations/implementation/readonly-worker-exec-orchestration-map-2026-05-09.md` | Exec orchestration | Splits exec into orchestration only, product-stock vertical owner, runtime neutralization, CLI migration, and QUIC bridge relocation. |
| `documentations/implementation/readonly-worker-exec-srpl-business-map-2026-05-09.md` | Exec/SRPL/business | Recommends `andromeda-inventory-demo` for V0 inventory/product-stock logic; keeps `andromeda-procedure-runtime` generic. |
| `documentations/implementation/readonly-worker-observe-envelope-audit-map-2026-05-09.md` | Observe envelope/audit | Moves durable audit append/journal runtime only after an event-neutral append input exists in `andromeda-audit`. |
| `documentations/implementation/readonly-worker-observe-principal-trace-map-2026-05-09.md` | Observe principal/trace | Requires security inversion before principal binding moves, and moves transition/protocol DTOs to runtime-free owners. |
| `documentations/implementation/readonly-worker-transaction-core-trace-map-2026-05-09.md` | Transaction/core/trace | Removes transaction-family `andromeda-core` and `andromeda-observe` imports before direct exec-to-owner transaction topology. |

### Current Seven-Crate Size Baseline

Counts were refreshed after the read-only reports completed.

| Crate | Rust files | Rust lines |
| --- | ---: | ---: |
| `andromeda-storage` | 293 | 34694 |
| `andromeda-exec` | 188 | 23862 |
| `andromeda-catalog` | 112 | 14472 |
| `andromeda-observe` | 119 | 14460 |
| `andromeda-quic` | 74 | 10286 |
| `andromeda-transaction` | 75 | 10240 |
| `andromeda-core` | 42 | 4098 |

### Consolidated Findings

- `andromeda-storage` is still the largest extraction target. The next write wave should start with low-risk cleanup: delete the 8 real storage WAL orphan files, remove 15 stale orphan exceptions, and migrate callers off already-owner-backed facades before moving recovery behavior.
- Storage recovery should be split by truth ownership: physical WAL stays in `andromeda-wal`; neutral recovery decisions and replay DTOs move to `andromeda-recovery`; catalog replay contracts move to `andromeda-catalog-recovery`; concrete page/heap/index/catalog adapters stay in `andromeda-storage` until traits make the boundary explicit.
- Storage placement/layout is not a high-value immediate extraction. `layout` and `publication` remain facade surfaces, while placement/profile/cold policy stay storage-local until a dependency audit proves that a single `andromeda-storage-placement` crate is worth the churn.
- `andromeda-catalog` should not move `CatalogSnapshot` yet. The correct next move is a trait boundary: generic mutation replay target, generic publication registry, and store-owned runtime manifest DTOs without importing catalog, storage, exec, proto, or QUIC into lower crates.
- Catalog still has facade debt around contracts, objects, names, procedure-store, statistics, plan-cache/scenario evidence, WAL/recovery, publication, and server DTOs. The next write work should migrate direct callers first, then delete proven unused modules.
- Catalog/proto coupling is now narrow enough for a focused removal: runtime status-to-generated-proto conversion belongs in `andromeda-proto` or `andromeda-rpc-codec`, not in catalog.
- `andromeda-quic` should be transport-local. Frame bytes, typed envelopes, backpressure contracts, procedure gateway projection, and catalog manifest projection should be consumed from `andromeda-rpc-protocol`, `andromeda-rpc-codec`, `andromeda-rpc`, and procedure/catalog contract owners.
- `andromeda-exec` should become orchestration only. V0 inventory/product-stock behavior should move to a vertical owner, concrete SRPL adapter behavior should leave exec/SRPL facade paths, and generic runtime code must not absorb product-specific inventory logic.
- `andromeda-observe` should become an event-envelope/query/export owner over external DTO payloads. Durable audit append/journal belongs to `andromeda-audit`; transition/protocol trace DTOs belong to `andromeda-observability` or another runtime-free owner; principal binding moves only after security dependency inversion.
- `andromeda-core` should not grow a `core -> iam` edge. The recommended path is a low-level `andromeda-principal` owner, then `andromeda-core` becomes a compatibility facade while IAM/admission/security/protocol callers migrate.
- `andromeda-transaction` can shed `andromeda-core` and `andromeda-observe` dependencies before `andromeda-tx` is deleted. `andromeda-tx` must remain until `andromeda-exec` and topology tests allow direct dependencies on transaction owner crates.

### Hard Dependency Blockers

- Do not add `core -> iam` while `iam -> core` exists.
- Do not move principal binding into observe-owned security logic while `security -> observe` exists.
- Do not add `observe -> execution-trace` while `execution-trace -> observe` exists.
- Do not make `andromeda-audit` depend on observe envelopes; add an event-neutral append input first.
- Do not move `CatalogSnapshot` into recovery/store crates. It remains the live-state catalog owner.
- Do not add proto/rpc/quic dependencies to `andromeda-catalog-store` or `andromeda-catalog-recovery`.
- Do not delete `andromeda-tx` until `andromeda-exec` has direct owner dependencies and topology gates approve them.
- Do not move durable recovery/WAL/page/catalog behavior without owner tests and crash/replay gates.
- Do not put V0 inventory/product-stock behavior into `andromeda-procedure-runtime`; keep that crate generic.

### Mega TODO And SUB-TODO

TODO 1: Stabilize cleanup evidence before deep moves.

- SUB-TODO: Delete the 8 real storage WAL orphan files identified by the storage orphan report.
- SUB-TODO: Remove the 15 stale orphan exception entries.
- SUB-TODO: Run orphan/topology gates after cleanup, before behavior movement.
- SUB-TODO: Keep named exceptions only when they match existing source files and include exit criteria.

TODO 2: Migrate callers away from pure facades.

- SUB-TODO: Replace external storage facade imports with direct owner crate imports for WAL, page, segment, disk, buffer, index, backup, restore, HADR, recovery, and manifest types.
- SUB-TODO: Replace catalog facade imports in exec, SRPL, CLI, fuzz, and tests with owner imports where the owner already exists.
- SUB-TODO: Replace QUIC frame/typed-envelope/backpressure imports with RPC protocol/codec/result-stream owners.
- SUB-TODO: Replace observe durable-audit DTO imports with audit owner imports once event-neutral append types exist.
- SUB-TODO: Replace transaction-family core aliases with direct foundation owner crates.

TODO 3: Split topology blockers before moving authority.

- SUB-TODO: Introduce or prepare `andromeda-principal` for identity/session/registry primitives, then convert `andromeda-core` into a facade.
- SUB-TODO: Remove IAM's direct dependency on core before moving principal behavior toward IAM/security layers.
- SUB-TODO: Remove `security -> observe` before moving observe principal-binding authority.
- SUB-TODO: Move transition DTOs to observability and remove transaction/result-stream/execution-trace imports from observe.
- SUB-TODO: Add topology guard tests only after the corresponding migration lands.

TODO 4: Move runtime-free DTOs and trait boundaries.

- SUB-TODO: Add neutral recovery startup, redo plan, replay target, WAL scan, manifest view, and storage format view contracts to `andromeda-recovery`.
- SUB-TODO: Add catalog recovery apply target and generic replay/publication contracts to `andromeda-catalog-recovery`.
- SUB-TODO: Move catalog runtime manifest DTOs to `andromeda-catalog-store` without protobuf coupling.
- SUB-TODO: Move catalog generated-proto conversion tests to proto/rpc owners and remove `andromeda-catalog -> andromeda-proto`.
- SUB-TODO: Move durable audit sink/journal runtime after `andromeda-audit` owns append input/result/failure DTOs.

TODO 5: Extract integration behavior only after lower owners are clean.

- SUB-TODO: Move V0 inventory/product-stock behavior to `andromeda-inventory-demo` or an equivalent vertical owner.
- SUB-TODO: Move concrete SRPL adapter behavior to `andromeda-execution` or a narrower SRPL execution runtime crate.
- SUB-TODO: Move QUIC catalog/procedure gateway projection to RPC/procedure owners while QUIC keeps transport contracts.
- SUB-TODO: Migrate `andromeda-exec` transaction imports directly to transaction owner crates, then remove `andromeda-exec -> andromeda-tx`.
- SUB-TODO: Delete `andromeda-tx` only after direct caller and topology proof is complete.

### Prepared WRITE Worker Set

The next WRITE phase should use 22 workers. Each worker should update its own task report markdown after implementation, then the consolidation file.

| Worker | Future task report | Write scope | Depends on |
| --- | --- | --- | --- |
| W01 | `documentations/implementation/write-worker-storage-orphan-topology-cleanup-2026-05-09.md` | Delete 8 real storage WAL orphan files; remove 15 stale orphan exceptions; keep facade tests intact. | None. First. |
| W02 | `documentations/implementation/write-worker-storage-wal-facade-callers-2026-05-09.md` | Migrate external WAL/LSN imports from `andromeda_storage::` to `andromeda-wal`; update deps/tests. | W01. |
| W03 | `documentations/implementation/write-worker-storage-page-segment-index-callers-2026-05-09.md` | Migrate page, segment, extent, disk, buffer, B-tree/index callers to owner crates. | W01. |
| W04 | `documentations/implementation/write-worker-storage-backup-restore-hadr-cli-2026-05-09.md` | Split CLI/storage boundary imports for backup, restore, HADR, recovery, and vertical commands. | W01. |
| W05 | `documentations/implementation/write-worker-storage-recovery-startup-redo-2026-05-09.md` | Move neutral startup and redo-plan DTOs/tests to `andromeda-recovery`. | W02, W03. |
| W06 | `documentations/implementation/write-worker-storage-replay-filewal-catalog-bridge-2026-05-09.md` | Split replay driver/adapters, file-WAL recovery report, and catalog recovery bridge boundaries. | W05. |
| W07 | `documentations/implementation/write-worker-storage-placement-cold-audit-2026-05-09.md` | Add placement/profile/cold ownership doctrine, consumer audit, and optional future extraction design only if justified. | W01. |
| W08 | `documentations/implementation/write-worker-catalog-facade-import-cleanup-2026-05-09.md` | Migrate exec/SRPL/CLI/fuzz/catalog tests off catalog facades toward owner crates. | W01. |
| W09 | `documentations/implementation/write-worker-catalog-recovery-publication-generic-2026-05-09.md` | Add generic recovery replay target and publication registry boundary in `andromeda-catalog-recovery`. | W08. |
| W10 | `documentations/implementation/write-worker-catalog-runtime-proto-detach-2026-05-09.md` | Move runtime manifest DTOs to store owner and remove catalog generated-proto conversion dependency. | W08, W09. |
| W11 | `documentations/implementation/write-worker-quic-owner-import-migration-2026-05-09.md` | Migrate frame, typed-envelope, and backpressure callers to RPC protocol/codec/result-stream owners. | W01. |
| W12 | `documentations/implementation/write-worker-quic-gateway-projection-runtime-2026-05-09.md` | Move procedure/catalog projection out of QUIC and settle runtime-quinn feature boundary. | W11, W10. |
| W13 | `documentations/implementation/write-worker-exec-facade-ledger-cli-2026-05-09.md` | Produce/update exec public reexport ledger and migrate CLI imports to owner crates. | W08, W11. |
| W14 | `documentations/implementation/write-worker-exec-inventory-demo-extraction-2026-05-09.md` | Add/move V0 inventory/product-stock behavior to `andromeda-inventory-demo` or equivalent vertical owner. | W13. |
| W15 | `documentations/implementation/write-worker-exec-runtime-srpl-adapter-2026-05-09.md` | Neutralize local runtime, move concrete SRPL adapter behavior, keep procedure runtime generic. | W13, W14. |
| W16 | `documentations/implementation/write-worker-observe-audit-journal-move-2026-05-09.md` | Add audit-owned append input/result/failure types, move durable audit sink/journal and tests to `andromeda-audit`. | W01. |
| W17 | `documentations/implementation/write-worker-observe-security-principal-binding-2026-05-09.md` | Invert `security -> observe`, then move principal-binding authority to IAM/security owners with observe compatibility. | W19, W20. |
| W18 | `documentations/implementation/write-worker-observe-transition-protocol-envelope-2026-05-09.md` | Move transition/protocol DTOs to observability/runtime-free owners and shrink observe to envelope/query/export runtime. | W16, W21. |
| W19 | `documentations/implementation/write-worker-core-principal-crate-extraction-2026-05-09.md` | Add `andromeda-principal`, move identity/session/registry primitives and tests, keep core compatibility reexports. | W01. |
| W20 | `documentations/implementation/write-worker-core-iam-admission-security-cleanup-2026-05-09.md` | Remove IAM/admission/security/core facade cycles and foundation aliases; migrate QUIC/RPC/audit/exec tests. | W19. |
| W21 | `documentations/implementation/write-worker-transaction-core-observe-direct-owners-2026-05-09.md` | Remove transaction-family core aliases and transition DTO observe dependency; use direct owner crates. | W18, W20. |
| W22 | `documentations/implementation/write-worker-exec-tx-direct-and-facade-delete-2026-05-09.md` | Allow/migrate exec direct transaction owner deps, remove `exec -> tx`, then delete `andromeda-tx` only after no callers remain. | W21, W13. |

If W22 becomes too broad, split it into W22A for exec direct topology and W22B for final `andromeda-tx` deletion. Do not start W22B until `rg "andromeda_tx::|use andromeda_tx|andromeda-tx"` is clean except intentional docs being updated by the same worker.

### Validation Gates For The Prepared WRITE Wave

Run these after each major worker batch:

```powershell
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
cargo test -p andromeda-cli --test orphan_source_invariants -- --nocapture
cargo check --workspace --all-targets --all-features
```

Storage-focused batches:

```powershell
cargo check -p andromeda-storage -p andromeda-wal -p andromeda-recovery -p andromeda-catalog-recovery -p andromeda-storage-page -p andromeda-segment -p andromeda-disk-page-store -p andromeda-buffer-pool -p andromeda-storage-index -p andromeda-storage-heap -p andromeda-backup -p andromeda-restore -p andromeda-hadr --all-targets --all-features
cargo test -p andromeda-storage --test wal_ownership_invariants -- --nocapture
cargo test -p andromeda-wal -p andromeda-recovery -p andromeda-catalog-recovery --all-targets --all-features
```

Catalog/proto/QUIC batches:

```powershell
cargo check -p andromeda-catalog -p andromeda-catalog-recovery -p andromeda-catalog-store -p andromeda-procedure-store -p andromeda-proto -p andromeda-rpc-codec -p andromeda-rpc-protocol -p andromeda-quic -p andromeda-quic-runtime-quinn --all-targets --all-features
cargo test -p andromeda-catalog --test catalog_store_contract -- --nocapture
cargo test -p andromeda-catalog --test wal_record_design -- --nocapture
cargo test -p andromeda-rpc-codec -p andromeda-rpc-protocol -p andromeda-quic --all-targets
```

Exec/observe/core/transaction batches:

```powershell
cargo check -p andromeda-exec -p andromeda-execution -p andromeda-procedure-runtime -p andromeda-srpl-execution-adapter -p andromeda-observe -p andromeda-audit -p andromeda-observability -p andromeda-core -p andromeda-iam -p andromeda-security -p andromeda-security-contract -p andromeda-transaction -p andromeda-transaction-log -p andromeda-mvcc -p andromeda-locking -p andromeda-savepoint --all-targets --all-features
cargo test -p andromeda-exec --all-targets
cargo test -p andromeda-observe --all-targets
cargo test -p andromeda-transaction -p andromeda-transaction-log -p andromeda-mvcc -p andromeda-locking -p andromeda-savepoint --all-targets
```

### Stop Condition

This re-analysis wave stops here. Do not deploy WRITE workers from this plan until the owner explicitly orders the write phase.

## Executed 22-Worker WRITE Wave - 2026-05-09

The owner explicitly started the WRITE phase after the read-only stop condition. This section records the completed W01-W22 execution wave and replaces the prepared-worker table above as the current branch state.

### Write Report Inventory

| Worker | Report | Status |
| --- | --- | --- |
| W01 | `documentations/implementation/write-worker-storage-orphan-topology-cleanup-2026-05-09.md` | Completed. Deleted 8 storage WAL orphan files and emptied the stale orphan allowlist. |
| W02 | `documentations/implementation/write-worker-storage-wal-facade-callers-2026-05-09.md` | Completed. Migrated WAL/LSN callers toward `andromeda-wal`. |
| W03 | `documentations/implementation/write-worker-storage-page-segment-index-callers-2026-05-09.md` | Completed. Migrated page, segment, disk, buffer, and index callers to owner crates. |
| W04 | `documentations/implementation/write-worker-storage-backup-restore-hadr-cli-2026-05-09.md` | Completed. Moved CLI backup/restore/HADR imports toward owner crates. |
| W05 | `documentations/implementation/write-worker-storage-recovery-startup-redo-2026-05-09.md` | Completed. Moved neutral recovery startup, coverage, and redo-plan contracts into `andromeda-recovery`. |
| W06 | `documentations/implementation/write-worker-storage-replay-filewal-catalog-bridge-2026-05-09.md` | Completed. Moved generic replay/file-WAL report to `andromeda-recovery` and catalog LSN replay selection to `andromeda-catalog-recovery`. |
| W07 | `documentations/implementation/write-worker-storage-placement-cold-audit-2026-05-09.md` | Completed. Added placement/cold-storage doctrine and kept storage-local behavior where extraction was not justified yet. |
| W08 | `documentations/implementation/write-worker-catalog-facade-import-cleanup-2026-05-09.md` | Completed. Reduced catalog facade imports in callers and tests. |
| W09 | `documentations/implementation/write-worker-catalog-recovery-publication-generic-2026-05-09.md` | Completed. Added generic catalog replay/publication recovery boundaries. |
| W10 | `documentations/implementation/write-worker-catalog-runtime-proto-detach-2026-05-09.md` | Completed. Moved runtime manifest DTOs to `andromeda-catalog-store` and removed catalog/proto status coupling. |
| W11 | `documentations/implementation/write-worker-quic-owner-import-migration-2026-05-09.md` | Completed. Migrated QUIC frame/stream/typed-envelope style imports toward RPC owner crates. |
| W12 | `documentations/implementation/write-worker-quic-gateway-projection-runtime-2026-05-09.md` | Completed. Moved procedure/catalog projection toward `andromeda-procedure-contract` and `andromeda-rpc-codec`. |
| W13 | `documentations/implementation/write-worker-exec-facade-ledger-cli-2026-05-09.md` | Completed. Migrated CLI imports off exec facades for admission/result-stream surfaces and updated the exec facade ledger. |
| W14 | `documentations/implementation/write-worker-exec-inventory-demo-extraction-2026-05-09.md` | Completed. Added `andromeda-inventory-demo` and moved V0 inventory/ProductStock behavior out of `andromeda-exec`. |
| W15 | `documentations/implementation/write-worker-exec-runtime-srpl-adapter-2026-05-09.md` | Completed. Moved the concrete SRPL runtime adapter from exec to `andromeda-execution`. |
| W16 | `documentations/implementation/write-worker-observe-audit-journal-move-2026-05-09.md` | Completed. Moved durable audit sink/journal/runtime DTOs into `andromeda-audit`. |
| W17 | `documentations/implementation/write-worker-observe-security-principal-binding-2026-05-09.md` | Completed. Moved principal-binding authority to `andromeda-security`, leaving observe compatibility where required. |
| W18 | `documentations/implementation/write-worker-observe-transition-protocol-envelope-2026-05-09.md` | Completed. Moved transition/protocol DTOs to `andromeda-observability`. |
| W19 | `documentations/implementation/write-worker-core-principal-crate-extraction-2026-05-09.md` | Completed. Added `andromeda-principal` and moved core principal primitives/tests into it. |
| W20 | `documentations/implementation/write-worker-core-iam-admission-security-cleanup-2026-05-09.md` | Completed. Migrated IAM/admission/security/core consumers toward principal/foundation owners. |
| W21 | `documentations/implementation/write-worker-transaction-core-observe-direct-owners-2026-05-09.md` | Completed. Moved transaction-family imports off core/observe aliases to direct owner crates. |
| W22 | `documentations/implementation/write-worker-exec-tx-direct-and-facade-delete-2026-05-09.md` | Partially completed. W22A removed `andromeda-exec -> andromeda-tx`; W22B final `andromeda-tx` deletion remains blocked. |

### Consolidated Implementation Result

- Added owner crates `andromeda-principal` and `andromeda-inventory-demo`.
- Reduced `andromeda-storage` by removing orphan WAL files and pushing WAL/page/segment/disk/buffer/index/backup/restore/HADR callers toward owner crates.
- Moved recovery-neutral startup, replay, file-WAL report, and redo-plan surfaces into `andromeda-recovery`; moved catalog-specific replay selection and generic replay/publication boundaries into `andromeda-catalog-recovery`.
- Detached catalog runtime manifest/status DTOs from proto by placing owner DTOs in `andromeda-catalog-store` and RPC/proto mappings in protocol/codec owners.
- Moved QUIC catalog/procedure gateway projection out toward `andromeda-rpc-codec` and `andromeda-procedure-contract`.
- Moved durable audit sink/journal behavior into `andromeda-audit`; moved observe transition/protocol DTOs into `andromeda-observability`; moved principal-binding authority into `andromeda-security`.
- Moved core principal primitives into `andromeda-principal` and adjusted IAM/admission/security/QUIC/RPC/audit/exec callers to use the narrower owner.
- Moved inventory/product-stock demo behavior into `andromeda-inventory-demo` and the concrete SRPL adapter into `andromeda-execution`.
- Removed `andromeda-exec`'s runtime dependency on `andromeda-tx`; exec now uses direct transaction owner crates where the topology allows it.

### Integration Fixes Applied After Worker Merge

- Fixed `andromeda-catalog-recovery` durable payload/mutation borrowing after DTO movement.
- Mapped backup execution-plan validation errors to CLI errors after the backup owner split.
- Migrated exec tests off SRPL facade types where direct owner crates were already available.
- Updated topology/doctrine gates for the 96-crate workspace and for temporary, named extraction exceptions.
- Aligned exec durable-audit validation tests with the extracted `andromeda-audit` fail-closed `Security` error kind.

### Remaining TODO, SUB-TODO, And Dependencies

TODO: Finish the `andromeda-tx` removal only after the last intentional callers are gone.

- SUB-TODO: Migrate `andromeda-inventory-demo` off `andromeda-tx` if it can use direct `andromeda-transaction`, `andromeda-transaction-log`, and `andromeda-mvcc` owners.
- SUB-TODO: Move or delete `andromeda-tx` compatibility tests once equivalent owner tests exist.
- SUB-TODO: Update topology and documentation references that still intentionally mention `andromeda-tx`.
- DEPENDENCY: Do not delete `andromeda-tx` until `rg "andromeda_tx::|use andromeda_tx|andromeda-tx"` is clean except the deletion patch itself.

TODO: Remove temporary topology exceptions introduced to keep the massive extraction compiling.

- SUB-TODO: Remove the temporary `andromeda-exec -> andromeda-inventory-demo` backedge by deciding whether inventory-demo remains a dev-only vertical fixture or gets inverted behind a trait.
- SUB-TODO: Remove temporary C5 core facade exceptions for `andromeda-buffer-pool -> andromeda-core` and `andromeda-recovery -> andromeda-core`.
- SUB-TODO: Remove temporary transitive storage exceptions through `andromeda-catalog-recovery` once catalog recovery no longer pulls catalog-store/SRPL diagnostics on storage paths.

TODO: Continue facade deletion from owner-call proof, not from naming alone.

- SUB-TODO: Run targeted `rg "andromeda_storage::|andromeda_catalog::|andromeda_quic::|andromeda_observe::|andromeda_core::|andromeda_tx::"` scans before each deletion.
- SUB-TODO: Prefer direct owner imports in tests first; then delete facade files when no caller remains.
- SUB-TODO: Keep compatibility reexports only when the owner crate tests already prove the moved behavior.

TODO: Clean remaining warning-only dead code after extraction stabilizes.

- SUB-TODO: Storage backup helper and catalog-WAL bridge helpers now report unused warnings.
- SUB-TODO: Catalog publication-subscription helper imports now report unused warnings.
- SUB-TODO: Exec registry/inventory test support still contains unused fixture helpers after inventory extraction.
- SUB-TODO: Recovery tests now report unused fixtures/imports/variables after the recovery DTO split.

### Validation Passed For This Wave

The following gates passed after all worker outputs were integrated and the final exec audit-contract assertion was aligned:

```powershell
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
cargo test -p andromeda-cli --test orphan_source_invariants -- --nocapture
cargo check --workspace --all-targets --all-features
cargo test -p andromeda-storage --test wal_ownership_invariants -- --nocapture
cargo test -p andromeda-backup -p andromeda-restore -p andromeda-buffer-pool -p andromeda-storage-index -p andromeda-storage-heap --all-targets
cargo test -p andromeda-catalog --test catalog_store_contract -- --nocapture
cargo test -p andromeda-catalog --test wal_record_design -- --nocapture
cargo test -p andromeda-rpc-codec -p andromeda-rpc-protocol -p andromeda-quic --all-targets
cargo test -p andromeda-observe --all-targets
cargo test -p andromeda-exec --all-targets
```

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
- `documentations/implementation/readonly-global-god-crates-map-2026-05-09.md`
- `documentations/implementation/readonly-worker-core-iam-foundation-map-2026-05-09.md`
- `documentations/implementation/readonly-worker-storage-facade-callers-map-2026-05-09.md`
- `documentations/implementation/readonly-worker-storage-recovery-wal-map-2026-05-09.md`
- `documentations/implementation/readonly-worker-storage-placement-layout-map-2026-05-09.md`
- `documentations/implementation/readonly-worker-storage-orphan-topology-map-2026-05-09.md`
- `documentations/implementation/readonly-worker-catalog-facade-callers-map-2026-05-09.md`
- `documentations/implementation/readonly-worker-catalog-runtime-snapshot-map-2026-05-09.md`
- `documentations/implementation/readonly-worker-catalog-proto-resolution-map-2026-05-09.md`
- `documentations/implementation/readonly-worker-quic-protocol-gateway-map-2026-05-09.md`
- `documentations/implementation/readonly-worker-exec-orchestration-map-2026-05-09.md`
- `documentations/implementation/readonly-worker-exec-srpl-business-map-2026-05-09.md`
- `documentations/implementation/readonly-worker-observe-envelope-audit-map-2026-05-09.md`
- `documentations/implementation/readonly-worker-observe-principal-trace-map-2026-05-09.md`
- `documentations/implementation/readonly-worker-transaction-core-trace-map-2026-05-09.md`
- Worker analysis: storage read-only extraction map, 2026-05-09.
- Worker analysis: catalog/procedure read-only extraction map, 2026-05-09.
- Worker analysis: transaction read-only extraction map, 2026-05-09.
- Worker analysis: SRPL/exec read-only extraction map, 2026-05-09.
- Worker analysis: RPC/proto/QUIC read-only extraction map, 2026-05-09.
- Worker analysis: observability/core/bench read-only extraction map, 2026-05-09.
