# Workspace Topology

## Snapshot

`cargo metadata --no-deps --format-version 1` reports 88 workspace packages in
the current checkout on 2026-05-09. This document groups the current package
surface by architectural ownership. It is a topology guide, not release
readiness evidence.

## Package groups

| Group | Packages |
| --- | --- |
| Foundation | `andromeda-error`, `andromeda-digest`, `andromeda-types`, `andromeda-time`, `andromeda-hardware`, `andromeda-codec` when present, `andromeda-core` compatibility paths when present. |
| Principal, IAM, and security vocabulary | `andromeda-principal`, `andromeda-security-contract`, `andromeda-iam`, `andromeda-security`, `andromeda-admission`, `andromeda-audit`. |
| Contracts and protocol model | `andromeda-contract`, `andromeda-contract-compat`, `andromeda-procedure-contract`, `andromeda-structured-object`, `andromeda-proto`, `andromeda-proto-wire`, `andromeda-result-stream`, `andromeda-rpc-protocol`, `andromeda-rpc-codec`, `andromeda-rpc`. |
| SRPL model and compiler | `andromeda-srpl`, `andromeda-srpl-ast`, `andromeda-srpl-binder`, `andromeda-srpl-cardinality`, `andromeda-srpl-catalog-binding`, `andromeda-srpl-definition-batch`, `andromeda-srpl-diagnostics`, `andromeda-srpl-execution-adapter`, `andromeda-srpl-interpreter`, `andromeda-srpl-ir`, `andromeda-srpl-lexer`, `andromeda-srpl-lowering`, `andromeda-srpl-parser`, `andromeda-srpl-test-fixtures`. |
| Catalog and Procedure publication | `andromeda-catalog`, `andromeda-catalog-store`, `andromeda-catalog-recovery`, `andromeda-catalog-diff`, `andromeda-definition-batch`, `andromeda-procedure-store`, `andromeda-procedure-runtime`. |
| Durable storage and WAL | `andromeda-wal`, `andromeda-wal-codec`, `andromeda-storage`, `andromeda-storage-page`, `andromeda-storage-heap`, `andromeda-storage-index`, `andromeda-segment`, `andromeda-manifest`, `andromeda-buffer-pool`, `andromeda-disk-page-store`, `andromeda-backup`, `andromeda-restore`, `andromeda-recovery`, `andromeda-hadr`. |
| Transactions and concurrency | `andromeda-transaction`, `andromeda-transaction-log`, `andromeda-mvcc`, `andromeda-locking`, `andromeda-savepoint`. |
| Execution | `andromeda-exec`, `andromeda-execution`, `andromeda-execution-trace`, `andromeda-retry`. |
| Transport runtime | `andromeda-quic`, `andromeda-quic-runtime-quinn`. |
| Observability and evidence | `andromeda-observability`, `andromeda-observe`, `andromeda-decision-trace`, `andromeda-scenario-evidence`, `andromeda-regression`. |
| Advisory analytics | `andromeda-statistics`, `andromeda-optimizer`, `andromeda-plan-cache`, `andromeda-analytics`, `andromeda-maps`, `andromeda-columnar`, `andromeda-resource`. |
| Tools, demos, and tests | `andromeda-cli`, `andromeda-bench`, `andromeda-bench-harness`, `andromeda-bench-workload`, `andromeda-test-support`, `andromeda-business-fixtures`, `andromeda-inventory-demo`, `andromeda-inventory-demo-cli-adapter`. |

## Owner surfaces

| Owner surface | Rule |
| --- | --- |
| WAL owner | Controls LSNs, WAL records, frame codec, WAL bounds, durable prefix, and file WAL contracts. |
| Storage owner | Controls pages, heaps, B-Trees, buffer pool, disk store, manifest, segment index, backup, restore, HA/DR storage integration, and recovery integration. |
| Transaction owner | Controls transaction lifecycle, commit and rollback evidence, MVCC, locks, savepoints, and transaction log integration. |
| Catalog owner | Controls catalog store, DefinitionBatch, publication, catalog recovery, Procedure Store integration, statistics metadata, plan-cache identity, and catalog WAL records. |
| Contract owner | Controls Procedure contracts, contract hash behavior, compatibility diagnostics, qualified names, and catalog descriptors. |
| SRPL owners | Model crates own syntax and IR; bridge crates own catalog or execution integration. |
| RPC protocol owner | Controls runtime-free frame, stream, ResultStream, and protocol invariants. |
| QUIC owner | Controls concrete transport, route binding, session behavior, mTLS integration, and feature-gated Quinn runtime. |
| Security owners | Runtime-free contract crates own vocabulary; IAM and admission crates own policy evaluation paths. |
| Advisory owners | Statistics, optimizer, plan cache, Maps, hardware policy, benchmarks, and scenario evidence remain advisory unless a domain spec routes them through publication gates. |

## Broad crates

These surfaces need extra care because they span multiple domains:

| Crate | Risk | Planning rule |
| --- | --- | --- |
| `andromeda-storage` | C5 storage truth plus broad compatibility paths. | Split only by durable behavior boundary and owner tests. |
| `andromeda-catalog` | Catalog truth, Procedure publication, statistics metadata, and compatibility surfaces. | Keep catalog publication separate from advisory optimizer evidence. |
| `andromeda-exec` | Admission, SRPL, catalog, storage, transaction, result streams, audit, and QUIC integration. | Keep Procedure dispatch and transaction creation boundaries explicit. |
| `andromeda-srpl` | Compatibility facade plus compiler and bridge paths. | Keep model crates runtime-free and isolate catalog bridge code. |
| `andromeda-quic` | Concrete transport plus compatibility protocol exports. | Keep runtime-free protocol contracts in the protocol owner. |
| `andromeda-observe` | Observability and durable audit evidence. | Preserve evidence-vs-truth separation. |

## Current topology watch edges

Current metadata shows several edges that should stay explicit during review:

| Edge | Reason to watch |
| --- | --- |
| `andromeda-exec` -> `andromeda-quic` | Execution should not own transport runtime behavior. Prefer protocol abstractions and transport adapters. |
| `andromeda-observe` -> `andromeda-storage` | Durable audit fixtures or storage integration must not make audit evidence database truth. |
| `andromeda-admission` -> `andromeda-storage` and `andromeda-storage-page` | Admission should avoid storage truth ownership unless the dependency is narrowly justified. |
| `andromeda-srpl` -> catalog and DefinitionBatch crates | SRPL facade still bridges catalog publication; keep parser and model crates catalog-store-free. |
| `andromeda-quic-runtime-quinn` -> `andromeda-quic` | Quinn backend remains concrete runtime, not protocol contract owner. |

## Validation commands

Use these checks when topology changes:

```powershell
cargo metadata --no-deps --format-version 1
cargo check --workspace --all-targets
```

Run narrower owner tests for changed domains. C5 changes need owner tests plus
crash, replay, recovery, or durability evidence appropriate to the path.
