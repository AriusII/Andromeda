# Workspace Crate Boundaries

## Status

Accepted

## Purpose

Define the dependency rings for the Rust workspace restructuring and record the temporary role of compatibility facades during the transition.

## Scope

This ADR applies to Cargo workspace crate boundaries, dependency direction, and governance tests for the Andromeda engine crates.

The first implementation batch introduces foundation crates, keeps `andromeda-core` as a compatibility facade, and adds dependency-topology validation. Lot 2.1 introduces `andromeda-contract` as the R1a contract model crate and keeps `andromeda-catalog` reexports for compatibility. Lot 2.2 introduces `andromeda-structured-object` as the R1b owner for contract-safe StructuredObject metadata, descriptor hashing, and row-count metadata policy while `andromeda-proto` keeps schema generation and compatibility reexports. Lot 3 introduces SRPL language-model crates for diagnostics, cardinality, AST, parser, and IR while `andromeda-srpl` remains the compatibility facade for binder, lowering, optimizer, interpreter, DefinitionBatch bridge, and historical public imports. Lot 4.0 adds C5 durable-kernel dependency governance before any WAL, storage, recovery, page-format, or transaction-runtime extraction. These batches do not change runtime behavior, persistent formats, network formats, WAL semantics, storage semantics, Procedure dispatch, or public compatibility paths.

## Non-goals

- Do not introduce application-facing SQL or a SQL-compatible dependency surface.
- Do not bypass typed, cataloged Procedure contracts.
- Do not move WAL, storage, recovery, catalog store, RPC, execution, SRPL, benchmark, or analytics runtime code in these batches.
- Do not remove the temporary `andromeda-core` facade until downstream crates have migrated to foundation crates.
- Do not make GPU, analytics, benchmark, or learned output part of commit, WAL, rollback, recovery, catalog publication, or security-critical paths.

## Context

Andromeda is a strict relational transactional database engine with a contract-first RPC surface. The workspace currently contains foundation crates, engine subsystem crates, runtime crates, and tools that are being separated from a broad `andromeda-core` dependency.

The restructuring must preserve these invariants:

- Application access remains RPC-only through typed, cataloged Procedures.
- Visible commit requires durable WAL.
- Recovery truth remains the latest valid cold snapshot plus durable WAL from that snapshot.
- Persistent and network formats use explicit codecs, not Rust native struct layout.
- GPU and learned components remain outside commit, WAL, rollback, recovery, catalog publication, and security-critical paths.
- Optimization choices remain bounded, observable, explainable, versioned, and disableable.

## Decision

Adopt dependency rings. Dependencies may point only to the same ring or a lower ring unless a later ADR records a bounded exception with validation.

| Ring | Responsibility | Examples | Dependency rule |
|---|---|---|---|
| R0 Foundation | Stable identifiers, scalar types, time, digest, typed errors, hardware descriptors with no engine ownership | `andromeda-error`, `andromeda-digest`, `andromeda-types`, `andromeda-time`, `andromeda-hardware` | May not depend on engine, execution, RPC runtime, benchmark, analytics, or GPU execution crates. |
| R1 Contracts and language model | Procedure contracts, catalog object descriptors, SRPL AST/IR contracts, protocol schemas, abstract RPC payload contracts | `andromeda-contract`, `andromeda-structured-object`, `andromeda-catalog`, `andromeda-srpl`, `andromeda-proto` | May depend on R0. Must not depend on execution engines, storage runtime implementations, Quinn runtime, or benchmark crates. |
| R2 Durable kernel | WAL, storage, recovery, MVCC, transaction state, cold snapshots, manifest publication | `andromeda-storage`, `andromeda-tx`, later `andromeda-wal`, `andromeda-recovery`, `andromeda-cold-store`, `andromeda-buffer-pool`, `andromeda-storage-page`, `andromeda-tx-mvcc` | May depend on R0, typed observability contracts, and narrowly on R1 contracts. Must not depend on SRPL parser/model crates, catalog store implementations, protocol or QUIC runtime crates, GPU, analytics, benchmark crates, SQL crates, or implicit native-layout serialization dependencies. |
| R3 Execution | Procedure execution, transaction orchestration, adapter boundaries, result sequencing | `andromeda-exec` | May depend on R0, R1 contracts, and R2 durable kernel APIs. Must not become a dependency of catalog contracts or SRPL parser-only crates. |
| R4 Transport runtime | QUIC session runtime, Quinn backend, reconnect policy, stream dispatch | `andromeda-quic` | May depend on R0 and protocol contracts. Quinn stays isolated to transport runtime crates and features. |
| R5 Tools and evidence | CLI, benchmarks, workload evidence, offline analytics, test harnesses | `andromeda-cli`, `andromeda-bench` | May depend downward to exercise the engine. Must not be used by R0 through R4 production crates. |

