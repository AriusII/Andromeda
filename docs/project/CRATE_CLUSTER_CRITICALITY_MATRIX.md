# Crate cluster and criticality matrix

> **Status:** P00 governance baseline
> **Audience:** Andromeda maintainers, reviewers, roadmap workers
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, resolver 3

## Purpose

This matrix maps the 89 workspace crates declared in the root `Cargo.toml` to
engine clusters and P00 criticality. It is a governance aid, not a production
readiness claim.

The root `Cargo.toml` `workspace.members` list is the count source of truth.
`crates/andromeda-cli/tests/workspace_dependency_topology.rs` protects this by
checking both the declared 89 members and the physical crate manifests under
`crates/`. If the count changes, update this matrix and the topology gate in the
same change.

Criticality describes the blast radius of an invariant. It does not prove that a
crate is complete, durable, secure, or release-ready.

Criticality follows `docs/project/CRITICALITY_MODEL.md`:

| Level | Meaning |
|---|---|
| C5 | Non-negotiable invariant or durable/security truth boundary. |
| C4 | Mission-critical runtime or operations behavior. |
| C3 | Critical policy, diagnostics, optimizer, or compatibility behavior. |
| C2 | Important measured subsystem with fallback or bounded impact. |
| C1 | Opportunistic, advisory, fixture, benchmark, or support surface. |
| C0 | Isolated research only. |

## Matrix

