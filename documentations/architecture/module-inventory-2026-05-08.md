# Module Inventory - 2026-05-08

## Purpose

This ledger records the current crate and top-level module inventory for Step 0 governance. It gives later workers a shared baseline for ownership, split planning, compatibility facades, and validation scope.

## Scope

This inventory covers the 32 Rust workspace packages reported by `cargo metadata --no-deps --format-version 1` and their current `src/` and `tests/` Rust files. Counts and module lists are taken from the local workspace shape observed on 2026-05-08. The root `Cargo.toml` explicit `members` list contains the same 32 paths in the final Step 0 snapshot.

The inventory is intentionally crate-level and top-level-module-level. It does not list every type, function, private submodule, generated protobuf message, test case, fuzz target, or documentation file.

## Non-goals

- Do not treat this inventory as proof that any module is complete.
- Do not treat source-file counts as quality, readiness, or risk acceptance.
- Do not use this document to approve broad movement of C5 storage, WAL, recovery, transaction, catalog, security, or RPC code.
- Do not use this document to override owner-specific tests, ADRs, or release gates.

## Prerequisites

Before using the inventory, read:

- `AGENTS.md`
- `crates/README.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `documentations/architecture/dependency-edge-matrix-2026-05-08.md`

If a crate or module changes, refresh the relevant row before using this document as planning evidence.

## Procedure

1. List workspace crates from the root `Cargo.toml`.
2. Count Rust source files under each crate's `src/` directory.
3. Count Rust integration test files under each crate's `tests/` directory.
4. Record top-level modules and facade boundaries from `src/lib.rs` and top-level `src/*.rs` or `src/*/` paths.
5. Classify each crate as owner, compatibility facade, tool, evidence crate, or mixed state.
6. Use this ledger to choose the smallest packet that can be validated without touching unrelated work.

## Inventory

| Crate | Source files | Test files | Current top-level module surface | Step 0 posture |
| --- | ---: | ---: | --- | --- |
| `andromeda-bench` | 61 | 14 | Benchmark history, CRUD scenarios, evidence, regression detection, runner, scenario boundary, storage and WAL benchmark modules, workload definitions. | R5 evidence crate. Keep benchmark output advisory and version-bound. |
| `andromeda-catalog` | 120 | 40 | Batch, contracts, dependencies, digest, names, objects, plan cache, procedure feedback, Procedure Store, publication subscription, recovery, scenario evidence, server, snapshot, statistics, store, WAL integration and records. | Broad catalog owner with compatibility reexports and C5 catalog-publication risk. |
| `andromeda-cli` | 65 | 24 | Args parser, audit, benchmark, backup, catalog, protocol, recovery, restore, vertical command, HA/DR, diagnostics, command dispatch, protobuf helpers. | R5 operator interface. Keep Admin, HA/DR, and Application Surface boundaries explicit. |
| `andromeda-codec` | 3 | 0 | Explicit codec error and little-endian helpers. | Dependency-free codec scaffold. Keep disk and network formats explicit; do not introduce native-layout serialization. |
| `andromeda-contract` | 11 | 1 | Contracts, dependency edges, qualified names, catalog object descriptors. | Contract-safe owner crate. |
| `andromeda-core` | 30 | 12 | Principal module plus facade exports for digest, error, hardware, time, and types. | Temporary R0 compatibility facade with principal identity still local. |
| `andromeda-digest` | 2 | 1 | Digest primitive module. | R0 foundation leaf. |
| `andromeda-error` | 2 | 1 | Typed error module. | R0 foundation leaf. |
| `andromeda-exec` | 97 | 101 | Admission, business, dispatch, executor bridge, invocation, local runtime, registry, result, result stream, retry, services, SRPL adapters, surface gate, traces, vertical slice entry, WAL evidence. | Broad execution orchestrator. C4/C5 when it touches admission, transaction creation, visible commit, audit, or recovery visibility. |
| `andromeda-hardware` | 6 | 1 | CPU, GPU policy, integration, pipeline, RAM profiles. | R0 hardware descriptor crate. GPU policy is exclusion vocabulary, not GPU runtime ownership. |
| `andromeda-maps` | 3 | 0 | Descriptor and error modules. | Map scaffold. Keep analytical maps advisory until refresh, summarizability, source, and validation rules are proven. |
| `andromeda-observe` | 77 | 39 | Emitters, events, exporters, principal binding, query, restore trace, trace IDs. | Observability and audit evidence owner. Evidence is not database truth. |
| `andromeda-policy` | 3 | 0 | Admission and identity modules. | Policy scaffold. It must remain bounded by security-contract vocabulary and explicit admission rules. |
| `andromeda-procedure-store` | 6 | 0 | Error, evidence, identity, sink, and status modules. | Procedure Store scaffold. Treat as versioned evidence until durable catalog/runtime integration is proven. |
| `andromeda-proto` | 36 | 42 | Completion, envelope frame and validation, generated messages, generated validation, manifest, payload, structured object facade, versioning. | Typed protocol payload owner with StructuredObject compatibility facade. |
| `andromeda-quic` | 61 | 38 | Backpressure, catalog manifest resolution, connection, HA/DR streams, mTLS identity, procedure gateway, protocol invariants, Quinn backend and TLS behind feature gates, reconnect, RPC, session, stream concurrency, transport, typed envelope, zero-RTT. | R4 transport runtime. Runtime-free protocol ownership belongs to `andromeda-rpc-protocol`. |
| `andromeda-resource` | 3 | 0 | Error and limits modules. | Resource-limit scaffold. Keep limits bounded, observable, and tied to explicit policy when used in admission. |
| `andromeda-rpc-protocol` | 10 | 2 | Backpressure, frame code, frame codec, frame sequence, frame struct, protocol invariants, stream types. | Runtime-free frame and stream contract owner. |
| `andromeda-security-contract` | 6 | 1 | Admission, error, permission, policy, surface vocabulary. | Runtime-free security contract vocabulary. Not IAM runtime. |
| `andromeda-srpl` | 59 | 41 | AST, binder, cardinality, DefinitionBatch bridge, diagnostics, execution adapter, identifier, interpreter, IR, lexer facade, lowering, optimizer, parser, procedure compiler, procedure model, procedure resolver, source location. | SRPL compatibility facade with remaining compiler and catalog-facing bridges. Lexer ownership has moved to `andromeda-srpl-lexer` in the current metadata graph. |
| `andromeda-srpl-ast` | 2 | 1 | AST data shapes and cardinality/source-span reexports. | Extracted language-model owner. |
| `andromeda-srpl-cardinality` | 2 | 0 | Cardinality semantics. | Extracted language-model owner. |
| `andromeda-srpl-diagnostics` | 4 | 0 | Diagnostics and source location. | Extracted diagnostics owner. |
| `andromeda-srpl-ir` | 9 | 0 | Identifier, IR, signature. | Extracted semantic IR owner. |
| `andromeda-srpl-lexer` | 1 | 0 | Token kinds, token records, and lexical scanner in `lib.rs`. | Extracted lexer owner reported by metadata. Keep tokenization catalog-store-free and parser-runtime-free. |
| `andromeda-srpl-parser` | 7 | 1 | Lexer facade and parser modules. | Extracted parser owner that imports tokenization from `andromeda-srpl-lexer`. |
| `andromeda-storage` | 269 | 163 | Backup, B+Tree, B+Tree format validation, B+Tree key codec, buffer pool, catalog WAL bridge, cold store, disk manager, extent, FileWal recovery facade, format version, HA/DR, heap, heap row encoder, layout, LSN facade, manifest, operational profile, page, page codec, placement, publication facade, recovery, restore orchestration, segment index, segment, WAL codec facade, WAL record catalog, WAL segment facade, write-ahead-log domain. | Largest broad C5 durable-kernel owner and compatibility facade. Do not split without owner tests and crash/recovery evidence. |
| `andromeda-structured-object` | 3 | 0 | StructuredObject contract model and hash support. | Contract-safe owner crate. |
| `andromeda-time` | 2 | 1 | Engine timestamp and clock abstractions. | R0 foundation crate. |
| `andromeda-tx` | 96 | 79 | Active snapshot registry, allocator, commit log, commit protocol, deadlock detection, GC, lock history, lock manager, lock protocol, locking protocol, LSN facade, manager, MVCC, savepoints, state, trace, WAL adapter. | Broad C5 transaction-kernel owner and compatibility facade. |
| `andromeda-types` | 3 | 0 | Semantic identifiers and scalar type descriptors. | R0 foundation crate. |
| `andromeda-wal` | 25 | 5 | FileWal, LSN, WAL codec, WAL segment, write-ahead-log domain facade. | C5 WAL owner for pure WAL primitives and physical FileWal byte contracts. |

## Broad Owner Watchlist

| Crate | Why it is a Step 0 watch item | Smallest safe planning rule |
| --- | --- | --- |
| `andromeda-storage` | It owns the widest source surface and many C5 paths: page, heap, B+Tree, manifest, recovery, backup, restore, HA/DR, WAL compatibility, and storage integration. | Split by durable behavior lock and validation gate, not by convenience. |
| `andromeda-catalog` | It combines DefinitionBatch, catalog store, publication, Procedure Store, statistics, plan cache, scenario evidence, and contract facades. | Separate catalog truth from optimizer/evidence scaffolding before claiming C5 readiness. |
| `andromeda-exec` | It bridges admission, SRPL, catalog, storage, transaction, result stream, audit, and QUIC. | Keep Procedure dispatch and transaction creation boundaries explicit. |
| `andromeda-tx` | It combines state, MVCC, locks, GC, commit log, savepoints, and WAL adapter behavior. | Preserve visible-commit and rollback invariants before extracting modules. |
| `andromeda-quic` | It combines runtime-free compatibility exports with concrete transport behavior and feature-gated Quinn modules. | Keep protocol contract ownership in `andromeda-rpc-protocol` and concrete runtime behavior in transport code. |
| `andromeda-srpl` | It remains a compiler facade while extracted language-model crates exist. | Move catalog-facing bridge work only after extracted parser/model crates stay catalog-store-free. |
| `andromeda-proto` | It owns generated protocol material and compatibility projections. | Keep wire/schema generation separate from RPC runtime and StructuredObject contract ownership. |
| `andromeda-bench` | It exercises engine paths and stores evidence. | Never promote benchmark output into durable truth or optimizer authority without bounded, versioned governance. |
| New scaffold crates | `andromeda-codec`, `andromeda-maps`, `andromeda-policy`, `andromeda-procedure-store`, and `andromeda-resource` are small, dependency-free packages in this snapshot. | Assign durable or security authority only after ADR/topology coverage and owner tests exist. |
| `andromeda-srpl-lexer` split | Tokenization now has a dedicated package in the explicit workspace member list and metadata graph. | Keep lexer ownership catalog-store-free and update topology governance with the split. |

## Validation

Refresh this inventory with read-only commands such as:

```powershell
rg --files crates
rg -n "^(pub\s+)?mod\s|^pub\(crate\)\s+mod\s|^pub\s+use\s" crates -g "lib.rs" -g "main.rs"
```

Use package-specific validation when a module row changes. For C4 or C5 modules, add crash/recovery, property, fuzz, Miri, threat-model, or audit evidence as required by the owning subsystem.

## Troubleshooting

If the source-file count changes but the ownership row does not, check whether a worker added hidden responsibility inside a facade. A facade should preserve compatibility and should not become a new owner without a governance update.

If a broad crate gains new unrelated top-level modules, stop and classify the module before accepting the packet. Generic module buckets such as `common`, `utils`, `misc`, and unbounded `helpers` remain suspect unless an existing test or ADR explicitly scopes them.

If a module is deleted in the working tree but still staged, do not use this inventory to decide the final state. Inspect the owning packet and reconcile the path explicitly.

## References

- `AGENTS.md`
- `Cargo.toml`
- `crates/README.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `documentations/architecture/dependency-edge-matrix-2026-05-08.md`
- `documentations/implementation/worktree-packaging-plan-2026-05-08.md`
