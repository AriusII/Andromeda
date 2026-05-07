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
- `andromeda-srpl` owns parsing, binding, and compiler-facing SRPL semantics.
- `andromeda-tx` owns transaction state, WAL durability gates, MVCC visibility, locks, savepoints, and recovery-facing transaction evidence.
- `andromeda-storage` owns page, heap, B+Tree, WAL-record, checkpoint, and recovery-planning storage surfaces.
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