| Crate | Cluster | Criticality | P00 status |
|---|---|---:|---|
| `andromeda-admission` | Procedure admission and resource gate | C5 | Pre-transaction admission boundary. |
| `andromeda-audit` | Audit vocabulary, durable audit evidence, and forensic correlation | C4 | Durable audit ledger/evidence boundary; not engine storage truth. |
| `andromeda-analytics` | Analytics and advisory execution | C2 | Non-C5 analytics boundary. |
| `andromeda-backup` | Backup and durable operations | C5 | Scaffold boundary; no recoverable claim without restore evidence. |
| `andromeda-bench` | Benchmark and advisory evidence | C1 | Advisory only. |
| `andromeda-bench-harness` | Benchmark harness | C1 | Advisory only. |
| `andromeda-bench-workload` | Benchmark workload fixtures | C1 | Advisory only. |
| `andromeda-buffer-pool` | Storage buffer management | C5 | Durable-kernel boundary; RAM is not system truth. |
| `andromeda-business-fixtures` | Test and demo business fixtures | C1 | Fixture/support only. |
| `andromeda-cli` | Operator and developer CLI | C2 | Administration surface; must not bypass Procedure contracts. |
| `andromeda-catalog` | Catalog, contracts, and publication | C5 | CatalogVersion and contract truth boundary. |
| `andromeda-catalog-diff` | Catalog diff support | C3 | Compatibility and review support. |
| `andromeda-catalog-recovery` | Catalog recovery | C5 | Catalog reconstruction boundary. |
| `andromeda-catalog-store` | Catalog durable store | C5 | Catalog persistence boundary. |
| `andromeda-columnar` | Columnar analytics layout | C2 | Analytics layout; not source truth. |
| `andromeda-contract` | Contract-safe object descriptors | C5 | Procedure/catalog contract foundation. |
| `andromeda-contract-compat` | Contract compatibility | C4 | Compatibility policy boundary. |
| `andromeda-core` | Foundation compatibility re-export | C5 | Temporary facade; must not regain broad ownership. |
| `andromeda-decision-trace` | Decision trace | C3 | Explains decisions; not authoritative truth alone. |
| `andromeda-definition-batch` | Catalog mutation batch | C5 | Controlled catalog mutation boundary. |
| `andromeda-digest` | Deterministic digest primitives | C5 | Hash and identity foundation. |
| `andromeda-disk-page-store` | Disk page persistence | C5 | Durable page store boundary. |
| `andromeda-error` | Typed engine errors | C5 | Error foundation. |
| `andromeda-exec` | Procedure execution orchestration | C5 | Procedure-only execution boundary. |
| `andromeda-execution` | Execution adapters and runtime glue | C4 | Runtime integration boundary. |
| `andromeda-execution-trace` | Execution trace evidence | C3 | Trace support. |
| `andromeda-hadr` | HA/DR cluster operations | C5 | Scaffold boundary; no HA claim without quorum/fencing evidence. |
| `andromeda-hardware` | Hardware profiles and acceleration policy | C2 | Resource and acceleration policy; no durable authority. |
| `andromeda-iam` | IAM admission | C5 | Fail-closed security admission boundary. |
| `andromeda-inventory-demo` | V0 vertical demo | C2 | Prototype evidence, not production runtime. |
| `andromeda-inventory-demo-cli-adapter` | Demo CLI adapter | C1 | Demo adapter only. |
| `andromeda-locking` | Lock coordination | C5 | Durable-kernel concurrency boundary. |
| `andromeda-manifest` | Database manifest | C5 | Manifest truth boundary. |
| `andromeda-maps` | Materialized maps | C2 | Derived analytics; never source truth. |
| `andromeda-mvcc` | MVCC visibility | C5 | Visibility truth boundary. |
| `andromeda-observe` | Observability and audit evidence | C4 | Evidence and audit ledger boundary; not storage truth. |
| `andromeda-observability` | Shared observability identifiers | C3 | Correlation support. |
| `andromeda-optimizer` | Optimizer and bounded planning | C3 | Plan selection support with DecisionTrace. |
| `andromeda-plan-cache` | Plan cache identity | C3 | Policy-bound performance component. |
| `andromeda-principal` | Principal identity | C5 | Security identity boundary. |
| `andromeda-procedure-contract` | Procedure contract | C5 | Typed, hashed, versioned contract boundary. |
| `andromeda-procedure-runtime` | Procedure runtime | C5 | Procedure invocation runtime boundary. |
| `andromeda-procedure-store` | Procedure Store evidence | C3 | Evidence and feedback, not commit truth. |
| `andromeda-proto` | Custom Protobuf payload contracts | C4 | Custom typed RPC schema; no gRPC surface. |
| `andromeda-proto-wire` | Protobuf wire helpers | C4 | Wire support under typed protocol rules. |
| `andromeda-quic` | QUIC transport boundary | C4 | Transport boundary; does not own Procedure semantics. |
| `andromeda-quic-runtime-quinn` | Quinn runtime adapter | C4 | Optional concrete runtime boundary. |
| `andromeda-recovery` | Recovery and forensic reconstruction | C5 | Recovery truth boundary. |
| `andromeda-regression` | Regression evidence | C2 | Regression support. |
| `andromeda-resource` | Resource budgets | C3 | Policy-bound runtime resource control. |
| `andromeda-restore` | Restore and PITR operations | C5 | Scaffold boundary; no recoverable claim without drill evidence. |
| `andromeda-result-stream` | Typed result stream | C4 | Result metadata, payload, and completion contract. |
| `andromeda-retry` | Retry policy | C3 | Policy-bound execution support. |
| `andromeda-rpc` | Runtime-free RPC dispatch glue | C4 | Protocol glue, not transport runtime. |
| `andromeda-rpc-codec` | RPC codec helpers | C4 | Bounded payload validation boundary. |
| `andromeda-rpc-protocol` | RPC frame protocol | C4 | Runtime-free frame and stream contracts. |
| `andromeda-savepoint` | Transaction savepoints | C5 | Durable-kernel transaction boundary. |
| `andromeda-scenario-evidence` | Scenario evidence | C1 | Advisory evidence only. |
| `andromeda-security` | Security domain scaffold | C5 | Scaffold boundary; must not duplicate IAM/security-contract ownership. |
| `andromeda-security-contract` | Runtime-free security contracts | C5 | Permission, surface, and policy vocabulary boundary. |
| `andromeda-segment` | Storage segment layout | C5 | Durable segment boundary. |
| `andromeda-srpl` | SRPL compatibility facade | C4 | Language facade with extraction exit criteria. |
| `andromeda-srpl-ast` | SRPL syntax shapes | C4 | Language model boundary. |
| `andromeda-srpl-binder` | SRPL binding | C4 | Name/type/cardinality binding boundary. |
| `andromeda-srpl-cardinality` | SRPL cardinality | C4 | Cardinality semantics boundary. |
| `andromeda-srpl-catalog-binding` | SRPL to catalog binding | C4 | Catalog/language binding boundary. |
| `andromeda-srpl-definition-batch` | SRPL DefinitionBatch bridge | C4 | Language-to-catalog mutation bridge. |
| `andromeda-srpl-diagnostics` | SRPL diagnostics | C4 | Stable diagnostics boundary. |
| `andromeda-srpl-execution-adapter` | SRPL execution adapter | C4 | Execution adapter boundary. |
| `andromeda-srpl-interpreter` | SRPL interpreter | C4 | Interpreter boundary; no SQL bypass. |
| `andromeda-srpl-ir` | Semantic IR | C4 | Bounded semantic IR boundary. |
| `andromeda-srpl-lexer` | SRPL lexer | C4 | Lexer boundary. |
| `andromeda-srpl-lowering` | SRPL lowering | C4 | Lowering boundary. |
| `andromeda-srpl-parser` | SRPL parser | C4 | Parser boundary. |
| `andromeda-srpl-test-fixtures` | SRPL test fixtures | C1 | Fixture/support only. |
| `andromeda-statistics` | Statistics publication | C3 | StatsVersion candidate/published boundary. |
| `andromeda-storage` | Storage integration | C5 | Durable storage truth boundary. |
| `andromeda-storage-heap` | Heap storage | C5 | Durable heap boundary. |
| `andromeda-storage-index` | Storage indexes | C5 | Durable index boundary. |
| `andromeda-storage-page` | Page format | C5 | Durable page codec boundary. |
| `andromeda-storage-placement` | Storage placement | C5 | Durable placement policy boundary. |
| `andromeda-structured-object` | Structured payload descriptors | C5 | Typed payload admission and descriptor hashing boundary. |
| `andromeda-test-support` | Shared test support | C1 | Test support only. |
| `andromeda-time` | Engine time and timestamps | C5 | Timestamp foundation. |
| `andromeda-transaction` | Transaction lifecycle | C5 | Transaction state truth boundary. |
| `andromeda-transaction-log` | Transaction log integration | C5 | Transaction WAL evidence boundary. |
| `andromeda-types` | Shared identifiers and type descriptors | C5 | Type and ID foundation. |
| `andromeda-wal` | WAL semantics and FileWal | C5 | Durable WAL truth boundary. |
| `andromeda-wal-codec` | WAL binary codec | C5 | Explicit WAL byte contract boundary. |

## Review rule

If a crate changes cluster or criticality, update this file, the owning crate
README if needed, and the validation gate that protects the boundary. Lower
criticality crates must not control higher criticality invariants.
