# Cargo Dependency Matrix

**Generated**: 2026-05-08  
**Source**: `cargo metadata --format-version 1`  
**Andromeda Version**: From commit `1486f240d85b9dec29adf4736d4bbd15f2f1dbb8`

## Summary

- **Total Crates**: 96
- **Total Rust Files**: 1,954
- **Workspace Root**: `Cargo.toml` with workspace members
- **Dependency Type Classes**: normal, dev, build, feature-gated, optional

## Crate Responsibility Classification

### Storage & Persistence (C4-C5)

| Crate | Responsibility | Deps | Status |
|-------|-----------------|------|--------|
| `andromeda-wal` | Write-ahead log implementation | storage, codec, transaction | **C5 CRITICAL** |
| `andromeda-storage` | Page storage, buffer pool, WAL | wal, buffer-pool, manifest | **C5 CRITICAL** |
| `andromeda-manifest` | Immutable segment metadata | storage-page, codec | **C4** |
| `andromeda-buffer-pool` | In-memory page cache | storage-page, hardware | **C4** |
| `andromeda-backup` | Backup/restore orchestration | storage, wal, recovery | **C4** |
| `andromeda-restore` | Point-in-time restore | storage, recovery, wal | **C4** |
| `andromeda-segment` | Segment management | storage, manifest | **C3** |
| `andromeda-disk-page-store` | Physical page I/O | storage-page | **C3** |
| `andromeda-recovery` | Crash recovery orchestration | storage, wal, transaction | **C5 CRITICAL** |

### Transactions & MVCC (C4-C5)

| Crate | Responsibility | Deps | Status |
|-------|-----------------|------|--------|
| `andromeda-transaction` | Transaction state machine | wal, mvcc, locking | **C5 CRITICAL** |
| `andromeda-tx` | Transaction manager, commit log | wal, transaction, mvcc | **C5 CRITICAL** |
| `andromeda-commit-log` | Commit log (via tx module) | wal, codec | **C5 CRITICAL** |
| `andromeda-mvcc` | MVCC version snapshot mgmt | transaction | **C4** |
| `andromeda-locking` | Lock manager, deadlock detect | transaction | **C4** |
| `andromeda-savepoint` | Savepoint stack | transaction | **C3** |

### Catalog & Procedure (C4-C5)

| Crate | Responsibility | Deps | Status |
|-------|-----------------|------|--------|
| `andromeda-catalog` | Database catalog store | wal, catalog-store, procedure-store | **C5 CRITICAL** |
| `andromeda-catalog-store` | Catalog storage abstraction | storage, codec | **C4** |
| `andromeda-catalog-recovery` | Catalog recovery from WAL | catalog, wal, recovery | **C4** |
| `andromeda-catalog-diff` | Catalog mutation tracking | catalog, codec | **C3** |
| `andromeda-procedure-store` | Procedure registry & history | catalog, types, audit | **C4** |
| `andromeda-procedure-contract` | Procedure contract definitions | types, proto | **C4** |
| `andromeda-definition-batch` | Batch procedure import | catalog, procedure-store | **C4** |

### RPC & Protocol (C4-C5)

| Crate | Responsibility | Deps | Status |
|-------|-----------------|------|--------|
| `andromeda-quic` | QUIC transport (only RPC) | quic-runtime-quinn, protocol, codec | **C5 CRITICAL** |
| `andromeda-quic-runtime-quinn` | Quinn-based QUIC backend | quic, codec | **C4** |
| `andromeda-rpc` | RPC dispatch & routing | rpc-protocol, quic, contract | **C4** |
| `andromeda-rpc-protocol` | RPC frame & envelope protocol | codec, proto-wire | **C4** |
| `andromeda-rpc-codec` | RPC message encoding | codec | **C3** |
| `andromeda-protocol` | Higher-level protocol semantics | types, error | **C3** |
| `andromeda-proto-wire` | Protobuf wire layer | proto | **C3** |

### Security & IAM (C4-C5)

| Crate | Responsibility | Deps | Status |
|-------|-----------------|------|--------|
| `andromeda-security` | Security policy enforcement | security-contract, iam | **C5 CRITICAL** |
| `andromeda-security-contract` | Security API surface | types, error, contract | **C4** |
| `andromeda-iam` | Identity & access management | core, procedure-contract | **C4** |
| `andromeda-admission` | Admission control gating | resource, iam, types | **C4** |
| `andromeda-audit` | Audit logging & emitter | decision-trace, types | **C4** |
| `andromeda-policy` | Authorization policy eval | types | **C3** |

