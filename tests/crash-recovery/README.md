# Crash/Recovery Test Index

## Purpose

This directory is a roadmap index for deterministic crash and recovery validation. It does not own executable Rust tests.

Crash/recovery truth remains in the crate-owned suites that validate WAL durability, manifest publication, storage replay, transaction visibility, catalog consistency, map consistency, and executor recovery behavior.

## Scope

Use this index for work orders that ask for root-level crash/recovery coverage or C5 recovery evidence.

| Area | Owning validation location |
| --- | --- |
| WAL scan, segment, checksum, and replay | `crates/andromeda-wal/tests/*` |
| Storage crash replay and page records | `crates/andromeda-storage/tests/crash_recovery_impl.rs`, `crates/andromeda-storage/tests/recovery_completeness_contract.rs`, `crates/andromeda-storage/tests/property_recovery_replay.rs`, `crates/andromeda-storage/tests/recovery_replay_page_records_contract.rs` |
| WAL durability fences and disk safety | `crates/andromeda-storage/tests/wal_durability_fence_contract.rs`, `crates/andromeda-storage/tests/disk_manager_durability_crash_safety.rs` |
| Transaction commit and WAL replay | `crates/andromeda-tx/tests/commit_log_durability.rs`, `crates/andromeda-tx/tests/tx_wal_replay_recovery.rs`, `crates/andromeda-tx/tests/storage_tx_wal_adapter_contract.rs` |
| Executor recovery visibility | `crates/andromeda-exec/tests/recovery_visibility_gates.rs` |

## Non-goals

- Do not create a root-level crash harness that bypasses crate ownership.
- Do not normalize flaky recovery tests through retries.
- Do not accept RAM, temporary files, fuzz output, or benchmark output as durable truth.
- Do not weaken WAL-before-visible-commit, typed Procedure, IAM, audit, or recovery invariants.

## Prerequisites

- Rust toolchain from `rust-toolchain.toml`.
- Workspace dependencies resolved with the root `Cargo.lock`.
- A deterministic crash point, durable evidence source, replay assertion, and post-recovery visibility assertion for each new scenario.

## Procedure

1. Identify the owning subsystem and crate for the recovery behavior.
2. Add or update the executable test in the owning crate, not in this root directory.
3. Record the crash point, durable state, replay expectation, visibility expectation, and release-blocker status.
4. Link the crate-owned suite from this index and from `tests/README.md` when the roadmap label changes.

## Acceptance Criteria

- Each scenario names the risk class, owning crate, crash point, durable evidence, replay expectation, and post-recovery visibility assertion.
- C5 scenarios prove that no commit becomes visible before durable WAL.
- Recovery assertions are deterministic and do not rely on timing retries.
- Corruption, partial-write, or missing-record cases state whether recovery must fail closed, replay, or produce a forensic report.
- Any new executable test remains at the crate boundary that owns the behavior.

## Validation

Recommended smoke gate:

```powershell
cargo test -p andromeda-wal --tests --locked
cargo test -p andromeda-storage --test crash_recovery_impl --locked -- --nocapture
cargo test -p andromeda-storage --test recovery_completeness_contract --locked -- --nocapture
cargo test -p andromeda-storage --test property_recovery_replay --locked -- --nocapture
cargo test -p andromeda-storage --test wal_durability_fence_contract --locked -- --nocapture
cargo test -p andromeda-exec --test recovery_visibility_gates --locked -- --nocapture
cargo test -p andromeda-tx --test commit_log_durability --locked -- --nocapture
```

## Troubleshooting

If a recovery test passes alone but fails in the combined gate, treat the combined failure as the release blocker. Recovery evidence must prove WAL durability, replay, visibility, and audit behavior together.

If a scenario needs nondeterministic timing to pass, move the design back to the owning subsystem and define a deterministic synchronization point.

## References

- `tests/README.md`
- `documentations/testing/step-11-validation-matrix.md`
- `docs/codex/mission-critical-change-policy.md`
- `docs/codex/rust-critical-quality-gates.md`
