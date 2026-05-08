# Large Crate Extraction Consolidation - 2026-05-09

## Purpose

Consolidate the analysis of the large Andromeda crates named in the work order and define the next safe extraction sequence for the current `codex/workspace-crate-restructure` branch.

The current local workspace has 94 Cargo packages according to `cargo metadata --no-deps --format-version 1`. Several documents still mention 96 crates and should be reconciled after the code boundary work stabilizes.

## Scope

This plan covers the ten large crates named by the owner: `andromeda-catalog`, `andromeda-core`, `andromeda-storage`, `andromeda-tx`, `andromeda-srpl`, `andromeda-observe`, `andromeda-exec`, `andromeda-bench`, `andromeda-proto`, and `andromeda-quic`.

It also names target crates that already exist in the workspace and are intended to receive extracted behavior.

## Non-goals

- Do not introduce an application-facing SQL surface.
- Do not bypass typed Procedure contracts.
- Do not move commit, WAL, rollback, MVCC visibility, catalog publication, or recovery behavior without owner tests and crash/recovery evidence.
- Do not treat benchmark, analytics, GPU, RAM, or temporary state as database truth.
- Do not remove compatibility reexports until call sites and owner tests have migrated.

## Current Evidence

- `cargo metadata --no-deps --format-version 1` reports 94 workspace packages.
- `cargo check --workspace --all-targets --all-features` completes after the first stabilization slice.
- The workspace is heavily dirty. Treat this document as a consolidation for the current branch state, not as an accepted clean baseline.
- Workers analyzed storage, catalog, transaction, SRPL, RPC/QUIC/proto/security, and observability/benchmark domains in read-only mode.

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
- Converted `andromeda-quic::hadr_streams` into a compatibility facade over `andromeda-hadr`.

## TODO / SUB TODO / DEPENDENCIES

### Storage Kernel

TODO: Reduce `andromeda-storage` from broad C5 owner to storage integration facade.

SUB TODO:

- Keep extents and segment descriptors owned by `andromeda-segment`; remove storage compatibility facades after downstream call sites migrate.
- Move remaining pure manifest domain, codec, root-switch, and publication boundary code into `andromeda-manifest`.
- Move segment-index owner tests fully to `andromeda-segment`; leave storage tests as compatibility and recovery integration tests.
- Move pure page I/O contracts into `andromeda-disk-page-store` only after WAL-before-page-flush tests are owner-level.
- Move buffer pool manager and guards into `andromeda-buffer-pool` after page-store and WAL observer traits are stable.
- Move pure backup DTOs, preflight, retention, and immutable artifact contracts into `andromeda-backup`; keep WAL/manifest copy orchestration under integration tests until PITR evidence is complete.
- Move restore planning and PITR preflight contracts into `andromeda-restore`; keep replay effects behind recovery gates.
- Keep heap and BTree durable mutation code in storage until DEC-032 storage format gates and replay handlers are complete.

DEPENDENCIES:

- `andromeda-storage-page`, `andromeda-wal`, `andromeda-segment`, `andromeda-manifest`, `andromeda-recovery`.
- Required gates: WAL owner tests, page codec tests, segment-index contract tests, manifest recovery floor tests, crash/recovery matrix.

### Catalog And Procedure Contracts

TODO: Make `andromeda-catalog` a catalog runtime owner, not a holder for every contract, plan, stats, and Procedure Store concern.

SUB TODO:

- Move Procedure contract model and compatibility-only imports to `andromeda-procedure-contract` and `andromeda-contract` facade paths.
- Move Procedure Store runtime records, feedback, and regression evidence into `andromeda-procedure-store` after catalog publication integration remains tested.
- Keep the ScenarioEvidence plan-cache bridge owned by `andromeda-scenario-evidence`; move any remaining plan-cache identity, selection, and bounded evidence to `andromeda-plan-cache`; leave catalog only with published plan invalidation integration.
- Move statistics object contracts and publication-switch primitives to `andromeda-statistics`; keep catalog activation and version publication in catalog until owner gates pass.
- Move catalog recovery replay orchestration into `andromeda-catalog-recovery` behind an application trait that avoids cycles.
- Move durable catalog diff behavior into `andromeda-catalog-diff` with Procedure compatibility and ContractHash impact evidence.

DEPENDENCIES:

- `andromeda-contract`, `andromeda-procedure-contract`, `andromeda-definition-batch`, `andromeda-procedure-store`, `andromeda-plan-cache`, `andromeda-statistics`, `andromeda-catalog-recovery`.
- Required gates: ContractHash golden vectors, DefinitionBatch dry-run/apply tests, catalog WAL replay tests, plan invalidation tests, statistics publication tests.

### Transaction Kernel

TODO: Convert `andromeda-tx` into a transaction integration facade over owner crates.

SUB TODO:

