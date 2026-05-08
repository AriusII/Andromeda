# Andromeda Implementation Index

## Purpose

Provide the entry point for implementation-state ledgers, roadmap execution
plans, gap trackers, and packaging guidance. Use this page to distinguish
working evidence from release approval.

## Scope

This index covers implementation documents under `documentations/implementation`
and links to related validation, specification, architecture, and operations
documents.

## Non-goals

This index does not approve release readiness, validate the dirty worktree,
create or move crates, stage or commit files, replace ADRs or DECs, or promote
contract previews to implemented durable behavior.

## Prerequisites

Before using these ledgers for planning, confirm:

- The current branch and dirty-worktree state are known.
- The path owner for any implementation packet is explicit.
- C5 storage, WAL, recovery, transaction, security, RPC, catalog publication,
  backup, restore, and HA/DR claims have source evidence.
- Any release claim has exact command output recorded in a release evidence
  artifact.

## Procedure

1. Read the status legend.
2. Open the ledger that matches the question: reality, release gaps, roadmap
   execution, target crate gaps, or packaging.
3. Cross-check any implemented claim against the relevant specification and
   runbook.
4. Run targeted gates before broad workspace gates when the worktree is dirty.
5. Use the release evidence template or local evidence generator only to record
   metadata and declared check results; do not treat generated metadata as a
   gate pass.
6. Record command, date, branch, pass or fail status, and residual risk before
   promoting any result.

## Status legend

| Status | Meaning |
| --- | --- |
| Working audit | Observes current tree behavior or structure, but does not approve release readiness. |
| Gap ledger | Tracks missing, partial, blocked, or residual work. |
| Execution plan | Orders future work and gates; it is not proof that those gates passed. |
| Packaging plan | Describes path-specific packaging in a dirty worktree; it is not a broad staging instruction. |
| Release evidence template | Captures command evidence; empty or unrun commands are not pass results. |

Current branch note: the root workspace declares 94 crates. This count reflects
branch shape only; multiple crates remain scaffolds, behavior-free contract
surfaces, or compatibility facades.

## Implementation register

| Document | Status | Use this document for |
| --- | --- | --- |
| [Implementation Reality Matrix](implementation-reality-matrix.md) | Working audit. It classifies component behavior from interface-only through recovery-backed functional, and explicitly notes remaining blockers. | Current component maturity, mock/scaffold detection, durable versus in-memory behavior, and highest-risk implementation gaps. |
| [V1 Gap Closure Tracker](v1-gap-closure-tracker.md) | Gap ledger. It summarizes partial coverage and residual risk; it states that build, test, fuzz, and benchmark commands were not run by that documentation pass. | Release-gate coverage, residual risk IDs, and next targeted gates before any V1 readiness claim. |
| [Roadmap Execution Plan - 2026-05-08](roadmap-execution-plan-2026-05-08.md) | Execution plan. It maps the external roadmap to the current 94-crate branch shape and dirty-worktree constraints. | Future worker sequencing, phase gates, C5 containment, and acceptance-gate planning. |
| [Target Crate Gap Ledger - 2026-05-08](target-crate-gap-ledger-2026-05-08.md) | Gap ledger. It identifies target crate gaps and explicitly does not approve C5 extraction while the worktree is dirty. | Missing target crate names, current broad owners, and acceptance gates before crate extraction. |
| [Worktree Packaging Plan - 2026-05-08](worktree-packaging-plan-2026-05-08.md) | Packaging plan. It describes pathspec-only packaging for a dirty worktree and warns against broad staging. | Packet order, staged-versus-working divergence handling, current `AD` risk register, and validation expectations by packet. |
| [Roadmap PR Packaging Guide - 2026-05-08](roadmap-pr-packaging-2026-05-08.md) | Packaging guide. It splits the 2026-05-08 roadmap wave into review lots for PR and release triage, while explicitly forbidding automatic staging or commits on the dirty branch. | Review lot boundaries, owner handoff checks, package validation expectations, and release-evidence prerequisites. |
| [Worker Wave Results - 2026-05-08](worker-wave-results-2026-05-08.md) | Working audit. It summarizes observed worker-wave output and blockers, but does not claim acceptance, release readiness, production readiness, or a clean compile. | Concrete wave output by roadmap step, new crates, new specs, tests, tooling, documentation, validation blockers, and packet-planning evidence. |

## Related validation and evidence

| Area | Link | Why it matters |
| --- | --- | --- |
| Release validation matrix | [Step 11 Validation Matrix](../testing/step-11-validation-matrix.md) | Lists workspace, C5, crash/recovery, fuzz, Miri, Loom, and supply-chain validation gaps. |
| CI release gate evidence | [CI Release Gate Evidence](../testing/ci-release-gate-evidence.md) | Defines expected release evidence commands, retained artifacts, and non-release advisory boundaries. |
| Release evidence capture | [Release Evidence Template](../testing/release-evidence-template.md) | Provides the format for exact command output, date, branch, and residual risk. |
| Local release evidence generator | [Release Evidence Schema](../../tools/testing/release_evidence_schema.md) | Describes `tools/testing/release_evidence.py`; the generator records metadata but does not run gates. |
| Bounded Miri subset | [Miri Subset - 2026-05-08](../testing/miri-subset-2026-05-08.md) | Lists the current `tools/testing/miri_subset.py` commands and exclusions for targeted Miri evidence. |
| Operations runbooks | [Operations Runbook Index](../operations/runbooks/index.md) | Separates implemented durable behavior from contract previews, dry-run steps, and planned gaps. |
| Specifications | [Specification Index](../specs/index.md) | Shows which v0 documents are implemented evidence, partial implementation, contract targets, or planned gaps. |
| Architecture context | [Architecture Index](../architecture/index.md) | Links protected doctrine, ADR and DEC records, and workspace restructure context. |

## Validation

This index is documentation-only. It should be validated by targeted link and
content checks. It does not replace Rust formatting, workspace build, clippy,
nextest, crash/recovery, fuzz, Miri, Loom, security, policy, audit, or
supply-chain gates.

Gate posture for this consolidation index:

- `cargo check --workspace --all-targets --all-features` is continuity only.
- Remaining required gates include clippy, nextest, doctest, audit, deny,
  sustained fuzz, Miri, Loom, combined C5 crash/recovery, and release gate
  chain evidence.

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| A ledger says a path is partially covered, but a release note says complete. | Release wording drifted ahead of current evidence. | Treat the ledger as the current working status until exact candidate gate output exists. |
| A worker wants to package unrelated dirty files together. | Packet boundaries are unclear. | Use the worktree packaging plan and pathspec-only commits after owner reconciliation. |
| A crate extraction is proposed for C5 storage or recovery while the tree is dirty. | Roadmap execution is being treated as permission rather than gated planning. | Stop the extraction and require behavior locks, source evidence, and targeted crash/recovery validation first. |

## References

- [Andromeda Documentation](../README.md)
- [Architecture Index](../architecture/index.md)
- [Specification Index](../specs/index.md)
- [Operations Runbook Index](../operations/runbooks/index.md)
- [CI Release Gate Evidence](../testing/ci-release-gate-evidence.md)
- [Step 11 Validation Matrix](../testing/step-11-validation-matrix.md)
- [Miri Subset - 2026-05-08](../testing/miri-subset-2026-05-08.md)
