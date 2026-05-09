# Architecture

This directory is the canonical `/docs` surface for Andromeda architecture.
It consolidates the current workspace structure, crate ownership boundaries,
dependency policy, and durable-system invariants.

## Canonical documents

| Document | Purpose |
| --- | --- |
| [Overview](overview.md) | System planes, criticality scale, global invariants, and architectural anti-patterns. |
| [Workspace Topology](workspace-topology.md) | Current workspace package groups, owner surfaces, broad crates, and validation commands. |
| [Dependency Policy](dependency-policy.md) | Ring direction, temporary facades, topology gates, and dependency review rules. |

## Supporting material

[Cargo Dependency Matrix](CARGO_DEPENDENCY_MATRIX.md) is retained as generated
supporting evidence. The canonical policy is the dependency policy above.

## Cross-references

- ADRs under `docs/adr/` record decision history.
- Domain specifications under `docs/specs/` define formal subsystem contracts.
- Runbooks under `docs/runbooks/` cover operational procedure.

## Non-negotiable boundaries

1. No ad hoc SQL invocation surface; typed Procedures and SRPL-derived
   contracts are the accepted path.
2. No visible commit before durable WAL evidence.
3. No dirty page flush beyond durable WAL coverage.
4. No Rust native layout as durable or network format.
5. No GPU, benchmark, statistics, or plan evidence as durable truth.
6. No Administration, HA/DR, BackupAgent, or Forensic operation through the
   Application surface.
