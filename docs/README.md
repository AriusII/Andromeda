# Andromeda Docs

`/docs` is the canonical documentation entrypoint for Andromeda. Start here for
architecture, specifications, governance, implementation status, runbooks, and
testing guidance.

## Start Here

| Document | Use |
| --- | --- |
| [Project status](status.md) | Current documentation status, crate count, readiness boundaries, and migration notes. |
| [Architecture](architecture/README.md) | System planes, workspace topology, dependency policy, and durable-system invariants. |
| [Domain specifications](specs/README.md) | Canonical domain contracts for core, catalog, storage, transactions, RPC, security, HA/DR, and advisory systems. |
| [Implementation roadmap](implementation/roadmap.md) | Current implementation priorities and boundaries. |
| [Extraction status](implementation/extraction-status.md) | Current crate extraction and ownership status. |
| [Runbooks](runbooks/README.md) | Operational procedures and incident guidance. |
| [Testing](testing/README.md) | Test strategy, validation scope, fuzzing, recovery, and benchmark guidance. |
| [Governance](governance/README.md) | Human-readable repository governance policies. |
| [ADRs](adr/README.md) | Architecture decision records and open decision tracking. |

## Directory Map

| Path | Status |
| --- | --- |
| `architecture/` | Canonical architecture surface. |
| `specs/` | Canonical domain specification surface. |
| `implementation/` | Current implementation roadmap and status summaries. |
| `runbooks/` | Operational procedure surface. |
| `testing/` | Testing and validation guidance. |
| `governance/` | Repository governance policies. |
| `adr/` | Architecture decision records. |
| Top-level policy files | Legacy matrices retained only when they still describe current repository contracts. |

## Status Rules

- Treat `/docs` as the current entrypoint.
- Treat status pages and validation output as readiness evidence.
- Do not infer implemented behavior from a file name alone.
- Keep active documentation links inside `/docs`.

## Out Of Scope

This index does not approve release readiness or replace decision records.