The temporary `andromeda-core` facade remains an R0 compatibility facade during batch 1. It may re-export or bridge foundation crates while callers are migrated. It must not regain engine ownership, durable truth ownership, RPC runtime ownership, benchmark ownership, analytics ownership, or GPU execution ownership. Each later batch should reduce direct reliance on the facade in favor of the precise foundation crate.

`andromeda-contract` is the R1a owner for contract-safe Procedure contracts, qualified names, catalog object descriptors, and structural dependency edges. It may depend only on `andromeda-error`, `andromeda-types`, and `andromeda-digest`. It must not depend on `andromeda-catalog`, `andromeda-proto`, SRPL, execution, durable kernel crates, Quinn, `prost`, benchmark, analytics, or GPU crates.

`andromeda-structured-object` is the R1b owner for StructuredObject headers, layout descriptors, descriptor-hash construction, and the row-count metadata policy shared by StructuredObject headers and handwritten result-stream descriptors. It may depend only on `andromeda-error`, `andromeda-types`, and `andromeda-digest`. It must not depend on `andromeda-contract`, `andromeda-catalog`, `andromeda-proto`, SRPL, execution, durable kernel crates, Quinn, `prost`, benchmark, analytics, or GPU crates. Protobuf schemas, generated messages, descriptor-set hashing, runtime projection validators, and transport framing remain owned by `andromeda-proto` or transport crates.

`andromeda-srpl-diagnostics`, `andromeda-srpl-cardinality`, `andromeda-srpl-ast`, `andromeda-srpl-parser`, and `andromeda-srpl-ir` are R1 language-model crates. They must not depend on catalog store implementations, execution, storage, transaction, transport runtime, protocol runtime, benchmark, analytics, or GPU crates. Parser and AST crates may use `andromeda-contract` for contract-safe identifiers such as `QualifiedName`; they must not use the `andromeda-catalog` facade as a shortcut to catalog store code. `andromeda-srpl` remains the temporary compatibility facade and the only SRPL crate in this batch that may keep catalog-facing bridge modules.

Lot 4.0 freezes durable-kernel dependency direction before any C5 extraction. `andromeda-storage` and `andromeda-tx` remain the compatibility facades for existing callers while future WAL, recovery, page-format, buffer-pool, cold-store, and MVCC crates are introduced behind the current public imports. A C5 split must first preserve public reexports, then prove byte-for-byte compatibility for persistent formats with roundtrip and golden-vector tests, and finally pass crash/recovery validation for any path that can affect commit visibility, rollback, replay, page flush, manifest publication, or recovery startup.

## Temporary exceptions

The following exceptions are accepted only for the migration window that starts with batch 1. They do not authorize new behavior, new runtime coupling, or additional dependencies.

| Exception | Current reason | Exit criterion |
|---|---|---|
| `andromeda-srpl` depends on `andromeda-catalog` for catalog-facing bridge modules. | Lot 3 moves diagnostics, cardinality, AST, parser, and IR into catalog-store-free crates, but binder/lowering, DefinitionBatch dry-run integration, procedure resolver, and contract materialization still live behind the historical facade. | A later SRPL bridge batch moves catalog-facing DefinitionBatch and resolver code behind a dedicated bridge crate, then the parser/model/compiler-core crates must remain free of catalog store dependencies. |
| `andromeda-exec` depends on `andromeda-quic` through the existing executor bridge. | The execution-to-transport bridge predates the ring policy and is outside the batch 1 foundation extraction. | A later RPC/transport batch moves the bridge to an R4 adapter or narrows it behind abstract protocol contracts, then R3 execution must not depend on Quinn-backed transport runtime crates. |
| `andromeda-core` still owns the `principal` module. | Principal identity types were not part of the batch 1 foundation extraction and remain behind the compatibility facade for existing callers. | A later security/IAM batch extracts principal identity and policy contracts to a dedicated crate, then `andromeda-core` becomes reexports only or is retired. |
| `andromeda-catalog` reexports `andromeda-contract` types. | Lot 2.1 keeps existing public imports such as `andromeda_catalog::ProcedureContract`, `andromeda_catalog::QualifiedName`, and `andromeda_catalog::CatalogDefinition` valid while downstream crates migrate. | A later compatibility cleanup removes the catalog facade only after direct callers have migrated to `andromeda-contract` and workspace gates prove no public import regressions. |
| `andromeda-proto` reexports `andromeda-structured-object` types. | Lot 2.2 keeps existing public imports such as `andromeda_proto::StructuredObjectHeader`, `andromeda_proto::StructuredObjectLayout`, `andromeda_proto::RowCountPolicy`, and `andromeda_proto::RowCountRequirement` valid while the protocol crate remains the historical caller-facing path. | A later compatibility cleanup removes the protocol facade only after direct callers have migrated to `andromeda-structured-object` and protocol compatibility tests prove no public import regressions. |
| `andromeda-storage` and `andromeda-tx` still depend on `andromeda-core` and `andromeda-observe`; `andromeda-tx` also depends on async runtime support. | Lot 4.0 is a governance gate, not a durable-kernel split. Existing C5 code uses the compatibility facade and typed observability while transaction APIs still expose async commit/lock boundaries. | A later durable-kernel batch moves direct foundation imports off `andromeda-core`, keeps observability as post-fact evidence rather than truth, and isolates async runtime support from pure commit/MVCC state where practical. |