- Move local MVCC modules from `andromeda-tx` into `andromeda-mvcc` or delete duplicates after call sites use the owner crate.
- Keep commit-log manager and tests owned by `andromeda-transaction`; remove the `andromeda-tx` compatibility facade after downstream call sites migrate.
- Keep transaction record shapes in `andromeda-transaction-log`.
- Keep lock manager and deadlock residuals owned by `andromeda-locking`; remove the `andromeda-tx` compatibility facade after downstream call sites migrate.
- Keep savepoint partial rollback local to `andromeda-savepoint`; do not confuse savepoint rollback with terminal durable rollback.
- Implement deferred MVCC and index replay handlers before claiming complete recovery semantics.

DEPENDENCIES:

- `andromeda-transaction`, `andromeda-transaction-log`, `andromeda-mvcc`, `andromeda-locking`, `andromeda-savepoint`, `andromeda-wal`, `andromeda-storage` recovery integration.
- Required gates: durable commit evidence tests, rollback evidence tests, MVCC isolation/anomaly tests, lock manager tests, recovery replay tests.

### SRPL And Procedure Runtime

TODO: Keep `andromeda-srpl` as a temporary compatibility facade while moving compiler and bridge behavior into owner crates.

SUB TODO:

- Move `definition_batch_bridge/*` out of `andromeda-srpl` into `andromeda-definition-batch`, `andromeda-catalog`, or a narrow integration crate if dependency cycles require it.
- Split `andromeda-srpl-binder/src/catalog_plan.rs` into catalog view, operation binding, validation, and fixtures.
- Move reusable fixture bodies into `andromeda-srpl-test-fixtures`.
- Split `andromeda-srpl-execution-adapter/src/contracts.rs` into row bounds, operation context, request types, and validation.
- Split parser statement logic after the grammar expands; keep current strict no-dynamic-SQL semantics.
- Treat placeholder lowering for assert/update/emit as V0 only until typed expression IR is complete.

DEPENDENCIES:

- `andromeda-srpl-lexer`, `andromeda-srpl-parser`, `andromeda-srpl-ast`, `andromeda-srpl-binder`, `andromeda-srpl-ir`, `andromeda-srpl-lowering`, `andromeda-srpl-interpreter`, `andromeda-srpl-execution-adapter`, `andromeda-procedure-runtime`.
- Required gates: parser diagnostics, lowering tests, binder contract tests, SRPL facade compatibility tests, exec SRPL integration tests, fuzz/property tests for grammar changes.

### Execution Engine

TODO: Reduce `andromeda-exec` to Procedure orchestration and integration, not ownership of protocol, SRPL language model, security vocabulary, or transaction semantics.

SUB TODO:

- Move generic dispatch request validation and remote-unavailable contracts into `andromeda-procedure-runtime`.
- Keep concrete SRPL adapters in `andromeda-exec` until a runtime integration crate is accepted.
- Remove temporary direct `andromeda-exec` to `andromeda-quic` bridge after transport callers use `andromeda-rpc`/`andromeda-rpc-protocol` paths.
- Keep WAL evidence handling aligned with transaction and storage owner crates.
- Verify ResultStream metadata-before-payload ordering at the procedure runtime boundary.

DEPENDENCIES:

- `andromeda-procedure-runtime`, `andromeda-result-stream`, `andromeda-admission`, `andromeda-iam`, `andromeda-security`, `andromeda-tx`, `andromeda-storage`, `andromeda-srpl-execution-adapter`.
- Required gates: admission-before-transaction tests, runtime contract tests, completion audit tests, retry semantics tests, recovery visibility gates.

### RPC, Proto, And QUIC

TODO: Keep runtime-free wire contracts separate from concrete QUIC runtime behavior.

SUB TODO:

- Split `andromeda-proto-wire/src/generated_validation.rs` into frame envelope, result stream, invocation, manifest, structured object, errors, and view modules.
- Keep `andromeda-rpc-protocol` as owner of frame and stream contracts; keep `andromeda-rpc-codec` as typed envelope/Protobuf glue.
- Keep HA/DR stream ranges and multiplexing owned by `andromeda-hadr`; remove the `andromeda-quic` compatibility facade after downstream call sites migrate.
- Keep `andromeda-quic` as transport abstraction and compatibility facade; concrete Quinn/Rustls/Tokio behavior remains in `andromeda-quic-runtime-quinn`.
- Clarify `andromeda-protocol`; it appears to be a runtime-free facade and should not duplicate protocol ownership.
- Add admission and mTLS certificate status evidence to the concrete Quinn accept/connect path.
- Add runtime mapping tests from Quinn flow control/backpressure to typed `BackpressureSignal`.

DEPENDENCIES:

- `andromeda-rpc-protocol`, `andromeda-rpc-codec`, `andromeda-rpc`, `andromeda-proto-wire`, `andromeda-proto`, `andromeda-quic-runtime-quinn`, `andromeda-security-contract`, `andromeda-admission`, `andromeda-iam`.
- Required gates: no-gRPC/no-runtime-JSON scans, malformed frame tests, ResultStream sequence tests, mTLS/admission tests, backpressure tests, surface separation tests.

