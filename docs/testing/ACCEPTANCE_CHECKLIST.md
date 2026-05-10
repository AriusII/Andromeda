# Acceptance checklist

> **Status:** Testing guidance  
> **Audience:** Maintainers, QA, engine developers, release owners  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, resolver 3, 89 workspace crates

## In this article

- Define the purpose of this test area.
- State required evidence.
- Provide acceptance checks.

## Scope

This document applies to Andromeda documentation, implementation planning, and release readiness.
Every checklist item must resolve to code, tests, retained evidence, a
documented exclusion, or a known blocker. The checklist does not allow
production-readiness claims from crate existence, scaffolds, ADR text, roadmap
intent, helper-script presence, or runbook presence alone.

## Required coverage

| Area | Requirement |
|---|---|
| `SRPL compiler` | Must have explicit test evidence or a documented exclusion. |
| `Procedure contract` | Must have explicit test evidence or a documented exclusion. |
| `Transaction kernel` | Must have explicit test evidence or a documented exclusion. |
| `WAL` | Must have explicit test evidence or a documented exclusion. |
| `Storage` | Must have explicit test evidence or a documented exclusion. |
| `QUIC/RPC` | Must have explicit test evidence or a documented exclusion. |
| `Security/IAM` | Must have explicit test evidence or a documented exclusion. |
| `Optimizer/statistics` | Must have explicit test evidence or a documented exclusion. |
| `GPU policy` | Must have explicit test evidence or a documented exclusion. |
| `Backup/PITR` | Must have explicit test evidence or a documented exclusion. |

## P00 acceptance checklist

| Item | Acceptance evidence | Status rule |
|---|---|---|
| Baseline truth | Root `Cargo.toml` declares `resolver = "3"`, `edition = "2024"`, `rust-version = "1.95.0"`, and 89 workspace crates verified by metadata. | Required for P00 closure. |
| Current-state boundary | `docs/status.md` and `docs/roadmap/00_CURRENT_STATE_CROSS_CHECK.md` state prototype, scaffold, implemented, and release-blocked boundaries without production-readiness claims. | Required for P00 closure. |
| Release gates | `docs/testing/RELEASE_GATES.md` identifies release blockers, required retained evidence, backup/PITR/HA/DR drill limits, and rejection criteria. | Required before any release readiness discussion. |
| CI gates | `docs/testing/CI_GATES.md` lists broad Rust gates, validation scripts, and the interpretation of `PASS`, `FAIL`, `BLOCKED`, and documented exclusions. | Required before any release readiness discussion. |
| Operations runbooks | `docs/runbooks/README.md` maps backup, restore, PITR, forensic, and HA/DR runbooks to evidence capture requirements. | Required for Personne 20 P00 scope. |
| Release packet | `tools/testing/release_evidence.py --json` can capture metadata and declared check outcomes, while explicitly not approving readiness. | Required as release-evidence shape, not as release approval. |
| Known blockers | `tools/testing/validation_manifest.py --json` blocker output is carried into release decision records. | Any unresolved blocker keeps release status closed unless explicitly scoped out. |

## Status vocabulary

| Status | Meaning |
|---|---|
| `PASS` | The exact command or evidence item passed on the recorded commit, toolchain, platform, and scope. |
| `FAIL` | The exact command or evidence item failed and blocks the relevant claim. |
| `BLOCKED` | Required evidence or infrastructure is missing; do not claim readiness. |
| `SKIPPED` | Gate was intentionally not run and has a release-owner decision plus residual risk. |
| `NOT IN SCOPE` | Gate does not apply to the bounded change and has an explicit scope reason. |

## Production-readiness guardrail

No document, checklist, or mission report may state that Andromeda is
production-ready until release gates have retained pass evidence for the
applicable C4/C5 surface, supply-chain checks, crash/recovery behavior,
backup/PITR drills, HA/DR drills, audit evidence, and release artifacts. P00 can
close repository-state governance while the release gate remains closed.

## General rules

- Tests are evidence, not ceremony.
- C5 paths require crash or recovery evidence when durable state is affected.
- Operational claims require a runbook and retained drill artifact; runbook
  existence is not enough.
- Fuzz untrusted or semi-trusted byte parsers.
- Property-test codecs, ordering, hashes, and state machines.
- Do not normalize flaky tests through blind retries.

## Minimum evidence record

```text
TestId
Component
Criticality
Input model
Expected behavior
Observed behavior
TraceId when applicable
RecoveryReport when applicable
Decision
```

## Rejection criteria

Reject release readiness when:

- commit visibility is not crash-tested;
- recovery does not emit RecoveryReport;
- WAL parser has no malformed-input tests;
- RPC payload length is allocated before validation;
- a GPU path has no CPU fallback;
- a security decision lacks audit evidence.
