---
name: roadmap-intake-and-stage-selection
description: Selects current roadmap stages and intake queues when prompts mention roadmap, phase selection, critical path, dependency queues, or stage planning.
license: MIT
---

# roadmap-intake-and-stage-selection

## When to use
- The prompt mentions roadmap, stage, phase, intake, critical path, dependency queue, next work, current stage, or what should be done next.
- A user asks to pick tasks from docs/roadmap/phases/P* or align work with project sequencing.
- A plan risks jumping ahead of prerequisites.

## Purpose
Turn broad Andromeda roadmap requests into a grounded current-stage decision. The skill makes agents read the master roadmap, execution method, queues, and phase files before choosing what work belongs now versus later.

## Process
1. Read ROADMAP_MASTER, EXECUTION_METHOD, CRITICAL_PATH, and DEPENDENCY_QUEUES first.
2. Scan the relevant P* phase files and identify prerequisites, current deliverables, blockers, and validation gates.
3. Prefer the earliest unblocked critical-path stage unless the user explicitly scopes a later phase.
4. Record assumptions about current repository status separately from roadmap intent.
5. Return a selected stage with rationale, dependency queue position, and handoff to task decomposition if work should begin.

## Expected output
- A selected roadmap stage or explicit no-stage decision.
- Prerequisites, blockers, and dependency queue rationale.
- Next recommended work packets or validation gates.

## Reference docs
- `docs/roadmap/ROADMAP_MASTER.md`
- `docs/roadmap/EXECUTION_METHOD.md`
- `docs/roadmap/queues/CRITICAL_PATH.md`
- `docs/roadmap/queues/DEPENDENCY_QUEUES.md`
- `docs/roadmap/phases/P00_REPOSITORY_STATE_AND_GOVERNANCE.md`
- `docs/roadmap/phases/P01_NORMATIVE_SPECIFICATION_BASELINE.md`
- `docs/roadmap/phases/P02_DURABLE_VERTICAL_PATH_PRODUCT_STOCK.md`
- `docs/roadmap/phases/P03_CATALOG_DEFINITION_BATCH_DURABILITY.md`
- `docs/roadmap/phases/P04_GENERIC_PROCEDURE_EXECUTION.md`
- `docs/roadmap/phases/P05_STORAGE_WAL_PAGE_MANIFEST_COMPLETION.md`
- `docs/roadmap/phases/P06_TRANSACTION_MVCC_ISOLATION_STRENGTHENING.md`
- `docs/roadmap/phases/P07_SECURITY_IAM_AUDIT_DURABILITY.md`
- `docs/roadmap/phases/P08_QUIC_RPC_APPLICATION_RUNTIME.md`
- `docs/roadmap/phases/P09_STATISTICS_OPTIMIZER_PROCEDURE_STORE_V0.md`
- `docs/roadmap/phases/P10_MAPS_ANALYTICS_CPU_FIRST.md`
- `docs/roadmap/phases/P11_CPU_NVME_PERFORMANCE_AND_RESOURCE_GOVERNANCE.md`
- `docs/roadmap/phases/P12_OPTIONAL_GPU_BATCH_ACCELERATION.md`
- `docs/roadmap/phases/P13_BACKUP_RESTORE_PITR_FORENSIC.md`
- `docs/roadmap/phases/P14_HADR_SINGLE_PRIMARY_CLUSTER.md`
- `docs/roadmap/phases/P15_ENTERPRISE_RELEASE_HARDENING.md`

## Guardrails
- Do not cherry-pick later work that bypasses critical-path prerequisites.
- Do not treat roadmap intent as implemented status.
- Do not ignore dependency queues when proposing parallel work.
- When doctrine is implicated, cite `docs/project/ANDROMEDA_DOCTRINE.md` and treat conflicts as blockers.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
