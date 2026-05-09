# Agent / Skill Matrix

## Agents

### `andromeda-architecture-orchestrator`
- Kind: `orchestrator`
- Description: Rust architecture governor for crates, folders, tests, tooling, TDD structure, and mission-critical project shape.
- Primary skills:
  - `architecture-workspace-governance`
  - `rust-crate-module-boundary`
  - `rust-file-splitting-placement`
  - `rust-test-strategy-tdd`
  - `source-grounding-from-project-docs`
  - `mission-critical-release-gates`

### `andromeda-codex-tooling-maintainer`
- Kind: `tooling-worker`
- Description: Maintains Andromeda Codex agents, skills, registries, validation scripts, prompts and .work output contracts.
- Primary skills:
  - `codex-skill-authoring`
  - `codex-agent-authoring`
  - `agent-skill-routing`
  - `codex-tooling-validation`
  - `work-directory-output-contract`
  - `markdown-report-quality-standard`

### `andromeda-implementation-orchestrator`
- Kind: `orchestrator`
- Description: Primary Codex orchestrator for targeted Rust implementation and improvement requests in Andromeda.
- Primary skills:
  - `codex-orchestration-protocol`
  - `task-scope-bounding`
  - `implementation-direct-change-protocol`
  - `todo-dependency-queue-planning`
  - `architecture-workspace-governance`
  - `rust-toolchain-1950-policy`
  - `rust-test-strategy-tdd`
  - `mission-critical-release-gates`

### `andromeda-implementation-write-worker`
- Kind: `write-worker`
- Description: Path-bounded implementation worker for direct Rust feature/improvement work. Produces one mission report under .work.
- Primary skills:
  - `write-worker-mission-report-contract`
  - `implementation-direct-change-protocol`
  - `rust-error-modeling`
  - `rust-test-strategy-tdd`
  - `rust-fuzz-property-miri-loom`
  - `binary-codec-format`
  - `mission-critical-release-gates`

### `andromeda-protocol-contract-worker`
- Kind: `specialist-worker`
- Description: Specialist for QUIC + Protobuf RPC contracts, ResultStream, payload bounds, and no-gRPC/no-JSON enforcement.
- Primary skills:
  - `quic-protobuf-rpc-contracts`
  - `protobuf-no-grpc-no-json`
  - `resultstream-protobuf-layout`
  - `security-iam-audit`
  - `binary-codec-format`
  - `mission-critical-release-gates`

### `andromeda-readonly-analysis-worker`
- Kind: `readonly-worker-report-only`
- Description: Repository-read-only analysis worker. It scans and reasons over a scoped area and writes exactly one Markdown report under .work.
- Primary skills:
  - `readonly-worker-report-contract`
  - `task-scope-bounding`
  - `source-grounding-from-project-docs`
  - `markdown-report-quality-standard`
  - `todo-dependency-queue-planning`

### `andromeda-refactor-orchestrator`
- Kind: `orchestrator`
- Description: Primary Codex orchestrator for Rust cleanup, code review, refactor planning, worker delegation, dependency queues, and final consolidation.
- Primary skills:
  - `codex-orchestration-protocol`
  - `task-scope-bounding`
  - `readonly-worker-report-contract`
  - `todo-dependency-queue-planning`
  - `rust-clean-code-refactor`
  - `rust-dead-code-detection`
  - `rust-orphan-detection`
  - `rust-duplicate-code-consolidation`
  - `rust-file-splitting-placement`
  - `rust-test-strategy-tdd`
  - `mission-critical-release-gates`

### `andromeda-refactor-write-worker`
- Kind: `write-worker`
- Description: Path-bounded write worker for cleanup/refactor/code review corrections. Produces exactly one mission report under .work.
- Primary skills:
  - `write-worker-mission-report-contract`
  - `rust-clean-code-refactor`
  - `rust-dead-code-detection`
  - `rust-orphan-detection`
  - `rust-duplicate-code-consolidation`
  - `rust-file-splitting-placement`
  - `rust-naming-noise-cleanup`
  - `rust-test-strategy-tdd`

### `andromeda-release-mission-critical-reviewer`
- Kind: `readonly-worker-report-only`
- Description: Repository-read-only report-only reviewer for mission-critical readiness, release gates, C4/C5 risks, and residual work.
- Primary skills:
  - `mission-critical-release-gates`
  - `source-grounding-from-project-docs`
  - `rust-ci-quality-gates`
  - `transaction-wal-recovery`
  - `security-iam-audit`
  - `markdown-report-quality-standard`

### `andromeda-roadmap-orchestrator`
- Kind: `orchestrator`
- Description: Roadmap-aware Codex orchestrator that consumes global and stage-specific Markdown roadmaps and converts them into queued worker execution.
- Primary skills:
  - `roadmap-intake-and-stage-selection`
  - `roadmap-task-decomposition`
  - `todo-dependency-queue-planning`
  - `write-worker-mission-report-contract`
  - `markdown-report-quality-standard`
  - `mission-critical-release-gates`

### `andromeda-srpl-catalog-worker`
- Kind: `specialist-worker`
- Description: Specialist for SRPL, Type System, ProcedureContract, CatalogVersion, DefinitionBatch, Modelization, Maps and compatibility.
- Primary skills:
  - `srpl-procedure-contracts`
  - `srpl-parser-binder-ir`
  - `catalog-definitionbatch`
  - `maps-analytics-summarizability`
  - `andromeda-doctrine-invariants`
  - `mission-critical-release-gates`

### `andromeda-storage-wal-worker`
- Kind: `specialist-worker`
- Description: Specialist for WAL, transaction state, MVCC, storage segments, manifests, binary codecs, recovery and crash-test behavior.
- Primary skills:
  - `transaction-wal-recovery`
  - `storage-manifest-segment`
  - `binary-codec-format`
  - `mvcc-isolation-anomalies`
  - `rust-unsafe-audit`
  - `rust-fuzz-property-miri-loom`
  - `mission-critical-release-gates`

### `andromeda-test-verification-worker`
- Kind: `specialist-worker`
- Description: Specialized worker for test topology, property tests, fuzzing, Miri, Loom, crash/recovery, CI and validation gates.
- Primary skills:
  - `rust-test-strategy-tdd`
  - `rust-fuzz-property-miri-loom`
  - `mission-critical-release-gates`
  - `transaction-wal-recovery`
  - `storage-manifest-segment`
  - `markdown-report-quality-standard`
