# ADR-0002-WORKSPACE AND CRATE BOUNDARIES — Workspace and crate boundaries

> **Status:** Accepted for V0 documentation baseline  
> **Scope:** Andromeda architecture and implementation governance  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, resolver 3, 89 crates

## Context

Andromeda targets an enterprise-grade relational transactional engine with a strict Procedure surface, typed contracts, WAL-first durability, recovery evidence, and bounded adaptive internals.

## Decision

Use a Cargo workspace and keep crates aligned to engine responsibilities. The P00 baseline is the 89-crate workspace declared by the root `Cargo.toml`.

Each crate must have a responsibility-named owner boundary. Generic catch-all ownership such as `common`, `utils`, `misc`, `helpers`, or `god_engine` is rejected because it hides criticality and review responsibility.

Lower-criticality crates must not control higher-criticality invariants. In particular, support, benchmark, observability, CLI, analytics, and GPU-adjacent crates must not own Procedure admission, WAL durability, catalog truth, MVCC visibility, recovery truth, authorization truth, or visible commit behavior.

## Boundary proof rule

Workspace and crate-boundary claims require:

| Claim | Required evidence |
|---|---|
| Workspace membership | Root `Cargo.toml` or `cargo metadata --format-version 1 --no-deps` showing 89 workspace members. |
| Crate responsibility | Owning crate README, ADR, specification, or crate matrix entry. |
| Criticality | `docs/project/CRITICALITY_MODEL.md` and `docs/project/CRATE_CLUSTER_CRITICALITY_MATRIX.md`. |
| Dependency safety | Targeted topology tests or review evidence for the affected boundary. |

Passing a broad workspace command is not enough to prove a C4/C5 boundary. The retained evidence must prove the specific ownership invariant that changed.

## Rationale

This decision reduces ambiguity and prevents implementation drift across architecture, code, tests, and operations.

## Consequences

### Positive

- The implementation boundary is explicit.
- Reviewers can reject incompatible shortcuts.
- Tests can be mapped to the decision.
- Operational behavior is easier to explain after an incident.

### Negative

- Some implementation shortcuts are intentionally unavailable.
- Additional tests and documentation are required.
- Experimental work must be isolated before entering critical paths.

## Validation

This ADR is validated by:

- root `Cargo.toml`, crate READMEs, and the P00 crate criticality matrix for workspace membership and ownership evidence;
- a matching specification when the decision affects a technical structure;
- a test plan when the decision affects runtime behavior;
- a runbook when the decision affects operations;
- trace or audit evidence when the decision affects security, durability, or recovery.

## Rejection criteria

Reject implementation work that contradicts this decision without a superseding ADR.

Reject changes that move C5 behavior into convenience crates, introduce a generic ownership bucket, or rely on unretained documentation claims for workspace topology.
