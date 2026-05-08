# Andromeda Architecture Index

## Purpose

Provide the entry point for Andromeda architecture, doctrine, decision, and
cross-reference documents under `documentations/`. Use this page to find the
authoritative architecture context before reading specifications, runbooks, or
implementation ledgers.

## Scope

This index covers protected top-level architecture references, architecture
decision records, the current workspace restructure baseline, crate and module
inventory ledgers, dependency and criticality matrices, facade migration
ledgers, and the related specification, runbook, and implementation-ledger
indexes.

## Non-goals

This index does not replace ADR or DEC authority, approve the dirty worktree as
release-ready, create new architecture decisions, or claim implementation
completion for C5 storage, WAL, recovery, transaction, security, RPC, catalog,
backup, restore, or HA/DR behavior.

## Prerequisites

Before using this index for planning or review, read:

- [Andromeda Documentation](../README.md) for documentation area ownership.
- [Current State](../CURRENT_STATE.md) for the latest summarized repository
  posture.
- [Implementation Index](../implementation/index.md) for current reality and
  release-gap ledgers.
- [Specification Index](../specs/index.md) for v0 contract status.

## Procedure

1. Start with the protected doctrine and planning references for conceptual
   boundaries.
2. Use the workspace restructure baseline for branch-shape context only.
3. Use ADR and DEC records for accepted decisions and release gates.
4. Cross-check implementation claims against the implementation index.
5. Use the specification and runbook indexes for detailed contract and
   operational status.

## Architecture register

| Document | Status | Use this document for |
| --- | --- | --- |
| [00 Andromeda Index Et Mode De Lecture](../00_ANDROMEDA_INDEX_ET_MODE_DE_LECTURE.md) | Protected planning and reading reference. | Reading order, documentation orientation, and consolidated project context. |
| [01 Doctrine, Lexique, Architecture, Catalogue, Modelization](../01_DOCTRINE_LEXIQUE_ARCHITECTURE_CATALOGUE_MODELIZATION.md) | Protected doctrine and architecture reference. | Non-negotiable engine doctrine, catalog and modelization vocabulary, and architecture framing. |
| [02 Type System, SRPL, Procedures, Maps](../02_TYPE_SYSTEM_SRPL_PROCEDURES_MAPS.md) | Protected architecture reference; implementation status must be checked through specs and ledgers. | Type system, SRPL, Procedure contracts, StructuredObjects, maps, and related conceptual boundaries. |
| [03 Transaction, WAL, MVCC, Storage, Recovery](../03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md) | Protected architecture reference; not release proof. | WAL-before-visible-commit, transaction, MVCC, storage, recovery, snapshot, and manifest concepts. |
| [04 QUIC, RPC, Security, HA/DR, Operations](../04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md) | Protected architecture reference with operational implications. | RPC over QUIC, surface separation, security, Administration, HA/DR, backup, restore, and operations context. |
| [05 Optimizer, Stats, Analytics, Hardware, Roadmap Sources](../05_OPTIMIZER_STATS_ANALYTICS_HARDWARE_ROADMAP_SOURCES.md) | Protected architecture and source reference. | Optimizer, statistics, analytics, hardware, SIMD, GPU, benchmark, and roadmap source context. |
| [Dependency Edge Matrix - 2026-05-08](dependency-edge-matrix-2026-05-08.md) | Working architecture ledger. It records observed workspace manifest edges and does not accept coupling, prove release readiness, or lower topology gates. | Direct workspace dependency edges, dev-only edges, temporary exceptions, watch edges, and manifest-change review context. |
| [Engine Crate Mapping - 2026-05-08](engine-crate-mapping-2026-05-08.md) | Dirty-branch planning evidence only. The current workspace is ahead of older Step 0 snapshots and this document is not release readiness evidence. | Macro-engine to crate ownership mapping, compatibility facades, provisional crate shells, dependency direction, and validation gate selection. |
| [Module Criticality C0-C5 - 2026-05-08](module-criticality-c0-c5-2026-05-08.md) | Validation planning ledger. It assigns default criticality and escalation rules, but does not claim any C4 or C5 path is complete. | C0-C5 classification, escalation rules, minimum validation posture, and blocked C5 claim tracking. |
| [Module Inventory - 2026-05-08](module-inventory-2026-05-08.md) | Working inventory ledger. It records current crate and top-level module surfaces and must not be treated as proof of completeness or release readiness. | Crate inventory, source and test file counts, top-level module surfaces, broad owner watchlists, and packet sizing context. |
| [Reexport Migration Ledger - 2026-05-08](reexport-migration-ledger-2026-05-08.md) | Working migration ledger. It documents compatibility reexports and facade debt without transferring canonical ownership or approving facade removal. | Compatibility facade paths, canonical owners, guard evidence, exit criteria, and owner-versus-facade evidence separation. |
| [Workspace Restructure Baseline 2026](WORKSPACE_RESTRUCTURE_BASELINE_2026.md) | Current branch baseline, not release-ready acceptance. | 94-crate branch shape, dirty-worktree constraints, packet ordering, and validation posture. |
| [Architecture Decision Records](../governance/decisions/index.md) | Decision index with per-record status. | Accepted, superseded, proposed, and release-gate decisions across storage, recovery, transaction, compiler, catalog, security, protocol, and observability. |

## Cross-reference map

| Need | Link | Status guidance |
| --- | --- | --- |
| v0 contracts and byte-format specifications | [Specification Index](../specs/index.md) | Read each specification status before treating it as implemented. |
| Current implementation reality and release gaps | [Implementation Index](../implementation/index.md) | Working audits and ledgers are not release approval records. |
| Operational incident and drill guidance | [Operations Runbook Index](../operations/runbooks/index.md) | Runbooks distinguish implemented durable behavior, contract previews, dry-run behavior, and planned gaps. |
| Release validation evidence | [Step 11 Validation Matrix](../testing/step-11-validation-matrix.md) | Use for gate planning; do not infer pass status without exact command output. |
| Evidence capture format | [Release Evidence Template](../testing/release-evidence-template.md) | Use to record command evidence and residual risk for candidate approval. |

## Validation

This index was written as documentation navigation only. It does not run or
replace architecture, Rust, crash/recovery, fuzz, Miri, Loom, security,
protocol, or release gates. Validate architecture-sensitive implementation
claims with the commands and acceptance gates in the linked decisions,
specifications, and implementation ledgers.

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| A protected planning document conflicts with a later DEC. | A decision record has narrowed or superseded older planning language. | Use the DEC or ADR as the decision authority and update editable navigation only. |
| A branch-shape statement is treated as acceptance evidence. | The workspace baseline was read without its dirty-worktree caveat. | Reclassify it as planning context until exact validation output exists. |
| A runtime claim lacks a spec or runbook link. | The architecture claim is not grounded enough for review. | Link the relevant specification, runbook, or implementation ledger before using the claim. |

## References

- [Andromeda Documentation](../README.md)
- [Specification Index](../specs/index.md)
- [Implementation Index](../implementation/index.md)
- [Operations Runbook Index](../operations/runbooks/index.md)
- [Architecture Decision Records](../governance/decisions/index.md)
