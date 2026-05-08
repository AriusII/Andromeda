# Reexport Migration Ledger - 2026-05-08

## Purpose

This ledger records intentional compatibility reexports and documented facades that exist during the workspace restructure. It prevents later packets from mistaking facade compatibility for canonical ownership or removing historical imports before caller migration is proven.

## Scope

The ledger covers public reexports and facade modules observed in the current workspace on 2026-05-08. It focuses on migration-relevant surfaces documented by source comments, ADR-0011, `crates/README.md`, and compatibility tests.

The ledger does not list every root-level `pub use` that simply exposes an owning crate's normal public API. It also does not remove, rename, or change any symbol.

## Non-goals

- Do not treat a reexport as proof that the facade crate owns the behavior.
- Do not remove compatibility paths without direct caller migration and compatibility tests.
- Do not use facade tests as a substitute for owner tests, crash/recovery tests, or protocol/security tests.
- Do not use this document to claim release completeness.

## Prerequisites

Before changing a reexported surface, read:

- `AGENTS.md`
- `crates/README.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `documentations/architecture/dependency-edge-matrix-2026-05-08.md`
- `documentations/architecture/module-inventory-2026-05-08.md`

For C5 storage, WAL, recovery, transaction, catalog, or security surfaces, freeze public imports first and prove compatibility before moving behavior.

## Procedure

1. Identify the facade path and the canonical owner path.
2. Confirm whether the facade is a temporary migration path or a stable domain grouping inside the owner crate.
3. Keep owner evidence separate from facade compatibility evidence.
4. Do not delete the facade until downstream imports are migrated and API compatibility tests prove no public import regression.
5. If a facade begins defining new types or behavior, stop and move the behavior to the canonical owner or record a governance decision.

## Migration Ledger

| ID | Facade path | Canonical owner | Current exported surface | Status | Guard evidence | Exit criteria |
| --- | --- | --- | --- | --- | --- | --- |
| RML-001 | `andromeda-core` root, `andromeda_core::digest`, `andromeda_core::policy` | `andromeda-error`, `andromeda-digest`, `andromeda-types`, `andromeda-time`, `andromeda-hardware`; principal identity remains local to `andromeda-core` for now | Error types, digest helpers, identifiers, scalar descriptors, clocks, hardware policy descriptors, principal identity and policy evidence types. | Active temporary R0 facade. | `andromeda-core` facade tests in `src/lib.rs`; topology guard for temporary core facade edges. | Downstream crates import foundation crates directly; principal identity and policy contracts move to a dedicated owner; `andromeda-core` becomes pure reexport or is retired. |
| RML-002 | `andromeda-catalog::contracts`, `andromeda-catalog::names`, `andromeda-catalog::objects` | `andromeda-contract` | Procedure contracts, compatibility diagnostics, qualified names, catalog object descriptors, object binding types, structural dependency vocabulary. | Active temporary contract facade. | `crates/andromeda-catalog/tests/contract_facade_compatibility.rs`; topology tests. | Callers migrate to `andromeda-contract`; catalog keeps only catalog storage, DefinitionBatch, snapshot, publication, WAL, statistics, and Procedure Store ownership. |
| RML-003 | `andromeda-proto::structured` | `andromeda-structured-object` | `StructuredObjectHeader`, `StructuredObjectLayout`, `RowCountPolicy`. | Active temporary StructuredObject facade. | `crates/andromeda-proto/tests/structured_object_facade_compatibility.rs`; protocol projection tests. | Callers migrate to `andromeda-structured-object`; `andromeda-proto` retains schema generation, payload projection, and protocol validation only. |
| RML-004 | `andromeda-quic::frame`, `andromeda-quic::stream`, selected root frame and stream reexports | `andromeda-rpc-protocol` | Frame codes, frame header, frame codec, frame family, frame type, stream role, ResultStream sequence validation, stream-family contracts. | Active temporary RPC protocol facade through transport crate. | QUIC protocol tests; `andromeda-rpc-protocol` owner tests; topology guard that keeps protocol contracts runtime-free. | Callers import runtime-free frame and stream contracts directly from `andromeda-rpc-protocol`; `andromeda-quic` keeps concrete transport adapters and no Procedure, storage truth, WAL, or authorization ownership. |
| RML-005 | `andromeda-quic::backpressure` and root backpressure reexports | `andromeda-rpc-protocol` for runtime-free backpressure contracts; `andromeda-quic` for transport behavior | `BackpressureReason`, `BackpressureSignal`, `BackpressureTransport` exposed through transport compatibility paths. | Active mixed facade. | Backpressure and protocol contract tests. | Runtime-free backpressure contracts are imported from `andromeda-rpc-protocol`; transport-only behavior remains in `andromeda-quic`. |
| RML-006 | `andromeda-srpl` root reexports and `andromeda-srpl::procedure_compiler` | Extracted SRPL model crates where present; `andromeda-srpl` still owns binder, lowering, interpreter, optimizer, DefinitionBatch bridge, and resolver code in this snapshot | Historical compiler, diagnostics, procedure model, DefinitionBatch bridge, lexer/parser/binder/lowering entry points. | Active SRPL compatibility facade. | SRPL compiler tests, DefinitionBatch compatibility tests, parser tests, topology guard for extracted language-model crates. | Parser/model callers migrate to extracted crates; catalog-facing bridge logic moves behind a dedicated bridge crate; `andromeda-srpl` no longer hides catalog-store coupling. |
| RML-007 | `andromeda-storage::wal`, `andromeda-storage::wal_codec`, `andromeda-storage::wal_segment` | `andromeda-wal` | Pure WAL records, transaction summaries, in-memory WAL management, WAL codec constants and scanner, WAL segment descriptors. | Active C5 storage compatibility facade. | `crates/andromeda-storage/tests/api_compat_reexports.rs`; `crates/andromeda-storage/tests/wal_ownership_invariants.rs`; `andromeda-wal` owner tests. | Callers migrate to `andromeda-wal`; storage keeps only recovery reports, replay planning, manifest/page integration, and durable visibility integration. |
| RML-008 | `andromeda-storage::write_ahead_log::{codec,manager,record,record_bounds,segment,transaction,durability_fence}` | `andromeda-wal` for pure WAL primitives and helpers | Domain-shaped storage import paths for pure WAL owner types. | Active C5 storage compatibility facade. | `wal_ownership_invariants` requires pure WAL facade modules to remain reexport-only; WAL owner tests prove canonical behavior. | Same as RML-007. New pure WAL behavior must be added in `andromeda-wal`, not in storage facades. |
| RML-009 | `andromeda-storage::file_wal` and `andromeda-storage::write_ahead_log::file` | `andromeda-wal` for `FileWal`, header, disk scan, and byte constants; `andromeda-storage` for startup recovery reports and manifest-aware replay planning | Physical FileWal owner items plus storage recovery projection items. | Active mixed C5 facade with owner/integration split. | `crates/andromeda-wal/tests/file_wal_contract.rs`; `crates/andromeda-storage/tests/file_wal_recovery_contract.rs`; `api_compat_reexports`. | Direct physical FileWal callers migrate to `andromeda-wal`; storage keeps only manifest-aware startup recovery, replay planning, and forensic report projection. |
| RML-010 | `andromeda-storage::layout::*` | Root storage modules `page`, `extent`, `segment`, `cold_store`, and `placement` | Layout-domain grouping for root-owned storage layout contracts. | Documented internal domain facade, not an ownership migration away from storage. | `crates/andromeda-storage/tests/layout_facade_invariants.rs`. | No removal required. New layout types must be defined in root owner modules first and then reexported through `layout`. |
| RML-011 | `andromeda-storage::publication::*` | `andromeda-storage::manifest` | Snapshot and manifest publication contracts and validators. | Documented internal domain facade, not an ownership migration away from storage. | `crates/andromeda-storage/tests/publication_facade_invariants.rs`. | No removal required. New publication contracts must be defined in `manifest` first and reexported through `publication`. |
| RML-012 | `andromeda-wal::write_ahead_log::{codec,file,segment}` | `andromeda-wal::{wal_codec,file_wal,wal_segment}` | Owner-crate domain facades for WAL codec, FileWal owner surface, and WAL segment value types. | Stable owner-crate domain facade. | `andromeda-wal` owner tests, including codec, property, and FileWal contract tests. | No migration out of `andromeda-wal`. Do not define duplicate canonical types inside the facade modules. |
| RML-013 | `andromeda-tx` root reexports and `andromeda_tx::mvcc::*` | `andromeda-tx` current owner; future transaction, MVCC, recovery, or lock split crates are not yet accepted in this snapshot | Transaction state, commit log, MVCC, lock manager, savepoints, GC, WAL adapter, and trace types through historical root paths. | Active C5 transaction compatibility facade for future splits. | `crates/andromeda-tx/tests/api_compat_reexports.rs`; transaction lifecycle, WAL replay, MVCC, and lock tests. | Future split crates exist, callers migrate deliberately, and transaction compatibility tests prove historical imports still work until removal is approved. |
| RML-014 | `andromeda-srpl::lexer`, `andromeda-srpl-parser::lexer`, and parser root token reexports | `andromeda-srpl-lexer` | `Token`, `TokenKind`, and `lex` through historical SRPL and parser import paths. | Active SRPL lexer extraction facade. | SRPL lexer library tests; parser owner tests; SRPL facade tests; topology guard for catalog-store-free language-model crates. | Callers import tokenization from `andromeda-srpl-lexer`; `andromeda-srpl` keeps only higher compiler facade responsibilities and `andromeda-srpl-parser` keeps syntax parsing. |

## Owner Evidence Versus Facade Evidence

| Surface | Owner evidence proves | Facade evidence proves | What facade evidence does not prove |
| --- | --- | --- | --- |
| WAL and FileWal | Canonical byte formats, codecs, scan boundaries, physical open/append/flush behavior. | Historical storage imports still resolve to owner types. | Storage startup recovery, manifest integration, page replay, or visible commit correctness. |
| Catalog contracts | Canonical Procedure contract and catalog object descriptor identity. | Historical catalog imports still resolve. | Catalog store durability, DefinitionBatch replay, publication subscription, or Procedure Store runtime behavior. |
| RPC frames | Runtime-free frame and stream contract behavior. | Historical QUIC imports still resolve. | Concrete Quinn runtime behavior, TLS identity, authorization, or durable audit. |
| StructuredObject | Canonical descriptor and row-count policy identity. | Historical proto imports still resolve. | Generated protobuf projection correctness or ResultStream runtime behavior. |
| Storage layout/publication | Canonical storage root module identity. | Domain paths resolve to the same root-owned storage types. | Crash/recovery correctness, manifest durability, or release readiness. |

## Validation

Before removing or changing a facade, run the facade guard and the canonical owner guard. Typical commands include:

```powershell
cargo test -p andromeda-catalog --test contract_facade_compatibility -- --nocapture
cargo test -p andromeda-proto --test structured_object_facade_compatibility -- --nocapture
cargo test -p andromeda-storage --test api_compat_reexports -- --nocapture
cargo test -p andromeda-storage --test wal_ownership_invariants -- --nocapture
cargo test -p andromeda-wal --tests
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
```

For C5 facade changes, add crash/recovery or replay tests that prove the owner/integration split still preserves durable truth.

## Troubleshooting

If a facade module starts defining a new `struct`, `enum`, `trait`, free function, or constant that belongs to the canonical owner, move the definition to the owner before accepting the packet.

If a compatibility test passes but an owner test is missing, do not claim ownership is validated. The facade only proves import stability.

If a caller cannot migrate because the owner crate lacks a public symbol, add the symbol to the owner first and keep the facade until the caller migration and compatibility test both pass.

## References

- `AGENTS.md`
- `crates/README.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `crates/andromeda-core/src/lib.rs`
- `crates/andromeda-catalog/src/contracts.rs`
- `crates/andromeda-catalog/src/names.rs`
- `crates/andromeda-catalog/src/objects.rs`
- `crates/andromeda-proto/src/structured.rs`
- `crates/andromeda-quic/src/lib.rs`
- `crates/andromeda-srpl/src/lib.rs`
- `crates/andromeda-srpl/src/lexer.rs`
- `crates/andromeda-srpl-lexer/src/lib.rs`
- `crates/andromeda-srpl-parser/src/lexer.rs`
- `crates/andromeda-storage/src/wal.rs`
- `crates/andromeda-storage/src/wal_codec.rs`
- `crates/andromeda-storage/src/wal_segment.rs`
- `crates/andromeda-storage/src/file_wal.rs`
- `crates/andromeda-storage/src/layout/mod.rs`
- `crates/andromeda-storage/src/publication.rs`
- `crates/andromeda-storage/src/write_ahead_log/mod.rs`
- `crates/andromeda-wal/src/write_ahead_log/mod.rs`
