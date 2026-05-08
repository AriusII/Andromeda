# Release Risk Register - 2026-05-08

## Purpose

Record the current release governance risks and blockers observed on 2026-05-08.

This register is a current risk artifact. It does not approve a release, replace an
accepted decision record, or modify historical DEC-035 or DEC-036 content.

## Scope

This register covers release readiness risks for the current Andromeda workspace
state, including:

- source reproducibility;
- Rust MSRV and dependency compatibility;
- local Windows validation readiness;
- crash/recovery and fuzz evidence;
- release-proof validity for decision records and evidence artifacts.

The affected risk class is mission-critical because the blockers can affect WAL
durability proof, visible commit proof, recovery proof, security admission proof,
or release governance authority.

## Non-goals

- Do not approve the current workspace for release.
- Do not alter DEC-035, DEC-036, or any other historical decision content.
- Do not treat benchmark, RAM, GPU, temporary, or partial test output as durable
  truth.
- Do not infer release readiness from proposed, future-dated, missing, or
  unretained evidence.
- Do not replace executable validation with this register.

## Prerequisites

Before this risk register can be cleared for a release candidate, the release
owner must have:

- a clean, reproducible worktree;
- a current branch and commit SHA recorded in the release evidence packet;
- the Rust toolchain pinned by `rust-toolchain.toml`;
- a working linker on platforms that run Rust tests locally;
- retained artifacts for every required C4/C5 command;
- a current-dated release approval decision or equivalent release authority.

## Current Evidence Snapshot

| Field | Value |
| --- | --- |
| Snapshot date | 2026-05-08 |
| Current branch observed | `codex/workspace-crate-restructure` |
| Current commit observed | `5f053fd7efcb8a81577dfc3c197d761487837609` |
| Worktree status | Dirty; `git status --short` reported 466 entries |
| Rust baseline | `Cargo.toml` sets `rust-version = "1.95.0"` and `edition = "2024"` |
| Toolchain pin | `rust-toolchain.toml` sets `channel = "1.95.0"` |
| Local Rust tools observed | `rustc 1.95.0`; `cargo 1.95.0` under the pinned toolchain |
| Windows linker status | Plain PowerShell did not expose `link.exe`; the configured Visual Studio developer command environment provides the linker for local Rust gates |
| Release evidence status | Blocked until clean-worktree, retained MSRV and supply-chain evidence, crash/recovery, fuzz, Miri, and current-date proof gates pass |

## Current Blockers

| Blocker | Release impact | Required disposition |
| --- | --- | --- |
| Dirty worktree | The current source state is not reproducible release evidence. A release packet cannot prove which edits were validated. | Cut or select a clean release candidate, record branch, commit, and `git status --short`, then retain artifacts from that exact source state. |
| MSRV and dependency evidence incomplete | The workspace is pinned to Rust 1.95.0 and local `--locked` checks can pass, but the worktree and lockfile are dirty and release-retained evidence is incomplete. | Run and retain the dependency and workspace gates under Rust 1.95.0 with `--locked`; block release on any dependency requiring a higher MSRV or on any unexplained lockfile drift. |
| Plain-shell Windows linker preflight blocked | Rust tests that link binaries on Windows MSVC cannot complete from a shell where `link.exe` is absent. | Use the configured Visual Studio developer command environment, install or expose the Visual Studio Build Tools C++ linker, or use a retained CI artifact from a configured host. |
| Missing crash/recovery and fuzz release evidence | Existing matrices identify owner suites and smoke coverage, but sustained fuzz evidence and combined crash/recovery evidence are still required for promotion. | Produce retained Step 11 evidence records for combined WAL, storage, tx, exec, security, RPC, and fuzz gates before any C5 readiness claim. |
| Future-dated decision records | DEC-035 and DEC-036 are dated 2026-06-16, and DEC-037 is dated 2026-06-15. The current context date is 2026-05-08. | Treat these DEC records as indexed historical or planned governance records, not current release proof for 2026-05-08. Use current-dated evidence and approval artifacts for this release review. |

## Risk Register