## Procedure

Use this order for later restructuring batches:

1. Stabilize foundation crates and keep `andromeda-core` as a thin facade.
2. Move direct type, error, digest, time, and hardware descriptor imports from subsystem crates to foundation crates.
3. Separate contract-only crates from runtime implementation crates, starting with `andromeda-contract`.
4. Split SRPL parser and binder concerns away from catalog store implementations.
5. Freeze WAL, storage, recovery, page-format, and transaction public imports before any C5 move.
6. Extract durable-kernel code in small phases: API compatibility tests, pure codecs and validators, recovery planning, replay drivers, transaction payload formats, then I/O/runtime adapters.
7. Keep WAL, storage, recovery, and MVCC free of Quinn, RPC runtime, protocol runtime, SRPL parser/model crates, catalog store implementations, benchmark, analytics, GPU, SQL, and implicit native-layout serialization dependencies.
8. Keep abstract protocol and RPC contract crates free of Quinn runtime dependencies.
9. Move tool-only and benchmark-only dependencies to R5 crates.
10. Retire `andromeda-core` only after no production crate needs it as a facade.

## Validation

The workspace dependency topology must be guarded by an integration test that reads Cargo manifests and rejects forbidden edges at minimum:

- Foundation crates and the temporary `andromeda-core` facade must not depend on higher-level engine crates.
- C5 or durable kernel crates must not depend on GPU, analytics, or benchmark crates.
- WAL, storage, and recovery crates must not depend on Quinn or RPC runtime crates.
- Durable-kernel crates must not depend on SRPL parser/model crates, catalog store implementations, execution crates, protocol runtime crates, SQL crates, or implicit native-layout serialization dependencies.
- Catalog and Procedure contract crates must not depend on execution crates.
- `andromeda-contract` must depend only on the contract-safe R0 crates allowed by the topology test.
- `andromeda-structured-object` must depend only on the contract-safe R0 crates allowed by the topology test.
- SRPL parser crates must not depend on catalog store implementation crates.
- Abstract RPC or protocol contract crates must not depend on Quinn runtime crates.

Lot 4.0 additionally guards the current direct dependencies of `andromeda-storage` and `andromeda-tx` with explicit allowlists. Expanding those allowlists requires an ADR update or a same-lot governance note that explains why the dependency is C5-safe.

The batch 1 validation gate is:

```bash
cargo test -p andromeda-cli --test workspace_dependency_topology
```

The same gate also keeps the batch 1 `andromeda-core` facade bounded to `lib.rs` and the temporary `principal/` module until a later security/IAM batch extracts those types.

When later batches edit Rust code, run the applicable Rust gates for the touched crates, including `cargo fmt --all --check`, `cargo check`, clippy, and targeted tests. WAL, storage, recovery, security, RPC, or catalog behavior changes require crash/recovery, property, fuzz, Miri, or threat-model validation as appropriate.

## Risks

The main residual risk is that `andromeda-core` can hide dependency drift if it grows again. Keep it thin, temporary, and observable through manifest tests and review.

Another risk is that crate names may change before the topology test is updated. If a crate is renamed or split, update the governance test in the same change that updates the manifest.

## References

- `AGENTS.md`
- `docs/AGENTS.md`
- `Cargo.toml`
- `crates/README.md`
- `documentations/governance/decisions/DEC-011-rust-workspace-topology.md`
- `documentations/governance/decisions/DEC-014-rust-crate-module-structure.md`