### Observability, Audit, Benchmarks, And Core

TODO: Prevent observability and benchmark evidence from becoming database truth.

SUB TODO:

- Consolidate `andromeda_observe::DecisionTrace` projections with `andromeda-decision-trace` versioned contracts.
- Move or strictly namespace durable audit journal internals in `andromeda-observe`; consider a future `andromeda-durable-audit` only if it has a single primary responsibility.
- Rename or move audit tests that claim fsync/crash/replay without real durable I/O evidence.
- Move duplicate flat JSON artifact parsing from `andromeda-scenario-evidence` and `andromeda-regression` into a bounded advisory artifact codec if the format becomes stable.
- Move `andromeda-core/src/principal/*` toward `andromeda-iam` and `andromeda-security-contract`; keep `andromeda-core` a thin compatibility facade.
- Keep `andromeda-bench` and benchmark evidence advisory-only and outside production dependencies.

DEPENDENCIES:

- `andromeda-observe`, `andromeda-observability`, `andromeda-audit`, `andromeda-decision-trace`, `andromeda-execution-trace`, `andromeda-scenario-evidence`, `andromeda-regression`, `andromeda-bench-harness`, `andromeda-bench-workload`, `andromeda-core`, `andromeda-iam`.
- Required gates: audit durable sink tests, decision trace tests, execution trace tests, benchmark advisory-only tests, GPU exclusion topology tests.

## Execution Order

1. Stabilize compile and owner tests for already-started extractions. This slice is complete for `segment_index`, `ExtentId`, and protocol completion version.
2. Run topology and orphan tests to identify duplicate modules hidden by compatibility facades.
3. Complete storage-family owner crates in this order: `segment`, `storage-page`, `manifest`, `disk-page-store`, `buffer-pool`, then recovery integration.
4. Complete catalog-family owner crates: `procedure-contract`, `definition-batch`, `procedure-store`, `plan-cache`, `statistics`, then catalog recovery.
5. Complete transaction-family owner crates: MVCC, locking, savepoint, transaction-log, commit manager.
6. Complete SRPL facade reduction after catalog/Procedure owner crates are stable.
7. Complete RPC/QUIC/security facade reduction after protocol and admission gates are stable.
8. Reduce observability/core/bench duplicate ownership after durable and contract owners stop depending on compatibility paths.

## Validation

Minimum gates for this stabilization slice:

```powershell
cargo check --workspace --all-targets --all-features
cargo check -p andromeda-storage -p andromeda-segment -p andromeda-storage-page -p andromeda-srpl-interpreter -p andromeda-proto -p andromeda-proto-wire -p andromeda-rpc-protocol --all-targets --all-features
cargo check -p andromeda-transaction -p andromeda-tx -p andromeda-locking -p andromeda-mvcc --all-targets --all-features
cargo check -p andromeda-scenario-evidence -p andromeda-plan-cache -p andromeda-catalog --all-targets --all-features
cargo check -p andromeda-hadr -p andromeda-quic --all-targets --all-features
rustfmt --edition 2024 <modified-rust-files>
```

Known validation caveats:

- `cargo fmt --all --check` fails on Windows with `os error 206` because the command line is too long for this workspace.
- `cargo fmt -p ... --check` reports pre-existing formatting diffs in packages beyond the files modified by this slice.
- Some rustfmt settings in `rustfmt.toml` require nightly or are unknown to stable rustfmt; stable rustfmt emits warnings.
- `cargo check` completes with warnings in `andromeda-storage` and `andromeda-recovery` tests that predate this slice.

## Troubleshooting

If `cargo check` reports `E0463: can't find crate for andromeda_*` after Cargo or module moves, inspect whether the rustc command already includes the `--extern` path. If it does, the failure is usually a stale `target/debug/deps` artifact. Use targeted artifact removal for the named crate only.

## Risks

- Moving recovery, manifest, page-store, WAL, or transaction behavior without crash/recovery gates can violate the durable truth model.
- Compatibility facades can hide reverse dependencies and make owner tests accidentally exercise the old crate.
- Documentation drift can cause workers to follow old crate-count or scaffold assumptions.
- `andromeda-execution-trace` and `andromeda-observe` currently sit close to storage/tx evidence. Keep evidence observable and versioned, not authoritative.
- Benchmark, regression, and scenario evidence must remain advisory-only and disableable.

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `docs/architecture/CARGO_DEPENDENCY_MATRIX.md`
- `docs/MODULES_INVENTORY.md`
- `documentations/architecture/engine-crate-mapping-2026-05-08.md`
- `documentations/architecture/dependency-edge-matrix-2026-05-08.md`
- `documentations/implementation/target-crate-gap-ledger-2026-05-08.md`
- `documentations/implementation/roadmap-execution-plan-2026-05-08.md`
- `manifest_extraction_plan.md`
- `segment_extraction_plan.md`
- Worker analyses for storage, catalog, transaction, SRPL, RPC/QUIC/proto, and observability/benchmark domains.