| Risk ID | Risk | Severity | Likelihood | Status | Owner | Mitigation | Review trigger |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `GOV-RISK-2026-05-08-001` | Release validation is performed against a dirty worktree rather than a reproducible source candidate. | Critical | High | Blocking | Release owner | Require a clean release candidate, record the exact commit SHA, record `git status --short`, and attach validation artifacts from that state. | Any release-readiness approval, release branch cut, or evidence packet review. |
| `GOV-RISK-2026-05-08-002` | A dependency or feature update raises the effective MSRV beyond Rust 1.95.0 or changes lockfile resolution without release review. | High | Medium | Partially validated, blocked until retained release evidence exists | Supply-chain owner | Run `cargo check --workspace --all-targets --all-features --locked`, `cargo audit --deny warnings`, and `cargo deny check --all-features` under Rust 1.95.0. Record any dependency requiring a newer compiler as a release blocker. | Any `Cargo.toml`, `Cargo.lock`, `deny.toml`, or dependency workflow change. |
| `GOV-RISK-2026-05-08-003` | Windows validation cannot link Rust test binaries from a shell that does not expose `link.exe`. | High | Medium on the observed local Windows host | Partially mitigated, blocked until retained release evidence exists | CI/tooling owner | Use the Visual Studio developer command environment, verify `link.exe`, rerun the affected Rust gates, and retain logs. If CI supplies Windows evidence, record the CI host and artifact path. | Any Windows local test run, workspace check, or release evidence packet that claims Windows validation. |
| `GOV-RISK-2026-05-08-004` | Crash/recovery evidence remains incomplete for mission-critical durable paths. | Critical | High | Blocking for C5 promotion | Recovery validation owner | Use the Step 11 matrix to run and retain combined WAL, storage, tx, exec, manifest, catalog, backup/PITR, HA/DR, and visibility evidence where applicable. Isolated owner tests do not by themselves prove end-to-end durable visibility. | Any claim that a path can affect visible commit, WAL durability, recovery, page or manifest truth, catalog publication, backup, restore, or HA/DR readiness. |
| `GOV-RISK-2026-05-08-005` | Sustained fuzz evidence is missing for byte, parser, protocol, or admission surfaces. | Critical | High | Blocking for affected surface promotion | Fuzz validation owner | Run sustained fuzz campaigns for the affected targets, record duration, corpus, seed, toolchain, commit, pass/fail result, and artifact path. Do not count compile-only or short smoke runs as release proof. | Any release promotion of WAL, page, storage byte format, SRPL parser, RPC frame, QUIC envelope, ResultStream, or security admission surfaces. |
| `GOV-RISK-2026-05-08-006` | Future-dated DEC records are treated as current release proof. | High | High | Blocking for governance approval | Governance owner | Keep DEC-035, DEC-036, and DEC-037 indexed, but require current-dated evidence and approval artifacts for the 2026-05-08 release review. Do not edit historical DEC content under this work order. | Any release approval packet that cites DEC-035, DEC-036, or DEC-037 as current proof. |
| `GOV-RISK-2026-05-08-007` | Release evidence lacks exact command, date, toolchain, commit, artifact path, or residual risk. | High | Medium | Open | Release evidence owner | Use `documentations/testing/release-evidence-template.md` for every command, manual decision, crash drill, fuzz run, or skipped gate. Mark missing data as `Partial`, `Skipped`, or `Fail`, not `Pass`. | Any retained release artifact review. |

## Procedure

1. Start release review from a clean release candidate.
2. Capture branch, commit SHA, toolchain versions, platform, and `git status --short`.
3. Resolve or explicitly defer every blocking risk in this register.
4. Run the release-readiness gates in `release-readiness-gates-2026-05-08.md`.
5. Record one retained evidence entry for every command or manual gate.
6. Require release-owner review before changing any risk status to closed.

## Validation

This register is documentation-only. It was prepared from current repository
evidence and existing validation matrices. It does not claim that Rust tests,
crash drills, fuzz campaigns, supply-chain scans, or release gates passed.

Required validation before a release claim:

```powershell
cargo check --workspace --all-targets --all-features --locked
cargo audit --deny warnings
cargo deny check --all-features
cargo test -p andromeda-wal --tests --locked
cargo test -p andromeda-storage --test crash_recovery_impl --locked -- --nocapture
cargo test -p andromeda-storage --test recovery_completeness_contract --locked -- --nocapture
cargo test -p andromeda-storage --test property_recovery_replay --locked -- --nocapture
cargo test -p andromeda-storage --test wal_scan_recovery_contract --locked -- --nocapture
cargo test -p andromeda-storage --test wal_durability_fence_contract --locked -- --nocapture
cargo test -p andromeda-exec --test recovery_visibility_gates --locked -- --nocapture
cargo test -p andromeda-tx --test commit_log_durability --locked -- --nocapture
```

Add the protocol, security, backup/PITR, HA/DR, Miri, Loom, and sustained fuzz
commands that apply to the release scope.

## Troubleshooting

If the worktree is dirty, do not approve release evidence from it. Either cut a
clean candidate or explicitly classify the run as advisory.

If a dependency requires a newer Rust version than 1.95.0, block release until the
dependency is pinned, replaced, isolated, or the Rust baseline is changed through
accepted governance.

If `link.exe` is missing on Windows, Rust build and test commands may fail at
link time. Use the configured Visual Studio developer command environment, fix
the local build environment, or use retained CI artifacts from a known-good
Windows host.

If a crash/recovery or fuzz artifact is missing, mark the affected gate as
blocked. Do not replace it with a unit test, benchmark, compile check, or
free-form reviewer note.

If a decision record is dated after 2026-05-08, do not use it as current release
proof for this review cycle. Keep it as indexed governance context until a
current-dated approval artifact exists.

## References

- `AGENTS.md`
- `Cargo.toml`
- `rust-toolchain.toml`
- `documentations/testing/step-11-validation-matrix.md`
- `documentations/testing/release-evidence-template.md`
- `documentations/governance/supply-chain-policy.md`
- `documentations/governance/decisions/DEC-035-release-gate-chain.md`
- `documentations/governance/decisions/DEC-036-release-readiness-approval.md`
- `documentations/governance/decisions/DEC-037-risk-register-updates.md`
