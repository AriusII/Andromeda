# Rust Source Layout

## Purpose

This directory contains the Rust workspace crates that implement the Andromeda foundation, protocol, storage, transaction, catalog, SRPL, execution, observability, CLI, and benchmark surfaces.

## Scope

The workspace uses explicit crate ownership boundaries:

- `andromeda-error` owns typed engine error kinds and results. It must not depend on higher-level engine crates.
- `andromeda-digest` owns deterministic digest primitives. It must not depend on higher-level engine crates.
- `andromeda-types` owns shared identifiers, contract hashes, and primitive type descriptors. It may depend only on foundation crates.
- `andromeda-time` owns engine timestamps and clock abstractions. It may depend only on foundation crates.
- `andromeda-hardware` owns hardware profiles and C5 exclusion policy descriptors. It may depend only on foundation crates.
- `andromeda-core` is a temporary compatibility facade over foundation crates and principal identity types. It must not regain higher-level engine ownership.
- `andromeda-security-contract` is the R1 runtime-free security contract vocabulary crate: surfaces, permission families, operation classes, canonical permission identifiers, stable labels, and explicit semantic mappings. It must not depend on IAM registries, mutable policy stores, revocation stores, QUIC/TLS runtimes, catalog stores, execution, WAL, storage, or recovery crates, and it must not be described as the IAM runtime. The current `andromeda-core` principal facade dependency is a documented temporary exception, not a runtime ownership claim.
- `andromeda-contract` owns contract-safe Procedure contracts, qualified names, catalog object descriptors, and structural catalog dependency edges. It may depend only on contract-safe foundation crates.
- `andromeda-structured-object` owns contract-safe StructuredObject headers, layout descriptors, descriptor hashing, and row-count metadata policy. It may depend only on contract-safe foundation crates.
- `andromeda-proto` owns custom typed RPC payload contracts. It must not introduce gRPC, tonic, generated gRPC services, or JSON as the runtime default.
- `andromeda-rpc-protocol` owns runtime-free RPC frame contracts, stream roles, explicit frame codecs, and ResultStream frame sequencing. It must not depend on QUIC runtime crates, TLS crates, async runtimes, executor, storage, WAL, or recovery.
- `andromeda-quic` owns concrete QUIC transport behavior and maps transport events to typed protocol boundaries. Optional Quinn/Rustls/Tokio code belongs behind this runtime boundary; it must not own Procedure semantics, storage truth, WAL authority, or authorization policy. It temporarily reexports `andromeda-rpc-protocol` frame and stream types for compatibility until callers import protocol contracts directly and API compatibility tests prove no public import regressions.
- `andromeda-catalog` owns catalog storage, DefinitionBatch behavior, plan cache identity, statistics metadata, publication, and WAL-facing catalog codecs. It temporarily reexports `andromeda-contract` types for compatibility.
- `andromeda-srpl-diagnostics` owns SRPL source spans, diagnostic phases, forbidden construct diagnostics, and source validation. It must not depend on parser, catalog store, execution, storage, or transport crates.
- `andromeda-srpl-cardinality` owns SRPL result cardinality semantics and contract cardinality conversion. It must not depend on catalog store or runtime crates.
- `andromeda-srpl-ast` owns SRPL syntax data shapes. It must not lex, parse, bind, lower, execute, or depend on catalog store.
- `andromeda-srpl-parser` owns SRPL tokenization and syntax parsing. It must not depend on catalog store, execution, storage, transport, or benchmark crates.
- `andromeda-srpl-ir` owns bounded semantic IR and procedure signature data shapes. It must not depend on catalog store, execution, storage, transport, or benchmark crates.
- `andromeda-srpl` is the temporary compatibility facade for parsing, binding, lowering, optimizer, interpreter, DefinitionBatch bridge, and compiler-facing SRPL semantics. Its exit criterion is a dedicated SRPL bridge crate plus direct caller migration to extracted language-model crates, with topology guards proving parser/model crates remain catalog-store-free and dev-dependency-bounded.
- `andromeda-wal` owns pure WAL primitives, LSNs, WAL records, segment descriptors, frame codecs, scan-prefix validation, record bounds, durability fence helpers, in-memory WAL summaries, and the physical FileWal byte contract and file-backed open/append/header/scan surface. It may depend only on WAL-safe foundation crates.
- `andromeda-tx` owns transaction state, WAL durability gates, MVCC visibility, locks, savepoints, and recovery-facing transaction evidence.
- `andromeda-storage` owns page, heap, B+Tree, checkpoint, manifest, recovery reports, startup planning, replay selection, manifest/page integration, and durable visibility integration. It temporarily reexports WAL and FileWal types for compatibility.
- C5 durable-kernel crates, including current `andromeda-wal`, `andromeda-storage`, and `andromeda-tx` plus future recovery, cold-store, buffer-pool, page-layout, and MVCC splits, must not depend on SRPL parser/model crates, catalog store implementations, protocol runtime crates, QUIC runtime crates, execution crates, benchmark/analytics/GPU crates, SQL crates, or implicit native-layout serialization dependencies.
- Future C5 extractions must keep `andromeda-storage` and `andromeda-tx` as temporary compatibility facades until public reexport tests pass for existing callers. WAL and physical FileWal compatibility paths must keep reexport tests passing until callers migrate to `andromeda-wal` directly.
- Persistent WAL, page, heap, B+Tree, manifest, backup, and recovery formats require explicit codecs, byte-for-byte roundtrip/golden tests, corruption rejection, and crash/recovery validation before any split is accepted.
- `andromeda-exec` owns execution orchestration over cataloged Procedures. It must not create an ad hoc SQL application surface.
- `andromeda-observe` owns typed traces, audit evidence, durable `AuditLedger v0` journal evidence, and post-fact decision explainability. Audit records are append-only and checksum chained at the record layer; retention compaction may rewrite retained records with rethreaded chain evidence, and audit must not become storage truth or the transaction commit path.
- `andromeda-cli` is an operator/developer interface over bounded workspace commands and must not bypass cataloged Procedure contracts for application execution.
- `andromeda-bench` owns deterministic benchmark harnesses and advisory evidence. Predictive evidence remains advisory only.

