# CI gates

> **Status:** Testing guidance  
> **Audience:** Maintainers, QA, engine developers, release owners  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, resolver 3, 89 workspace crates

## In this article

- Define the purpose of this test area.
- State required evidence.
- Provide acceptance checks.

## Scope

This document applies to Andromeda documentation, implementation planning, and release readiness.
CI gates protect the P00 repository baseline and later phase evidence. The root
`Cargo.toml` is the authority for workspace membership and Rust baseline:
Rust 1.95.0, Rust 2024 Edition, resolver 3, and 89 workspace crates.

CI results must be interpreted as evidence records, not readiness claims. A
passing command only proves the stated command on the recorded commit, platform,
toolchain, and feature scope.

## Required coverage

| Area | Requirement |
|---|---|
| `cargo fmt` | Must have explicit test evidence or a documented exclusion. |
| `cargo check` | Must have explicit test evidence or a documented exclusion. |
| `cargo test` | Must have explicit test evidence or a documented exclusion. |
| `cargo clippy` | Must have explicit test evidence or a documented exclusion. |
| `cargo nextest` | Must have explicit test evidence or a documented exclusion. |
| `cargo test --doc` | Must have explicit test evidence or a documented exclusion. |
| `cargo audit` | Must have explicit test evidence or a documented exclusion. |
| `cargo deny` | Must have explicit test evidence or a documented exclusion. |
| `cargo vet` | Must have explicit test evidence or a documented exclusion. |
| `Miri` | Must have explicit test evidence or a documented exclusion. |
| `cargo-fuzz` | Must have explicit test evidence or a documented exclusion. |
| `storage/recovery gate` | Must have explicit test evidence or a documented exclusion. |

## P00 baseline gates

| Gate | Command | Expected status | Notes |
|---|---|---:|---|
| Workspace metadata | `cargo metadata --no-deps --locked --format-version 1` | PASS | Confirms locked workspace resolution and current member set. |
| Rust typecheck | `cargo check --workspace --locked` | PASS | Broad local compile gate for 89 workspace crates. |
| Workspace tests | `cargo test --workspace --locked` | PASS | Broad crate-owned test gate; retained output required for release packets. |
| Clippy | `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS | Warning-free policy for release candidates. |
| Formatting | `cargo fmt --all -- --check` | PASS or BLOCKED | If blocked by host path length, record the exact error and run targeted formatting only as a scoped fallback, not a release-wide pass. |
| Supply chain | `cargo deny check` | PASS or BLOCKED | Duplicate-version warnings remain cleanup work unless accepted by release-owner decision. |
| Roadmap gate summary | `python -B tools/testing/roadmap_gate_summary.py --format json` | PASS | Fails when required CI/release workflows or roadmap gate inputs are missing. |
| Validation manifest | `python -B tools/testing/validation_manifest.py --json` | PASS | `BLOCKED` means release remains closed even if required indices exist. |
| Release evidence metadata | `python -B tools/testing/release_evidence.py --json` | PASS | Generator records metadata only; it does not run gates or approve release. |

## Operations-readiness gates

| Gate | Command | Evidence limit |
|---|---|---|
| Backup/restore/PITR local readiness | `python -B tools/testing/backup_restore_drill_check.py --format json` | Read-only readiness check. Release proof still requires retained restore/PITR drill artifacts. |
| HA/DR local readiness | `python -B tools/testing/hadr_cluster_drill_check.py --format json` | Simulation-only readiness check. Release proof still requires retained cluster drill artifacts. |
| Crash/recovery matrix | `python -B tools/testing/crash_matrix_check.py --format json` | Matrix presence is not a substitute for retained crash/recovery transcripts. |

## CI workflow requirements

Before a release candidate can be promoted, CI must contain enforceable quality
and release workflows or an approved release-owner exclusion. P00 treats missing
workflow files reported by `roadmap_gate_summary.py` as release blockers because
local command output is not sufficient release evidence by itself.

## General rules

- Tests are evidence, not ceremony.
- C5 paths require crash or recovery evidence when durable state is affected.
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
```

## Rejection criteria

Reject release readiness when:

- required P00 baseline commands were not run or their failure was not recorded;
- `validation_manifest.py` reports `BLOCKED` and blockers are not closed or
  explicitly accepted in the release evidence packet;
- `roadmap_gate_summary.py` reports missing release or quality workflows without
  an approved infrastructure exclusion;
- commit visibility is not crash-tested;
- recovery does not emit RecoveryReport;
- WAL parser has no malformed-input tests;
- RPC payload length is allocated before validation;
- a GPU path has no CPU fallback;
- a security decision lacks audit evidence.