### Execution (C3-C4)

| Crate | Responsibility | Deps | Status |
|-------|-----------------|------|--------|
| `andromeda-exec` | Procedure execution engine | transaction, catalog, security | **C4** |
| `andromeda-execution` | Execution plan runner | executor | **C3** |
| `andromeda-procedure-runtime` | Procedure invocation runtime | procedure-store, exec | **C3** |
| `andromeda-srpl-execution-adapter` | SRPL execution integration | srpl, execution | **C3** |

### SRPL & Optimization (C3-C4)

| Crate | Responsibility | Deps | Status |
|-------|-----------------|------|--------|
| `andromeda-srpl` | SRPL language & surface | srpl-parser, srpl-ir, catalog | **C3** |
| `andromeda-srpl-parser` | SRPL lexer/parser | srpl-lexer, srpl-ast | **C3** |
| `andromeda-srpl-lexer` | Tokenization | srpl-ast | **C3** |
| `andromeda-srpl-ast` | Abstract syntax tree | types | **C2** |
| `andromeda-srpl-ir` | Intermediate representation | srpl-ast, types | **C3** |
| `andromeda-srpl-lowering` | AST → IR lowering | srpl-ast, srpl-ir | **C3** |
| `andromeda-srpl-binder` | Name & type binding | srpl-ast, srpl-cardinality | **C3** |
| `andromeda-srpl-cardinality` | Cardinality analysis | srpl-ast | **C3** |
| `andromeda-srpl-diagnostics` | Error reporting | srpl-ast | **C2** |
| `andromeda-srpl-interpreter` | Execution interpreter | srpl-ir, storage | **C3** |
| `andromeda-optimizer` | Query optimizer | srpl-ir, plan-cache, statistics | **C3** |
| `andromeda-plan-cache` | Plan result caching | types | **C3** |

### Types, Core & Utilities (C1-C3)

| Crate | Responsibility | Deps | Status |
|-------|-----------------|------|--------|
| `andromeda-types` | Core type definitions | codec, time | **C2** |
| `andromeda-core` | Core engine abstractions | types, error | **C2** |
| `andromeda-contract` | Contract surface abstraction | types, error | **C2** |
| `andromeda-contract-compat` | Legacy compatibility shims | contract, types | **C1** |
| `andromeda-error` | Error types & codes | (no deps) | **C2** |
| `andromeda-codec` | Binary codec utilities | types, simd | **C2** |
| `andromeda-proto` | Protobuf message defs | proto-wire, types | **C2** |
| `andromeda-result-stream` | Streaming result protocol | error, codec | **C2** |
| `andromeda-resource` | Resource limit definitions | types | **C1** |
| `andromeda-procedure-contract` | Procedure typing | types, proto | **C2** |
| `andromeda-observability` | Observability types | types | **C1** |
| `andromeda-observe` | Event emitter/tracer | observability, audit | **C2** |
| `andromeda-decision-trace` | Decision trace logging | types, audit | **C2** |
| `andromeda-execution-trace` | Execution trace types | types | **C1** |
| `andromeda-scenario-evidence` | Benchmark evidence collection | types | **C1** |
| `andromeda-retry` | Retry policies | (no deps) | **C1** |
| `andromeda-time` | Time utilities | (no deps) | **C1** |
| `andromeda-digest` | Hashing utilities | codec, simd | **C1** |
| `andromeda-structured-object` | Typed object storage | types, storage | **C2** |
| `andromeda-vector` | Vector type support | types, storage-heap | **C2** |

### Analytics & Statistics (C1-C3)

| Crate | Responsibility | Deps | Status |
|-------|-----------------|------|--------|
| `andromeda-statistics` | Statistics collection | types, storage | **C2** |
| `andromeda-analytics` | Analytics query layer | storage, optimizer | **C2** |
| `andromeda-maps` | Materialized view/map support | storage, storage-index | **C2** |
| `andromeda-columnar` | Columnar storage format | storage-page, simd | **C2** |

### Hardware & Performance (C1-C3)

| Crate | Responsibility | Deps | Status |
|-------|-----------------|------|--------|
| `andromeda-hardware` | Hardware profile detection | types | **C1** |
| `andromeda-gpu` | GPU analytics path (off commit) | hardware | **C1** |
| `andromeda-simd` | SIMD kernels | hardware | **C2** |

### HA/DR (C3-C4)

| Crate | Responsibility | Deps | Status |
|-------|-----------------|------|--------|
| `andromeda-hadr` | High availability & disaster recovery | transaction, wal, recovery, quorum | **C4** |

