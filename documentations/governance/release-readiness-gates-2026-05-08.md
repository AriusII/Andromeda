# Release Readiness Gates - 2026-05-08

## Purpose

Define the current release-readiness gates for Andromeda as of 2026-05-08.

These gates convert the current blocker inventory into executable release
criteria. Passing this document review does not approve a release. A release
requires retained evidence from a clean candidate and a current-dated approval
artifact.

## Scope

These gates apply to mission-critical Andromeda release claims that touch or
depend on:

- WAL durability and visible commit;
- storage, page, manifest, backup, restore, PITR, and HA/DR behavior;
- transaction recovery and MVCC visibility;
- catalog publication and DefinitionBatch behavior;
- typed Procedure contracts and ContractHash rejection;
- QUIC/RPC framing, admission, authorization, and ResultStream behavior;
- durable audit and observability evidence;
- Rust dependency, MSRV, and supply-chain policy.

## Non-goals

- Do not approve release readiness from this document alone.
- Do not modify DEC-035 or DEC-036.
- Do not use future-dated decisions as current release proof.
- Do not define new runtime behavior, wire formats, persistent formats, or SRPL
  semantics.
- Do not use GPU, benchmark, RAM, temporary, or advisory output as durable truth.
- Do not replace crash/recovery, fuzz, security, or audit evidence with manual
  notes.

## Prerequisites

Before any gate can pass, the release owner must capture:

- branch name and commit SHA;
- clean `git status --short` output;
- `rustc -Vv` and `cargo -V`;
- platform and linker availability for local Rust test runs;
- exact command lines;
- pass, fail, skipped, or partial result;
- retained artifact paths;
- residual risk and owner review.

The current observed workspace does not satisfy these prerequisites because the
worktree is dirty, plain PowerShell does not expose Windows `link.exe`, and
current release evidence has not been recorded for the required crash/recovery
and fuzz gates. Local Rust gates can run from the configured Visual Studio
developer command environment, but those advisory runs do not replace a clean
release-candidate evidence packet.

## Gate Status Model

| Status | Meaning |
| --- | --- |
| `Pass` | The exact gate ran on the release candidate, passed, and has retained evidence. |
| `Fail` | The gate ran and failed. Release is blocked unless the release owner explicitly rejects the release scope. |
| `Blocked` | The gate cannot run or cannot be accepted because a prerequisite is missing. |
| `Partial` | Some evidence exists, but it is incomplete, advisory, isolated, or lacks retained artifacts. |
| `Skipped` | The gate was intentionally not run. Skipped mission-critical gates block release unless the affected scope is removed from the release claim. |

## Release Disposition

| Field | Value |
| --- | --- |
| Disposition date | 2026-05-08 |
| Overall release readiness | `Blocked` |
| Blocking reason | Dirty worktree, incomplete retained release evidence, plain-shell Windows linker preflight blocked, missing combined crash/recovery evidence, missing sustained fuzz evidence, missing Miri release evidence, and future-dated decisions that cannot serve as current proof. |
| Current release proof allowed | No |
| Next executable step | Create a clean release candidate and rerun the required gates with retained artifacts. |

## Stop-Ship Gates

| Gate ID | Gate | Current status | Requirement to pass | Release blocker |
| --- | --- | --- | --- | --- |
| `REL-2026-05-08-G01` | Clean reproducible source | `Blocked` | `git status --short` is clean for the exact release candidate, and the branch plus full commit SHA are recorded. | Yes |
| `REL-2026-05-08-G02` | Current release authority | `Blocked` | The release packet includes current-dated approval evidence. DEC-035, DEC-036, and DEC-037 are not used as current proof while dated after 2026-05-08. | Yes |
| `REL-2026-05-08-G03` | Rust MSRV and dependency compatibility | `Partial` | Rust 1.95.0 runs the workspace with `--locked`; supply-chain checks pass or have accepted current-dated risk dispositions with retained artifacts. | Yes until retained release evidence is complete |
| `REL-2026-05-08-G04` | Windows validation host readiness | `Partial` for local Windows validation | `link.exe` is visible on the Windows host that runs Rust build and test commands, or retained CI evidence from a configured Windows host is attached. | Yes for Windows claims until the developer-shell or CI artifact is retained |
| `REL-2026-05-08-G05` | C5 durable truth and recovery | `Blocked` | Combined WAL, storage, tx, exec, visibility, backup/restore, and applicable HA/DR crash/recovery gates pass with retained artifacts. | Yes |
| `REL-2026-05-08-G06` | Fuzz and deep validation | `Blocked` | Sustained fuzz evidence exists for affected byte, parser, protocol, ResultStream, and security admission surfaces; Miri or Loom evidence is attached when memory-sensitive or concurrency-sensitive code is in scope. | Yes for affected surfaces |
| `REL-2026-05-08-G07` | Security, protocol, and audit | `Blocked` | Admission fails closed before transaction creation, typed Procedure contracts are enforced, ContractHash mismatches are rejected, RPC/QUIC framing is validated, and durable audit covers accepted and rejected paths. | Yes |
| `REL-2026-05-08-G08` | Evidence packet completeness | `Blocked` | Every command and manual decision has an evidence record with command, date, toolchain, commit/branch, pass/fail, artifact path, residual risk, and owner review. | Yes |

## Required Command Gates

Run commands from the repository root on the clean release candidate.

### Workspace and supply chain

```powershell
cargo fmt --all --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo nextest run --workspace --all-features --locked
cargo test --doc --workspace --locked
cargo audit --deny warnings
cargo deny check --all-features
```

