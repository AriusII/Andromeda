# Andromeda Codex Skills Index

Generated: 2026-05-09T19:57:52.304571+00:00

Total skills: 56

## andromeda-analytics

- `maps-analytics-summarizability` — Use for Maps, analytical projections, grain, refresh policy and summarizability.

## andromeda-architecture

- `architecture-workspace-governance` — Use for Rust workspace, crates, folders, tests, docs and tooling architecture.

## andromeda-catalog

- `catalog-definitionbatch` — Use for Catalog, Modelization, DefinitionBatch, DryRun and CatalogVersion work.

## andromeda-core

- `andromeda-doctrine-invariants` — Use to enforce Andromeda strict-boundary doctrine and C5 invariants.

## andromeda-hardware

- `cpu-simd-dispatch` — Use for CPU SIMD, target features, runtime dispatch and scalar fallback.
- `gpu-batch-outside-commit` — Use for GPU analytics/statistics/vector batch work.

## andromeda-operations

- `hadr-backup-forensic` — Use for HA/DR, quorum, fencing, backup, restore, PITR and forensic startup.

## andromeda-optimizer

- `optimizer-statistics-plan-cache` — Use for optimizer, statistics, plan cache, PlanClass and DecisionTrace.
- `procedure-store-evidence` — Use for Procedure Store, ScenarioEvidence and feedback loops.

## andromeda-protocol

- `protobuf-no-grpc-no-json` — Use to enforce Protobuf without gRPC and no JSON native protocol.
- `quic-protobuf-rpc-contracts` — Use for QUIC + Protobuf custom RPC design and implementation.
- `resultstream-protobuf-layout` — Use for ResultStream metadata/payload order and Protobuf layout.

## andromeda-security

- `security-iam-audit` — Use for mTLS, CertificateIdentity, UserPrincipal, permissions, policies, surfaces and audit.

## andromeda-srpl

- `srpl-parser-binder-ir` — Use for SRPL parser, binder, diagnostics, semantic IR/ALT and plan preparation.
- `srpl-procedure-contracts` — Use for SRPL procedure semantics, ProcedureContract, ContractHash and compatibility.

## andromeda-storage

- `binary-codec-format` — Use for explicit binary codecs, persisted files, WAL, pages, manifests, frames and contracts.
- `storage-manifest-segment` — Use for storage segments, manifests, snapshots, root pointers, HotStore/ColdStore and SegmentIndex.

## andromeda-transaction

- `mvcc-isolation-anomalies` — Use for MVCC visibility, isolation policies, anomalies and long readers.
- `transaction-wal-recovery` — Use for transaction kernel, WAL, recovery, durability and crash behavior.

## codex-orchestration

- `codex-orchestration-protocol` — Use when a request requires Codex to coordinate analysis workers, write workers, dependency queues, and final consolidation.
- `task-scope-bounding` — Use to transform broad user requests into precise bounded Codex work orders.
- `todo-dependency-queue-planning` — Use to represent TODO/SUB-TODO/dependencies and decide worker execution order.

## codex-tooling

- `agent-skill-routing` — Use to choose between an agent, a skill, or direct work.
- `codex-agent-authoring` — Use to author or revise Codex agent TOML files for Andromeda.
- `codex-skill-authoring` — Use to author precise Codex skills with strong triggers and guardrails.
- `codex-tooling-validation` — Use to validate .codex agents, skills, registries and routing files.
- `source-grounding-from-project-docs` — Use to ground decisions in Andromeda project Markdown/PDF context.

## codex-workers

- `markdown-report-quality-standard` — Use for `.work` reports, plans, mission reports and final consolidations.
- `readonly-worker-report-contract` — Use by repository-read-only workers that must inspect code and produce one detailed Markdown report under .work.
- `work-directory-output-contract` — Use when workers or orchestrators need to write intermediate Codex task artifacts.
- `write-worker-mission-report-contract` — Use by write workers that must execute a bounded change and produce one mission report.

## implementation

- `implementation-direct-change-protocol` — Use for direct implementation requests with bounded code changes.

## release

- `mission-critical-release-gates` — Use for C4/C5 mission-critical readiness.

## roadmap

- `roadmap-intake-and-stage-selection` — Use to read a global roadmap and select the current detailed stage.
- `roadmap-task-decomposition` — Use to decompose roadmap stages into executable work packets.

## rust-architecture

- `rust-crate-module-boundary` — Use to design or audit Cargo crate and module boundaries.
- `rust-toolchain-1950-policy` — Use to enforce Rust 1.95.0 / Edition 2024 / resolver 3 baseline.

## rust-implementation

- `rust-error-modeling` — Use to design typed Rust errors and protocol error conversion.

## rust-performance

- `rust-allocation-clone-review` — Use to review allocations, clones, buffer growth and ownership pressure.
- `rust-performance-optimization` — Use for measured Rust performance improvement.

## rust-refactor

- `repository-cleanup-campaign` — Use for broad cleanup campaigns across workspace.
- `rust-clean-code-refactor` — Use for professional Rust cleanup/refactor work without behavior drift.
- `rust-dead-code-detection` — Use to detect and remove unused Rust code paths.
- `rust-deprecated-obsolete-removal` — Use to remove obsolete, deprecated or transitional code.
- `rust-duplicate-code-consolidation` — Use to consolidate duplicated Rust logic safely.
- `rust-file-splitting-placement` — Use to shrink or split large Rust files by responsibility.
- `rust-naming-noise-cleanup` — Use to clean Rust naming across crates, modules, files, functions and tests.
- `rust-orphan-detection` — Use to find orphan files, modules, crates, features, examples, benches, scripts, or docs.
- `rust-public-api-minimization` — Use to reduce unnecessary `pub` surface and stabilize APIs.

## rust-review

- `code-review-professional-grade` — Use for rigorous code review of Rust/Andromeda changes.

## rust-runtime

- `rust-async-cancellation-backpressure` — Use for async runtime, cancellation, shutdown and backpressure design.

## rust-safety

- `rust-unsafe-audit` — Use to audit unsafe Rust, FFI, SIMD, mmap, direct I/O, and raw pointer code.

## rust-security

- `dependency-supply-chain` — Use for Cargo dependency pruning and supply-chain governance.

## rust-testing

- `rust-ci-quality-gates` — Use to define CI commands and release gates for Rust work.
- `rust-fuzz-property-miri-loom` — Use for advanced Rust verification of parsers, codecs, unsafe and concurrency.
- `rust-test-strategy-tdd` — Use to design tests and TDD approach for Rust changes.