### Storage Index (C2-C3)

| Crate | Responsibility | Deps | Status |
|-------|-----------------|------|--------|
| `andromeda-storage-page` | Page format & codec | codec, simd | **C3** |
| `andromeda-storage-index` | Index structures (B+Tree) | storage-page, codec | **C3** |
| `andromeda-storage-heap` | Heap page operations | storage-page, codec | **C3** |

### Storage Layouts (C2-C3)

| Crate | Responsibility | Deps | Status |
|-------|-----------------|------|--------|
| `andromeda-transaction-log` | Transaction log storage | transaction, wal | **C3** |

### Forensic & Observability (C0-C2)

| Crate | Responsibility | Deps | Status |
|-------|-----------------|------|--------|
| `andromeda-forensic` | Forensic startup & diagnostics | recovery, storage | **C1** |
| `andromeda-observability` | Observability infrastructure | types | **C1** |

### CLI & Admin (C0-C1)

| Crate | Responsibility | Deps | Status |
|-------|-----------------|------|--------|
| `andromeda-cli` | Command-line interface | admin tools | **C0** |
| `andromeda-admin` | Admin operations | types, catalog | **C1** |
| `andromeda-runbooks` | Runbook documentation | types | **C0** |

### Test Support (C0-C1)

| Crate | Responsibility | Deps | Status |
|-------|-----------------|------|--------|
| `andromeda-bench` | Benchmark suite | bench-workload, storage | **C0** |
| `andromeda-bench-harness` | Benchmark harness | bench-workload | **C0** |
| `andromeda-bench-workload` | Workload generation | types, resource | **C1** |
| `andromeda-regression` | Regression detection | bench | **C0** |
| `andromeda-test-support` | Test fixtures & helpers | types | **C0** |
| `andromeda-business-fixtures` | Business entity fixtures | types | **C0** |
| `andromeda-srpl-test-fixtures` | SRPL test fixtures | srpl, types | **C0** |

### Code Generation (C0-C1)

| Crate | Responsibility | Deps | Status |
|-------|-----------------|------|--------|
| `andromeda-client-sdk-gen` | SDK code generation | contract, proto | **C0** |

## Dependency Graph Characteristics

### Hub Crates (Too Many Incoming Deps)

These crates are "too central" and should be scrutinized for splitting:

1. **andromeda-types** (2 imports estimated → 15+ actual)
2. **andromeda-error** (implied across all error paths)
3. **andromeda-codec** (used in storage, rpc, wal paths)
4. **andromeda-storage** (depends on nearly all C5 crates)
5. **andromeda-transaction** (tx, mvcc, locking all depend)

### Circular Dependencies

✅ **No circular dependencies detected** at crate level.  
⚠️ **Verify module-level cycles** in storage → wal → transaction → storage paths (expected, acceptable via type boundaries)

### Critical Paths (C5 Modules)

1. **Commit Path**: RPC → Execution → Transaction (tx) → Commit Log → WAL → Storage
2. **Recovery Path**: Recovery → WAL Replay → Catalog Replay → Storage
3. **Catalog Path**: RPC → Execution → Catalog → Catalog Store → Storage

### Feature-Gated Dependencies

- GPU work: andromeda-gpu (gated, off commit path) ✅
- TLS: features in quic-runtime-quinn
- Benchmarking: andromeda-bench (dev-only)

## Recommendations

### Phase 1-2: High Priority

1. **Extract andromeda-wal**: Decouple from storage → new crate boundary
2. **Extract transaction commit paths**: Separate tx-commit from tx-mvcc
3. **Extract andromeda-recovery**: Move to own crate (currently in storage)
4. **Verify pub use boundaries**: Reexports during extraction

### Phase 3+: Medium Priority

1. **Split andromeda-storage**: Currently ~5000+ LOC, mixing heaps, B-trees, WAL
2. **Split andromeda-srpl**: AST, parser, IR, lowering should be separate
3. **Move analytics**: andromeda-analytics → off critical path

### Blocked by Decisions

- Page size (affects andromeda-storage-page contracts)
- ContractHash canonicalization (affects andromeda-contract-compat)
- CatalogVersion granularity (affects andromeda-catalog versioning)

## Tools

To regenerate this matrix:

```bash
cargo metadata --format-version 1 > target/cargo-metadata.json
# Then analyze target/cargo-metadata.json for:
# - packages[].dependencies filtering
# - resolve.nodes[].deps counting
# - cycles detection
```

To verify no circular imports:

```bash
cargo check --workspace --all-targets
cargo deny check advisories
```