### Durable WAL, storage, recovery, and visibility

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
cargo test -p andromeda-tx --test commit_log_durability --locked -- --nocapture
cargo test -p andromeda-tx --test tx_wal_replay_recovery --locked -- --nocapture
```

### Protocol, security, and audit

```powershell
cargo test -p andromeda-rpc-protocol --tests --locked
cargo test -p andromeda-quic --test protocol_stability_contract --locked -- --nocapture
cargo test -p andromeda-quic --test protobuf_projection_contract --locked -- --nocapture
cargo test -p andromeda-quic --test procedure_gateway_route --locked -- --nocapture
cargo test -p andromeda-quic --test zero_rtt_admission_policy --locked -- --nocapture
cargo test -p andromeda-security-contract --lib --locked -- --nocapture
cargo test -p andromeda-exec --test iam_pipeline_e2e --locked -- --nocapture
cargo test -p andromeda-exec --test iam_hardening --locked -- --nocapture
cargo test -p andromeda-exec --test permission_scope_contract --locked -- --nocapture
cargo test -p andromeda-exec --test exec_audit_completion_validation --locked -- --nocapture
```

### Backup, restore, PITR, and HA/DR

```powershell
cargo test -p andromeda-storage --test backup_physical_plan_contract --locked -- --nocapture
cargo test -p andromeda-storage --test backup_execution_plan --locked -- --nocapture
cargo test -p andromeda-storage --test restore_contract --locked -- --nocapture
cargo test -p andromeda-storage --locked forensic_start -- --nocapture
cargo test -p andromeda-storage --test hadr_promotion_runtime_contract --locked -- --nocapture
cargo test -p andromeda-storage --test hadr_membership_store_contract --locked -- --nocapture
cargo test -p andromeda-storage --test quorum_membership_contract --locked -- --nocapture
cargo test -p andromeda-storage --test wal_shipping_reclaimability_contract --locked -- --nocapture
cargo test -p andromeda-quic --test hadr_stream_mapping_contract --locked -- --nocapture
```

### Fuzz, Miri, and concurrency-sensitive evidence

Use the targets listed in `fuzz/targets.toml` and `fuzz/VALIDATION_MATRIX.md`.
For each affected surface, the release packet must record duration, corpus, seed,
toolchain, commit, command, pass/fail result, artifact path, and residual risk.

Compile checks and short CI smoke runs are useful preflight checks, but they are
not release proof for byte, parser, protocol, or security admission promotion.

If unsafe, FFI, SIMD, lock-free, or concurrency-sensitive behavior is in scope,
attach a targeted Miri or Loom evidence record, or mark the gate blocked for that
surface.

## Procedure

1. Select the release candidate branch and commit.
2. Confirm `git status --short` is clean.
3. Confirm Rust 1.95.0 is active and the Windows linker is available if local
   Windows evidence is required.
4. Run the workspace and supply-chain gates.
5. Run the durable WAL, storage, recovery, visibility, protocol, security, audit,
   backup/PITR, HA/DR, fuzz, Miri, and Loom gates that apply to the release
   scope.
6. Create one evidence record per command or manual decision by using
   `documentations/testing/release-evidence-template.md`.
7. Mark any missing, failed, partial, skipped, or advisory-only gate as a
   release blocker unless the release scope explicitly excludes the affected
   surface.
8. Require release-owner review before changing overall disposition from
   `Blocked`.

## Validation

This document was prepared as a documentation-only release governance artifact.
It validates the checklist shape and blocker inventory, not executable release
readiness.

To validate a release, attach retained evidence for:

- clean source status;
- Rust 1.95.0 and dependency compatibility;
- platform linker readiness where local Windows validation is claimed;
- workspace build, clippy, tests, and documentation tests;
- supply-chain scans;
- combined crash/recovery and durable visibility gates;
- protocol, security, authorization, and audit gates;
- backup/PITR and HA/DR gates where in scope;
- sustained fuzz campaigns and targeted Miri or Loom gates where in scope;
- current-dated release approval.

## Troubleshooting

If the workspace is dirty, stop release approval and produce a clean candidate.

If `cargo check --locked` fails because the lockfile is stale or a dependency
requires a newer compiler, resolve the lockfile or dependency admission before
release approval.

If Windows tests fail because `link.exe` is missing, run from the configured
Visual Studio developer command environment or install and expose Visual Studio
Build Tools, then rerun the failed commands and retain artifacts.

If a crash/recovery command exists but was not run as part of a combined gate,
classify the result as partial. Isolated owner tests do not prove end-to-end
visible commit or recovery readiness.

If a fuzz target only compiled or ran as a short smoke test, classify it as
preflight evidence. Sustained fuzz evidence remains required before promotion.

If DEC-035, DEC-036, or DEC-037 are cited as release proof for 2026-05-08, reject
that proof until a current-dated approval artifact exists.

## References

- `AGENTS.md`
- `Cargo.toml`
- `rust-toolchain.toml`
- `documentations/governance/risk-register-2026-05-08.md`
- `documentations/testing/step-11-validation-matrix.md`
- `documentations/testing/release-evidence-template.md`
- `documentations/governance/supply-chain-policy.md`
- `fuzz/VALIDATION_MATRIX.md`
- `fuzz/targets.toml`
- `documentations/governance/decisions/DEC-035-release-gate-chain.md`
- `documentations/governance/decisions/DEC-036-release-readiness-approval.md`
- `documentations/governance/decisions/DEC-037-risk-register-updates.md`
