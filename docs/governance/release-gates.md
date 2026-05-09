# Release Gates

## Purpose

Define the governance gates required before Andromeda claims release readiness
for C4/C5 behavior. A listed command is not a pass claim; release readiness
requires retained evidence from the exact release candidate.

## Required Evidence

Every gate record must include:

- branch and full commit SHA;
- `git status --short` for the validated source state;
- `rustc -Vv`, `cargo -V`, and gate-specific tool versions;
- exact command and working directory;
- `Pass`, `Fail`, `Skipped`, or `Partial`;
- retained artifact path;
- residual risk and reviewer disposition.

## Stop-Ship Gates

| Gate | Requirement | Blocks release when |
| --- | --- | --- |
| Reproducible source | Clean candidate, recorded branch, recorded commit, locked dependencies. | Source state is dirty or command output cannot be tied to a commit. |
| Workspace quality | Format, check, clippy, nextest, and doctests pass under the claimed toolchain. | Any command fails, is skipped, or runs with narrower scope than the release claim. |
| Supply chain | Advisory, license, source, duplicate, and MSRV policy checks pass or have accepted current risk. | `cargo audit`, `cargo deny`, or dependency topology evidence is missing or unresolved. |
| Durable truth | WAL, storage, transaction, recovery, backup/PITR, and visibility gates pass for the claimed scope. | Commit visibility, replay, restore, or corruption handling lacks retained evidence. |
| Security and protocol | Admission fails closed, typed Procedure contracts hold, RPC/QUIC frames are stable, and audit is durable. | Authorization, ContractHash rejection, protocol, or audit evidence is partial or absent. |
| Deep validation | Fuzz, Miri, and Loom evidence exists when byte, unsafe, memory-sensitive, or concurrency-sensitive surfaces are in scope. | Only compile checks, short smoke runs, retries, or timing-dependent tests exist. |
| Evidence packet | One record exists for every command, skipped gate, manual decision, and residual risk. | Any release-significant action lacks retained artifacts or owner disposition. |

## Baseline Commands

Run from the repository root on the clean candidate unless a release packet
records a different approved environment.

```powershell
cargo fmt --all --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo nextest run --profile ci --workspace --all-features --locked
cargo test --doc --workspace --all-features --locked
cargo audit --deny warnings
cargo deny check --all-features
```

## C5 Durable Truth Commands

Use the applicable subset and record gaps explicitly.

```powershell
cargo test -p andromeda-wal --tests --locked
cargo test -p andromeda-storage --test crash_recovery_impl --locked -- --nocapture
cargo test -p andromeda-storage --test recovery_completeness_contract --locked -- --nocapture
cargo test -p andromeda-storage --test property_recovery_replay --locked -- --nocapture
cargo test -p andromeda-storage --test wal_scan_recovery_contract --locked -- --nocapture
cargo test -p andromeda-storage --test file_wal_recovery_contract --locked -- --nocapture
cargo test -p andromeda-storage --test wal_durability_fence_contract --locked -- --nocapture
cargo test -p andromeda-storage --test disk_manager_durability_crash_safety --locked -- --nocapture
cargo test -p andromeda-exec --test recovery_visibility_gates --locked -- --nocapture
cargo test -p andromeda-exec --test c5_commit_rollback_lifecycle --locked -- --nocapture
cargo test -p andromeda-transaction --test commit_log_durability --locked -- --nocapture
cargo test -p andromeda-transaction --test tx_wal_replay_recovery --locked -- --nocapture
```

## Security, Protocol, Backup, And HA/DR

```powershell
cargo test -p andromeda-rpc-protocol --tests --locked
cargo test -p andromeda-quic --test protocol_stability_contract --locked -- --nocapture
cargo test -p andromeda-quic --test protobuf_projection_contract --locked -- --nocapture
cargo test -p andromeda-quic --test procedure_gateway_route --locked -- --nocapture
cargo test -p andromeda-quic --test zero_rtt_admission_policy --locked -- --nocapture
cargo test -p andromeda-security-contract --lib --locked -- --nocapture
cargo test -p andromeda-exec --test iam_pipeline_e2e --locked -- --nocapture
cargo test -p andromeda-exec --test exec_audit_completion_validation --locked -- --nocapture
cargo test -p andromeda-storage --test backup_physical_plan_contract --locked -- --nocapture
cargo test -p andromeda-storage --test backup_execution_plan --locked -- --nocapture
cargo test -p andromeda-storage --test restore_contract --locked -- --nocapture
cargo test -p andromeda-storage --test hadr_promotion_runtime_contract --locked -- --nocapture
cargo test -p andromeda-storage --test hadr_membership_store_contract --locked -- --nocapture
cargo test -p andromeda-storage --test quorum_membership_contract --locked -- --nocapture
cargo test -p andromeda-storage --test wal_shipping_reclaimability_contract --locked -- --nocapture
cargo test -p andromeda-quic --test hadr_stream_mapping_contract --locked -- --nocapture
```

## Deep Validation

Fuzz evidence must name target, corpus, duration, toolchain, crash artifacts,
and residual risk. Miri evidence must name the unsafe, aliasing, layout, FFI, or
buffer invariant. Loom evidence must name modeled state, thread or task bound,
safety property, command, and unmodeled behavior.

Use `tools/loom-models/` for the standalone Loom model:

```powershell
cargo test --manifest-path tools/loom-models/Cargo.toml --locked
```

## References

- `docs/governance/supply-chain-policy.md`
- `docs/governance/risk-register.md`
- `docs/testing/release-gates.md`
- `docs/testing/release-evidence-template.md`
- `docs/testing/fuzz-miri-loom.md`
- `tests/README.md`
- `tools/loom-models/README.md`
