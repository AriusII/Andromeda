# Andromeda skills index

## andromeda-core

- `andromeda-doctrine-invariants` — Enforce Andromeda doctrine invariants and criticality boundaries.
- `source-grounding-from-project-docs` — Ground implementation decisions in canonical project documentation.

## andromeda-architecture

- `architecture-workspace-governance` — Maintain workspace ownership, crate boundaries, and architectural topology.

## andromeda-srpl

- `srpl-procedure-contracts` — Preserve typed Procedure contract semantics and versioned contract invariants.
- `srpl-parser-binder-ir` — Guide SRPL parsing, binding, IR, and deterministic validation flows.

## andromeda-catalog

- `catalog-definitionbatch` — Govern catalog DefinitionBatch behavior and publication evidence.

## andromeda-transaction

- `transaction-wal-recovery` — Protect transaction, WAL durability, recovery, and visible-commit rules.
- `mvcc-isolation-anomalies` — Analyze MVCC isolation guarantees and anomaly prevention.

## andromeda-storage

- `storage-manifest-segment` — Guide storage manifest, segment, page, and buffer-pool ownership.
- `binary-codec-format` — Require explicit binary codecs for disk and wire formats.

## andromeda-protocol

- `quic-protobuf-rpc-contracts` — Maintain QUIC plus custom Protobuf RPC contract boundaries.
- `protobuf-no-grpc-no-json` — Prevent gRPC and JSON application-surface regressions.
- `resultstream-protobuf-layout` — Preserve ResultStream metadata-before-payload layout.

## andromeda-security

- `security-iam-audit` — Review IAM, admission, audit, and policy enforcement changes.

## andromeda-optimizer

- `optimizer-statistics-plan-cache` — Guide optimizer statistics and plan-cache correctness.

## andromeda-operations

- `procedure-store-evidence` — Track Procedure store readiness and evidence requirements.
- `hadr-backup-forensic` — Guide HADR, backup, restore, and forensic operational evidence.
- `observability-trace-schema` — Maintain observability trace schema and operational telemetry contracts.
- `deployment-profile-policy` — Enforce deployment profile policy and runtime readiness gates.

## andromeda-hardware

- `nvme-resource-pressure` — Evaluate NVMe resource pressure and storage performance risks.
- `cpu-simd-dispatch` — Guide portable CPU/SIMD dispatch decisions.
- `gpu-batch-outside-commit` — Keep GPU batch paths outside commit, rollback, WAL, recovery, and auth paths.

## andromeda-analytics

- `maps-analytics-summarizability` — Preserve MAPS analytics summarizability and correctness constraints.

## release

- `mission-critical-release-gates` — Apply mission-critical release gate evidence before readiness claims.

## roadmap

- `roadmap-intake-and-stage-selection` — Triage roadmap intake and select implementation stages.
- `roadmap-task-decomposition` — Decompose roadmap items into bounded, dependency-aware tasks.

## rust-architecture

- `rust-toolchain-1950-policy` — Enforce the Andromeda Rust 1.95.0 Edition 2024 resolver 3 MSRV policy when toolchain baseline or compatibility keywords appear.
- `rust-crate-module-boundary` — Design and audit Rust crate and module boundaries when crate ownership common utils misc helpers god_engine or layering keywords appear.
- `rust-file-splitting-placement` — Shrink and split oversized Rust files by owner and responsibility when large file module split placement or ownership keywords appear.
- `rust-public-api-minimization` — Minimize Rust public APIs when pub surface exports stability or lib.rs re-export keywords appear.

## rust-implementation

- `rust-error-modeling` — Design typed Rust errors and protocol conversions when ErrKind thiserror anyhow error mapping or RPC error keywords appear.
- `rust-dead-code-detection` — Detect and remove unused Rust code paths when dead code unused udeps unreachable or stale feature keywords appear.
- `rust-orphan-detection` — Find orphan files modules crates features examples benches scripts or docs when orphan stale unowned or disconnected keywords appear.

## rust-refactor

- `rust-clean-code-refactor` — Perform professional Rust cleanup without behavior drift when refactor cleanup simplify or readability keywords appear.
- `rust-duplicate-code-consolidation` — Consolidate duplicated Rust logic safely when duplicate copy paste shared helper or abstraction keywords appear.
- `rust-deprecated-obsolete-removal` — Remove obsolete deprecated transitional Rust code when deprecated obsolete legacy temporary shim or migration keywords appear.
- `rust-naming-noise-cleanup` — Clean Rust naming noise when rename terminology casing module file function or test name keywords appear.
- `repository-cleanup-campaign` — Plan and execute broad workspace cleanup campaigns when repository cleanup hygiene campaign or many warnings keywords appear.

## rust-safety

- `rust-unsafe-audit` — Audit unsafe Rust FFI SIMD mmap and raw pointer code when unsafe safety invariants or memory keywords appear.
- `rust-allocation-clone-review` — Review allocations clones buffer growth and ownership pressure when clone allocation Vec String Arc or performance keywords appear.

## rust-performance

- `rust-performance-optimization` — Make measured Rust performance improvements when benchmark latency throughput criterion divan or optimization keywords appear.

## rust-runtime

- `rust-async-cancellation-backpressure` — Design Rust async cancellation shutdown and backpressure when tokio async channel timeout or slow client keywords appear.

## rust-security

- `dependency-supply-chain` — Govern Cargo dependencies and supply chain risk when dependency cargo audit pruning license or vendor keywords appear.

## rust-testing

- `rust-test-strategy-tdd` — Design Rust tests and TDD plans when test strategy TDD risk class property recovery or fixture keywords appear.
- `rust-fuzz-property-miri-loom` — Apply fuzz property Miri and loom verification when parser codec unsafe concurrency or invariants keywords appear.
- `rust-ci-quality-gates` — Run Andromeda Rust CI and release quality gates when CI clippy fmt test nextest release gate or validation keywords appear.

## rust-review

- `code-review-professional-grade` — Perform high signal Andromeda Rust code review when review PR diff risk bug security or correctness keywords appear.

## copilot-orchestration

- `copilot-orchestration-protocol` — Coordinate Copilot CLI analysis planning writing validation and consolidation when broad multi-agent or orchestration keywords appear.
- `task-scope-bounding` — Convert broad requests into precise bounded Copilot work orders when scope vague broad cleanup or roadmap keywords appear.
- `todo-dependency-queue-planning` — Represent TODO sub TODO and dependency queues when planning worker order or multi-step execution keywords appear.
- `agent-skill-routing` — Choose direct work skill loading or agent dispatch when routing Copilot CLI tasks by scope risk and available tools keywords appear.
- `markdown-report-quality-standard` — Set Markdown report quality for Copilot CLI work artifacts when report analysis final summary or .work keywords appear.

## copilot-workers

- `work-directory-output-contract` — Route Copilot CLI intermediate artifacts under .work/copilot-cli task directories when workdir artifact mission report keywords appear.
- `readonly-worker-report-contract` — Define read only Copilot worker reports when analysis worker scout audit inventory or no edits keywords appear.
- `write-worker-mission-report-contract` — Define Copilot write worker mission reports when bounded edit worker implementation or mission keywords appear.
- `implementation-direct-change-protocol` — Guide bounded direct Copilot implementation changes when implement fix change patch or small edit keywords appear.

## copilot-tooling

- `copilot-skill-authoring` — Author Copilot CLI skills when skill authoring SKILL.md frontmatter allowed-tools or trigger keywords appear.
- `copilot-agent-authoring` — Author Copilot CLI agent files when .agent.md agent frontmatter tool aliases worker or sub-agent keywords appear.