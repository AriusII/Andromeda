# Andromeda Documentation

## Purpose

Provide the canonical documentation surface for Andromeda. Use this directory
to find protected doctrine, architecture references, v0 specifications,
implementation-state ledgers, operations runbooks, governance decisions,
developer guides, and validation evidence templates.

## Scope

This README covers the structure of `documentations/` and points to the
navigation indexes that should be used before changing or citing project
documents.

## Non-goals

This README does not approve release readiness, replace architecture decision
records, create new runtime behavior, or treat draft, planning, runbook, audit,
benchmark, GPU, RAM, or temporary evidence as database truth.

## Prerequisites

Before editing or citing documentation:

1. Read the repository `AGENTS.md`.
2. Check for a more specific index in the target area.
3. Preserve the status language used by the source document.
4. Link to source evidence instead of inferring implemented behavior from a
   file name.

## Procedure

1. Start with the index that matches the task.
2. Follow status legends before treating a document as implemented evidence.
3. Use relative links inside `documentations/` when documents are meant to be
   browsed together.
4. Do not create a legacy `docs/` compatibility folder. Update editable
   references to `documentations/` instead.
5. Do not rewrite protected top-level doctrine and planning files unless the
   task explicitly grants permission.

## Primary indexes

| Area | Index | Purpose |
| --- | --- | --- |
| Architecture | [Architecture Index](architecture/index.md) | Doctrine, protected architecture references, workspace ledgers, decisions, and cross-links. |
| Specifications | [Specification Index](specs/index.md) | v0 contracts, byte-format specs, and honest draft-versus-implemented status. |
| Implementation | [Implementation Index](implementation/index.md) | Reality matrix, release-gap trackers, roadmap execution plans, and packaging guidance. |
| Operations runbooks | [Operations Runbook Index](operations/runbooks/index.md) | Incident and drill runbooks with implemented, contract-preview, dry-run, and planned-gap status. |
| Governance decisions | [Architecture Decision Records](governance/decisions/index.md) | ADR and DEC navigation, release gates, and decision status. |
| Testing and release evidence | [Testing Documentation Index](testing/index.md) | Validation matrices, release-gate context, unsafe inventory guidance, and evidence templates. |
| Release evidence | [Release Evidence Template](testing/release-evidence-template.md) | Template for exact command evidence and residual risk. |

## Protected doctrine and planning files

The following files are intentionally kept at the top level because they are
project-structuring references:

- [00 Andromeda Index Et Mode De Lecture](00_ANDROMEDA_INDEX_ET_MODE_DE_LECTURE.md)
- [01 Doctrine, Lexique, Architecture, Catalogue, Modelization](01_DOCTRINE_LEXIQUE_ARCHITECTURE_CATALOGUE_MODELIZATION.md)
- [02 Type System, SRPL, Procedures, Maps](02_TYPE_SYSTEM_SRPL_PROCEDURES_MAPS.md)
- [03 Transaction, WAL, MVCC, Storage, Recovery](03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md)
- [04 QUIC, RPC, Security, HA/DR, Operations](04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md)
- [05 Optimizer, Stats, Analytics, Hardware, Roadmap Sources](05_OPTIMIZER_STATS_ANALYTICS_HARDWARE_ROADMAP_SOURCES.md)
- [Andromeda Roadmap Implementation Crosscheck 2026](ANDROMEDA_ROADMAP_IMPLEMENTATION_CROSSCHECK_2026.md)
- [Current State](CURRENT_STATE.md)
- [Roadmap Implementation 2026](ROADMAP_IMPLEMENTATION_2026.md)
- [Roadmap Restructure Status 2026-05-08](ROADMAP_RESTRUCTURE_STATUS_2026_05_08.md)
- [Worker Execution Matrix 2026](WORKER_EXECUTION_MATRIX_2026.md)
- [Andromeda SGBDRT SRPL Master Consolidation 2026](Andromeda_SGBDRT_SRPL_Master_Consolidation_2026.pdf)

## Supporting areas

| Directory | Purpose |
| --- | --- |
| [agent-operations](agent-operations/) | AI operating architecture, research basis, and agent/skill matrix. |
| [architecture](architecture/index.md) | Architecture navigation, current workspace restructure baseline, crate/module inventory, dependency matrix, criticality ledger, and reexport migration ledger. |
| [developer-guides](developer-guides/) | CLI and installation guidance. |
| [governance/decisions](governance/decisions/index.md) | Architecture decision records and release-gate decisions. |
| [implementation](implementation/index.md) | Implementation-state audits, roadmap plans, and release-gap trackers. |
| [operations](operations/) | Security, maintenance, benchmarking, troubleshooting, and runbooks. |
| [reference](reference/) | Schemas, output contracts, and test vectors. |
| [specs](specs/index.md) | v0 specifications and contract status. |
| [testing](testing/index.md) | Validation matrices, unsafe/Miri inventory guidance, and release evidence templates. |

## Validation

Documentation-only edits should use targeted link, status, and terminology
checks. Rust workspace gates, crash/recovery gates, fuzz, Miri, Loom, security,
policy, and supply-chain checks are required only when the linked implementation
or release task calls for them.

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| A document exists but has unclear implementation status. | A reader inferred status from location or filename. | Use the relevant index and the document's own status section before citing it. |
| A README or index claims production readiness without exact command output. | Documentation wording drifted ahead of validation evidence. | Reword the claim as contract target, partial implementation, or planned gap until evidence is recorded. |
| A link points to `docs/`. | Legacy path drift. | Update editable references to `documentations/` or a relative path in this tree. |

## References

- [Architecture Index](architecture/index.md)
- [Specification Index](specs/index.md)
- [Implementation Index](implementation/index.md)
- [Operations Runbook Index](operations/runbooks/index.md)
- [Architecture Decision Records](governance/decisions/index.md)
