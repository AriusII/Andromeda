# Release gates

> **Status:** Testing guidance  
> **Audience:** Maintainers, QA, engine developers, release owners  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, resolver 3, 89 workspace crates

## In this article

- Define the purpose of this test area.
- State required evidence.
- Provide acceptance checks.

## Scope

This document applies to Andromeda documentation, implementation planning, and release readiness.
It is grounded in the P00 repository truth snapshot: the root `Cargo.toml` is the
workspace authority for Rust 1.95.0, Rust 2024 Edition, resolver 3, and 89
workspace crates.

Release gates are refusal gates. A gate can be `PASS`, `FAIL`, `BLOCKED`,
`SKIPPED`, or `NOT IN SCOPE`, but it cannot be silently assumed from the
existence of a crate, ADR, runbook, benchmark, trace, or roadmap item.

## Required coverage

| Area | Requirement |
|---|---|
| `no unreviewed unsafe` | Must have explicit test evidence or a documented exclusion. |
| `audit clean or exception` | Must have explicit test evidence or a documented exclusion. |
| `deny clean` | Must have explicit test evidence or a documented exclusion. |
| `vet gaps closed or accepted` | Must have explicit test evidence or a documented exclusion. |
| `crash recovery passed` | Must have explicit test evidence or a documented exclusion. |
| `backup restore passed` | Must have explicit test evidence or a documented exclusion. |
| `forensic start passed` | Must have explicit test evidence or a documented exclusion. |
| `release artifacts identified` | Must have explicit test evidence or a documented exclusion. |
| `Backup/PITR/HA/DR` | Must have retained restore drill, PITR, quorum, fencing, promotion, and audit evidence or a documented release exclusion. |

## P00 release-operation gates

| Gate | Required evidence | Release decision rule |
|---|---|---|
| Repository baseline truth | `cargo metadata --no-deps --locked --format-version 1` or equivalent retained output showing 89 workspace crates, plus `Cargo.toml` review for `rust-version = "1.95.0"`, `edition = "2024"`, and `resolver = "3"`. | Block release if the retained evidence conflicts with `Cargo.toml` or any release note claims 88 crates. |
| CI workflow presence | `python -B tools/testing/roadmap_gate_summary.py --format json` retained output. | Block release when required quality or release workflows are missing unless the release packet documents an approved infrastructure exclusion. |
| Validation manifest | `python -B tools/testing/validation_manifest.py --json` retained output. | `BLOCKED` is not a release approval. Every `known_blockers` entry must be closed, scoped out, or accepted by release owner sign-off. |
| Release evidence packet | `python -B tools/testing/release_evidence.py --json` plus supplied check records for every gate actually run. | Block release if the packet has no commit SHA, dirty-worktree explanation, gate commands, statuses, artifact paths, skipped scopes, and residual risks. |
| Backup and restore drill | `python -B tools/testing/backup_restore_drill_check.py --format json` plus retained real restore/PITR drill transcript when the release claims recoverability. | The script is readiness evidence only. Production backup/PITR claims require backup id, snapshot id, WAL range, target LSN, `RestoreTrace`, `RecoveryReport`, and open-mode decision. |
| HA/DR cluster drill | `python -B tools/testing/hadr_cluster_drill_check.py --format json` plus retained cluster drill transcript when the release claims HA/DR. | The script is simulation readiness only. HA/DR claims require primary crash or partition scenario, quorum proof, fencing proof, candidate recovery, promotion, manifest update, and replica repointing evidence. |
| Runbook linkage | `docs/runbooks/README.md` links every operational release claim to an operator procedure and evidence capture rule. | Block release if an operational claim has no runbook or the runbook says evidence is advisory-only. |

## Evidence retention rules

- Record the exact command, exit code, commit SHA, branch, toolchain, platform,
  and artifact path for each gate.
- Record skipped or unavailable tools as `SKIPPED` with a release-owner decision;
  do not convert missing evidence into `PASS`.
- Treat generated release evidence, validation manifests, helper-script output,
  benchmarks, RAM state, GPU output, and traces as evidence about a run, not as
  system truth.
- For C5 durability, security, catalog, WAL, storage, transaction, recovery,
  backup, restore, PITR, and HA/DR claims, retained evidence must include the
  relevant crash/recovery, audit, or fail-closed behavior.

## General rules

- Tests are evidence, not ceremony.
- C5 paths require crash or recovery evidence when durable state is affected.
- Backup, restore, PITR, and HA/DR claims require retained operational evidence;
  helper scripts and runbooks do not certify production readiness by themselves.
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
ArtifactPath
ResidualRisk
ReleaseOwnerDecision
```

## Rejection criteria

Reject release readiness when:

- workspace count, Rust version, Edition, or resolver claims conflict with the
  root `Cargo.toml`;
- release evidence is missing exact gate commands, statuses, artifacts, or
  skipped-scope decisions;
- commit visibility is not crash-tested;
- recovery does not emit RecoveryReport;
- WAL parser has no malformed-input tests;
- RPC payload length is allocated before validation;
- a GPU path has no CPU fallback;
- a security decision lacks audit evidence;
- backup/PITR or HA/DR readiness is claimed from local readiness scripts without
  retained restore, PITR, quorum, fencing, promotion, and audit drill evidence.
