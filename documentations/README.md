# Andromeda Documentation

This directory is the canonical documentation surface for Andromeda.

## Protected Doctrine And Planning Files

The following files are intentionally kept at the top level because they are project-structuring references:

- `00_ANDROMEDA_INDEX_ET_MODE_DE_LECTURE.md`
- `01_DOCTRINE_LEXIQUE_ARCHITECTURE_CATALOGUE_MODELIZATION.md`
- `02_TYPE_SYSTEM_SRPL_PROCEDURES_MAPS.md`
- `03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md`
- `04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md`
- `05_OPTIMIZER_STATS_ANALYTICS_HARDWARE_ROADMAP_SOURCES.md`
- `ANDROMEDA_ROADMAP_IMPLEMENTATION_CROSSCHECK_2026.md`
- `CURRENT_STATE.md`
- `ROADMAP_IMPLEMENTATION_2026.md`
- `WORKER_EXECUTION_MATRIX_2026.md`
- `Andromeda_SGBDRT_SRPL_Master_Consolidation_2026.pdf`

Do not rewrite these files during documentation cleanup unless the task explicitly grants permission.

## Supporting Areas

| Directory | Purpose |
|---|---|
| `agent-operations/` | AI operating architecture, research basis, and agent/skill matrix. |
| `developer-guides/` | CLI and installation guidance. |
| `governance/decisions/` | Architecture decision records. |
| `implementation/` | Implementation-state audits and release-gap trackers. |
| `operations/` | Security, maintenance, benchmarking, and troubleshooting runbooks. |
| `reference/` | Schemas, output contracts, and test vectors. |

## Link Policy

Prefer links relative to the repository root in root documents, and relative links inside this directory when documents are meant to be browsed together. Do not create a legacy `docs/` compatibility folder; update editable references to `documentations/` instead.
