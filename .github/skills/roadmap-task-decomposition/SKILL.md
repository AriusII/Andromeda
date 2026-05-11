---
name: roadmap-task-decomposition
description: Decomposes roadmap stages into executable work packets when prompts mention task breakdown, work packets, TODOs, dependency ordering, or team scope.
license: MIT
---

# roadmap-task-decomposition

## When to use
- The prompt mentions decompose, task breakdown, work packets, TODOs, sub-tasks, dependency ordering, worker scope, or roadmap execution.
- A stage has been selected and needs implementation-ready packets.
- A user asks how to split work across people or agents without conflicting edits.

## Purpose
Convert a selected phase into small, dependency-aware, validation-backed work packets that respect Andromeda crate ownership and mission-critical gates. The skill prevents vague roadmap prose from becoming unbounded implementation work.

## Process
1. Read the relevant phase file, roadmap sources, team scope docs, and ROADMAP_STEP_TEMPLATE.
2. Break deliverables into packets with owner crate/module, docs/spec source, prerequisites, acceptance tests, and forbidden shortcuts.
3. Separate read-only analysis packets from write packets; do not parallelize packets that edit the same files or invariants.
4. Attach validation commands and release/CI gates appropriate to criticality.
5. Return dependency queues that can be executed incrementally and stopped safely after any packet.

## Expected output
- A packet list with ID, scope, owner, dependencies, deliverables, validation, and guardrails.
- Parallelization guidance and conflict warnings.
- A done definition tied to tests, docs, and acceptance gates.

## Reference docs
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
- `docs/roadmap/sources/CROSS_CHECK_SOURCES.md`
- `docs/roadmap/sources/P02_FILEWAL_RECOVERY_EVIDENCE.md`
- `docs/roadmap/sources/P02_PRE_TRANSACTION_REJECTION_MATRIX.md`
- `docs/roadmap/sources/P02_PRODUCT_STOCK_DURABLE_PATH_ACCEPTANCE.md`
- `docs/roadmap/sources/P02_RESULT_STREAM_ORDERING_CONTRACT.md`
- `docs/roadmap/sources/P03_CATALOG_RECOVERY_REPORT.md`
- `docs/roadmap/sources/P03_CATALOG_STORE_DURABLE_EVIDENCE.md`
- `docs/roadmap/sources/P03_CONTRACT_HASH_CANONICALIZATION.md`
- `docs/roadmap/sources/P03_DEFINITION_BATCH_APPLY_EVIDENCE.md`
- `docs/roadmap/sources/VALIDATION_REPORT.md`
- `docs/roadmap/team/PERSON_01_SCOPE.md`
- `docs/roadmap/team/PERSON_02_SCOPE.md`
- `docs/roadmap/team/PERSON_03_SCOPE.md`
- `docs/roadmap/team/PERSON_04_SCOPE.md`
- `docs/roadmap/team/PERSON_05_SCOPE.md`
- `docs/roadmap/team/PERSON_06_SCOPE.md`
- `docs/roadmap/team/PERSON_07_SCOPE.md`
- `docs/roadmap/team/PERSON_08_SCOPE.md`
- `docs/roadmap/team/PERSON_09_SCOPE.md`
- `docs/roadmap/team/PERSON_10_SCOPE.md`
- `docs/roadmap/team/PERSON_11_SCOPE.md`
- `docs/roadmap/team/PERSON_12_SCOPE.md`
- `docs/roadmap/team/PERSON_13_SCOPE.md`
- `docs/roadmap/team/PERSON_14_SCOPE.md`
- `docs/roadmap/team/PERSON_15_SCOPE.md`
- `docs/roadmap/team/PERSON_16_SCOPE.md`
- `docs/roadmap/team/PERSON_17_SCOPE.md`
- `docs/roadmap/team/PERSON_18_SCOPE.md`
- `docs/roadmap/team/PERSON_19_SCOPE.md`
- `docs/roadmap/team/PERSON_20_SCOPE.md`
- `docs/templates/ROADMAP_STEP_TEMPLATE.md`

## Guardrails
- Do not create packets without validation gates.
- Do not split tightly coupled edits across parallel workers.
- Do not bypass crate ownership or doctrine constraints.
- When doctrine is implicated, cite `docs/project/ANDROMEDA_DOCTRINE.md` and treat conflicts as blockers.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
