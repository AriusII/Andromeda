# Rust Source Layout

This directory contains the Rust workspace crates that implement the Andromeda foundation, protocol, storage, transaction, catalog, SRPL, execution, observability, CLI, and benchmark surfaces.

## Boundary Rules

- `andromeda-error` owns typed engine error kinds and results. It must not depend on higher-level engine crates.
- `andromeda-digest` owns deterministic digest primitives. It must not depend on higher-level engine crates.
- `andromeda-types` owns shared identifiers, contract hashes, and primitive type descriptors. It may depend only on foundation crates.
- `andromeda-time` owns engine timestamps and clock abstractions. It may depend only on foundation crates.
- `andromeda-hardware` owns hardware profiles and C5 exclusion policy descriptors. It may depend only on foundation crates.
- `andromeda-core` is a temporary compatibility facade over foundation crates and principal identity types. It must not regain higher-level engine ownership.
- `andromeda-contract` owns contract-safe Procedure contracts, qualified names, catalog object descriptors, and structural catalog dependency edges. It may depend only on contract-safe foundation crates.
- `andromeda-structured-object` owns contract-safe StructuredObject headers, layout descriptors, descriptor hashing, and row-count metadata policy. It may depend only on contract-safe foundation crates.
- `andromeda-proto` owns custom typed RPC payload contracts. It must not introduce gRPC or make JSON the runtime default.
- `andromeda-quic` owns QUIC transport behavior and maps transport events to typed protocol boundaries.
- `andromeda-catalog` owns catalog storage, DefinitionBatch behavior, plan cache identity, statistics metadata, publication, and WAL-facing catalog codecs. It temporarily reexports `andromeda-contract` types for compatibility.
- `andromeda-srpl-diagnostics` owns SRPL source spans, diagnostic phases, forbidden construct diagnostics, and source validation. It must not depend on parser, catalog store, execution, storage, or transport crates.
- `andromeda-srpl-cardinality` owns SRPL result cardinality semantics and contract cardinality conversion. It must not depend on catalog store or runtime crates.
- `andromeda-srpl-ast` owns SRPL syntax data shapes. It must not lex, parse, bind, lower, execute, or depend on catalog store.
- `andromeda-srpl-parser` owns SRPL tokenization and syntax parsing. It must not depend on catalog store, execution, storage, transport, or benchmark crates.
- `andromeda-srpl-ir` owns bounded semantic IR and procedure signature data shapes. It must not depend on catalog store, execution, storage, transport, or benchmark crates.
- `andromeda-srpl` is the temporary compatibility facade for parsing, binding, lowering, optimizer, interpreter, DefinitionBatch bridge, and compiler-facing SRPL semantics.
- `andromeda-wal` owns pure WAL primitives, LSNs, WAL records, segment descriptors, frame codecs, scan-prefix validation, record bounds, durability fence helpers, in-memory WAL summaries, and the physical FileWal byte contract and file-backed open/append/header/scan surface. It may depend only on WAL-safe foundation crates.
- `andromeda-tx` owns transaction state, WAL durability gates, MVCC visibility, locks, savepoints, and recovery-facing transaction evidence.
- `andromeda-storage` owns page, heap, B+Tree, checkpoint, manifest, recovery reports, startup planning, replay selection, manifest/page integration, and durable visibility integration. It temporarily reexports WAL and FileWal types for compatibility.
- C5 durable-kernel crates, including current `andromeda-wal`, `andromeda-storage`, and `andromeda-tx` plus future recovery, cold-store, buffer-pool, page-layout, and MVCC splits, must not depend on SRPL parser/model crates, catalog store implementations, protocol runtime crates, QUIC runtime crates, execution crates, benchmark/analytics/GPU crates, SQL crates, or implicit native-layout serialization dependencies.
- Future C5 extractions must keep `andromeda-storage` and `andromeda-tx` as temporary compatibility facades until public reexport tests pass for existing callers. WAL and physical FileWal compatibility paths must keep reexport tests passing until callers migrate to `andromeda-wal` directly.
- Persistent WAL, page, heap, B+Tree, manifest, backup, and recovery formats require explicit codecs, byte-for-byte roundtrip/golden tests, corruption rejection, and crash/recovery validation before any split is accepted.
- `andromeda-exec` owns execution orchestration over cataloged Procedures. It must not create an ad hoc SQL application surface.
- `andromeda-observe` owns typed traces, audit evidence, and post-fact decision explainability.
- `andromeda-cli` is an operator/developer interface over bounded workspace commands and must not bypass cataloged Procedure contracts for application execution.
- `andromeda-bench` owns deterministic benchmark harnesses and advisory evidence. Predictive evidence remains advisory only.

## Source Policy

- Keep crate dependencies acyclic and aligned with engine ownership.
- Keep every externally visible contract typed, versioned, and test-covered.
- Keep durable state changes bound to WAL/recovery evidence before visible publication.
- Keep GPU, SIMD, and analytics acceleration outside commit, rollback, WAL, recovery, MVCC visibility, and security-critical paths.
- Prefer deterministic fixtures and generated corpora over opaque binary blobs.
- Treat `target/` as disposable build output. Do not reference files in `target/` from source, tests, documentation, fuzz corpora, or CI logic.

## Validation Gates

Run these before accepting source changes:

```powershell
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
