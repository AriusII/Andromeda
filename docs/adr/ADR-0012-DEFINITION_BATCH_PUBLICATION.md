# ADR-0012-DEFINITION BATCH PUBLICATION — DefinitionBatch transactional publication

> **Status:** Accepted for V0 documentation baseline  
> **Scope:** Andromeda architecture and implementation governance  
> **Baseline:** Rust 1.95.0

## Context

Andromeda targets an enterprise-grade relational transactional engine with a strict Procedure surface, typed contracts, WAL-first durability, recovery evidence, and bounded adaptive internals.

## Decision

Catalog changes publish through validated, ordered, WAL-covered batches.

DefinitionBatch is the only V0 publication path for catalog mutation. A catalog mutation is visible only after all of the following are true:

- dry-run accepted the exact batch id, source hash, dependency graph hash, previous catalog version, next catalog version, operation count, operation order, compatibility decisions, policy version, and requester evidence;
- apply consumed that accepted dry-run without recomputing a different graph, order, compatibility decision, or publication target;
- mutation WAL contains a matching begin record, every operation record, and a commit record for the batch;
- the commit record is inside the durable WAL prefix before the next catalog version becomes visible;
- a publication receipt and audit/trace evidence can be produced for the accepted catalog version.

V0 active lifecycle operations are `Create` and `Deprecate`. `Alter` is reserved unless represented by a compatible create of the same identity at the next version. `Drop`, `Rename`, and `Move` are reserved and rejected until a superseding ADR and specification define dependency closure, retained evidence, recovery, and restore semantics.

Rollback is a pre-commit behavior only. If apply fails before durable commit, the previous catalog version remains visible and rollback or recovery-skip evidence is emitted. After durable commit, history is not removed in place; reversal requires a new DefinitionBatch.

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
- Implementations must carry publication evidence through dry-run, apply, WAL, recovery, security audit, and operator-visible receipts.
- Unsupported operation classes must fail with stable rejection codes instead of falling back to ad hoc migration behavior.

## Validation

This ADR is validated by:

- a matching specification when the decision affects a technical structure;
- a test plan when the decision affects runtime behavior;
- a runbook when the decision affects operations;
- trace or audit evidence when the decision affects security, durability, or recovery.

For DefinitionBatch publication, validation must include tests for:

- apply without accepted dry-run;
- dry-run/apply batch id, source hash, graph hash, operation count, policy version, and catalog version drift;
- dependency cycle, missing target, kind mismatch, and forward same-batch reference rejection;
- compatibility and security downgrade rejection;
- failure before begin, after begin, during operation apply, after operation records but before commit, and commit outside the durable WAL prefix;
- recovery replay of a complete committed batch and recovery skip of every incomplete boundary;
- publication receipt and security audit trace linkage.

## Rejection criteria

Reject implementation work that contradicts this decision without a superseding ADR.
