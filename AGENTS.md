# AGENTS.md — Andromeda Repository Instructions

## Project identity

Andromeda is a modern 2026 relational transactional database project. It is not a generic SQL server. The native application surface is:

```text
QUIC + custom typed RPC + cataloged Procedure + SRPL + typed ResultStream
```

Use American English for generated repository documents unless a task explicitly requests another language.

## Non-negotiable invariants

1. No ad hoc SQL on the application surface.
2. Every application execution goes through a cataloged Procedure.
3. Every Procedure has a typed, hashed, versioned contract.
4. Every Procedure is transactionally scoped.
5. No visible commit without durable WAL.
6. RAM is never system truth.
7. System truth is the last valid cold snapshot plus durable WAL since that snapshot.
8. GPU never participates in commit, rollback, WAL, recovery, MVCC visibility, or security-critical paths.
9. Predictive evidence never decides alone.
10. Active plans are tied to `CatalogVersion + StatsVersion + ContractHash`.
11. Every critical decision must be observable and explainable after the fact.
12. Every feature must be definable, deterministic or explicitly bounded, typed, observable, recoverable after crash, versioned, explainable, and disableable.

## Native terminology

Prefer Andromeda terms over SQL analogies:

| Use | Avoid as native term |
|---|---|
| Procedure / Invocation | Query |
| Procedure Store | Query Store |
| Map | View |
| Modelization | Model |
| DefinitionBatch | Migration script |
| ResultStream | Result set |
| StructuredObject | TVP / dynamic record |

SQL Server, Oracle, PostgreSQL, and research literature may be used as references, but they do not define the native Andromeda surface.

## Engineering posture

- Preserve strict module boundaries.
- Prefer explicit contracts over runtime discovery.
- Prefer deterministic scripts over repeated ad hoc shell fragments.
- Prefer small patches with clear validation.
- Do not introduce dynamic SQL text, implicit nullability, non-bounded loops, random unseeded behavior, or filesystem/network access from Procedures.
- Do not remove audit, WAL, version, recovery, or security requirements to simplify a design.

## Validation expectations

For code or spec changes, report:

```text
Changed files
Reason for change
Andromeda invariants touched
Validation commands run
Tests added or updated
Residual risks
```

When checks cannot be run, state that explicitly and explain why.

## Recommended specialist routing

- SRPL syntax/type/cardinality: use `srpl-language-specifier`.
- WAL/MVCC/recovery: use `transaction-wal-recovery-auditor`.
- Page/storage/manifest: use `storage-engine-page-layout-auditor`.
- QUIC/RPC/contracts: use `quic-rpc-contract-auditor`.
- Security/IAM/audit: use `security-iam-threat-modeler`.
- Optimizer/statistics/evidence: use `optimizer-statistics-critic`.
- Hooks/skills/agent tooling: use `hooks-governance-auditor` or `codex-skill-maintainer`.

## Pull request guidance

A PR should include:

1. Summary.
2. Design rationale.
3. Andromeda invariants preserved.
4. Tests and validation.
5. Recovery/security/compatibility impact.
6. Known limitations.

Do not present unvalidated assumptions as facts.
