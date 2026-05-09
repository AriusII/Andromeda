# Testing Release Gates

## Purpose

Define the testing evidence expected before release readiness claims. This page
complements `docs/governance/release-gates.md` and focuses on command coverage
and artifact expectations.

## Gate Matrix

| Gate | Command shape | Acceptance rule |
| --- | --- | --- |
| Format | `cargo fmt --all --check` | Exit code `0`; no formatting drift. |
| Workspace check | `cargo check --workspace --all-targets --all-features --locked` | Full target and feature scope passes under locked dependencies. |
| Clippy | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | Exit code `0`; no warnings or unexplained allow-list expansion. |
| Nextest | `cargo nextest run --profile ci --workspace --all-features --locked` | Exit code `0`; skipped or flaky tests recorded as residual risk. |
| Doctests | `cargo test --doc --workspace --all-features --locked` | Exit code `0`; ignored or failed doctests recorded. |
| Supply chain | `cargo audit --deny warnings`; `cargo deny check --all-features` | No unreviewed advisories, license failures, source failures, or policy drift. |
| Crash/recovery | Owner WAL, storage, transaction, and execution recovery commands. | Durable WAL, replay, rejected corrupt tails, and post-recovery visibility are proven for scope. |
| Protocol/security/audit | Owner RPC, QUIC, security, execution, and audit commands. | Admission fails closed, contracts hold, wire formats are stable, and audit evidence is durable. |
| Backup/PITR/HA/DR | Owner backup, restore, membership, quorum, fencing, promotion, and WAL shipping commands. | Restore targets and promotion candidates satisfy retained evidence gates. |
| Fuzz/Miri/Loom | Target-specific commands selected from affected surfaces. | Evidence is retained and classified as pass, fail, partial, skipped, or blocked. |

## Combined Durable Gate

Use this set before claiming durable Procedure execution, recovery visibility, or
C5 storage readiness.

```powershell
cargo test -p andromeda-wal --tests --locked
cargo test -p andromeda-exec --test c5_combined_release_gate --locked -- --nocapture
cargo test -p andromeda-storage --test crash_recovery_impl --locked -- --nocapture
cargo test -p andromeda-storage --test recovery_completeness_contract --locked -- --nocapture
cargo test -p andromeda-storage --test property_recovery_replay --locked -- --nocapture
cargo test -p andromeda-storage --test wal_scan_recovery_contract --locked -- --nocapture
cargo test -p andromeda-storage --test file_wal_recovery_contract --locked -- --nocapture
cargo test -p andromeda-storage --test wal_durability_fence_contract --locked -- --nocapture
cargo test -p andromeda-exec --test recovery_visibility_gates --locked -- --nocapture
cargo test -p andromeda-transaction --test commit_log_durability --locked -- --nocapture
cargo test -p andromeda-transaction --test tx_wal_replay_recovery --locked -- --nocapture
```

## Evidence Rules

- One evidence record per command or manual decision.
- Short fuzz runs, continue-on-error Miri jobs, compile checks, and standalone
  Loom models are partial unless the release owner accepts the residual risk for
  the exact scope.
- Failed, skipped, partial, or missing gates cannot be recorded as pass.
- Artifact retention must be clear enough for a reviewer to recheck source
  state, toolchain, command, output, and disposition.

## References

- `docs/testing/testing-strategy.md`
- `docs/testing/release-evidence-template.md`
- `docs/testing/fuzz-miri-loom.md`
- `docs/governance/release-gates.md`