## Non-goals

- Do not introduce application-facing ad hoc SQL or a general SQL compatibility surface.
- Do not create generic ownership buckets such as `common`, `utils`, `misc`, `helpers`, or `god_engine`.
- Do not use support crates, benchmark crates, observability crates, or CLI crates to bypass Procedure contracts, WAL durability, catalog publication, or security boundaries.
- Do not treat RAM, temporary storage, GPU output, benchmark output, trace output, or diagnostic JSON as durable truth.

## Prerequisites

- Read the repository root `AGENTS.md` and any deeper `AGENTS.md` files before changing files under `crates/`.
- Run commands from the workspace root unless a crate README says otherwise.
- Keep Rust 2024 Edition settings and workspace dependency rules intact.
- Identify the owning crate before adding APIs, tests, or documentation.

## Procedure

- Keep crate dependencies acyclic and aligned with engine ownership.
- Do not add generic ownership buckets named `common`, `utils`, `misc`, `helpers`, or `god_engine`; create responsibility-named crates instead.
- Keep every externally visible contract typed, versioned, and test-covered.
- Keep durable state changes bound to WAL/recovery evidence before visible publication.
- Keep GPU, SIMD, and analytics acceleration outside commit, rollback, WAL, recovery, MVCC visibility, and security-critical paths.
- Prefer deterministic fixtures and generated corpora over opaque binary blobs.
- Treat `target/` as disposable build output. Do not reference files in `target/` from source, tests, documentation, fuzz corpora, or CI logic.

## Operational and Support Crates

Use the crate README files for operator-facing and support-surface guidance:

- [`andromeda-cli`](andromeda-cli/README.md) documents the operator and developer CLI surface. It is administration-only and must not become an application-facing runtime, ad hoc SQL shell, or bypass around cataloged Procedure contracts.
- [`andromeda-bench`](andromeda-bench/README.md) documents bounded benchmark harnesses and advisory evidence. Benchmark output can inform diagnosis and regression review, but it is not storage truth, catalog truth, optimizer authority, or a standalone plan-selection input.
- [`andromeda-observe`](andromeda-observe/README.md) documents typed traces, durable audit evidence, bounded query contracts, and audit-safe exporter behavior. Observability output supports explanation and review, but it must not become storage truth or the transaction commit path.
- [`andromeda-structured-object`](andromeda-structured-object/README.md) documents explicit StructuredObject metadata contracts. Payload bytes must be admitted only after the header, descriptor hash, row-count policy, and payload bounds validate.

## Validation

Run these before accepting source changes:

```powershell
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
cargo test -p andromeda-cli --test orphan_source_invariants -- --nocapture
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python fuzz/generators/generate_seed_corpus.py --check
```

Use narrower package-level commands while developing, but the full workspace gates are the acceptance baseline.

For Lot 4.5 FileWal ownership documentation and implementation gates, keep owner evidence and integration evidence separate:

```powershell
cargo test -p andromeda-wal --test file_wal_contract -- --nocapture
cargo test -p andromeda-storage --test api_compat_reexports -- --nocapture
cargo test -p andromeda-storage --test wal_ownership_invariants -- --nocapture
cargo test -p andromeda-storage --test file_wal_recovery_contract -- --nocapture
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
```

The `andromeda-wal` command is physical FileWal owner evidence. The storage commands are API compatibility, ownership-boundary, and recovery integration evidence. The topology command proves the crate graph remains inside the accepted durable-kernel dependency rings.

## Troubleshooting

- If a dependency creates a cycle or crosses an ownership boundary, move the contract to the lower-level owning crate or add a responsibility-named crate.
- If a crate starts accumulating unrelated APIs, split by engine responsibility instead of adding generic helper modules.
- If a support surface appears to make runtime decisions authoritative, re-check the owning runtime, WAL, catalog, security, and audit boundaries.
- If a validation gate is too broad while developing, run a narrow package-level command first and keep the full workspace gate as the acceptance baseline.

## References

- [Repository operating instructions](../AGENTS.md)
- [`andromeda-cli`](andromeda-cli/README.md)
- [`andromeda-bench`](andromeda-bench/README.md)
- [`andromeda-observe`](andromeda-observe/README.md)
- [`andromeda-structured-object`](andromeda-structured-object/README.md)
