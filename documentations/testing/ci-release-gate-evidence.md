# CI Release Gate Evidence

## Purpose

Define the release-review evidence expected from CI and supplemental release validation gates.

This document describes what must be recorded for workspace, supply-chain, fuzz, Miri, Loom, and crash/recovery gates. It does not change workflow behavior and does not claim that any gate has passed.

## Scope

This document covers evidence requirements for:

- `cargo fmt`
- `cargo check`
- `cargo clippy`
- `cargo nextest`
- documentation tests
- `cargo audit`
- `cargo deny`
- fuzz gates
- Miri gates
- Loom gates
- crash/recovery gates
- local release evidence metadata capture

Use this document with `release-evidence-template.md` and `step-11-validation-matrix.md` when preparing a release packet.

## Non-goals

- Do not modify `.github/workflows/**` from this document.
- Do not approve release readiness from a listed command alone.
- Do not claim that a gate passed without a retained artifact from the exact release candidate.
- Do not treat short fuzz smoke, continue-on-error Miri, compile checks, RAM state, benchmark output, GPU output, or temporary output as durable C5 proof.
- Do not replace crash/recovery, audit, authorization, or protocol evidence with a manual note.

## Prerequisites

Before a gate can count as release evidence, capture:

- the release candidate branch and full commit SHA;
- `git status --short` for the source state under validation;
- the workflow name, job name, run ID, run attempt, runner image, and run URL when the evidence comes from GitHub Actions;
- the exact command as executed;
- `rustc -Vv` and `cargo -V`;
- gate-specific tool versions, such as `cargo nextest --version`, `cargo audit --version`, `cargo deny --version`, `cargo fuzz --version`, and Miri nightly details;
- the pass, fail, skipped, or partial result;
- the retained artifact path;
- residual risk and reviewer or release-owner disposition.

Run supplemental local commands from the repository root unless a workflow log proves the command was executed from another explicit path.

## Procedure

1. Select the release candidate commit.
2. Run or collect the relevant CI jobs.
3. Add supplemental command evidence for any expected release gate that is not covered by the current workflow command.
4. Create one evidence record per command or manual decision by using `release-evidence-template.md`.
5. Mark every missing, failed, skipped, continue-on-error, smoke-only, or narrower-than-required gate as `Partial`, `Skipped`, `Fail`, or `Blocked`.
6. Attach the retained artifacts before changing any release status.

## Expected Gate Matrix

The expected release evidence command is the command that should appear in the release packet. The workflow coverage note identifies current CI coverage observed in repository workflows; it is not a pass claim. `release-gate-chain.yml` pins release-evidence jobs to Rust `1.95.0`; nightly fuzz smoke remains fuzz-specific preflight evidence.

| Gate | Expected release evidence command | Current workflow coverage note | Required evidence artifact | Acceptance rule |
| --- | --- | --- | --- | --- |
| Format | `cargo fmt --all --check` or workflow-equivalent `cargo fmt --all -- --check` | `release-gate-chain.yml` runs `cargo fmt --all -- --check` in `format`. | Command transcript, toolchain versions, workflow run metadata, and changed-file diff status if the command fails. | Exit code `0`; no formatting changes required. |
| Workspace check | `cargo check --workspace --all-targets --all-features --locked` | `release-gate-chain.yml` runs the exact release command in `workspace-check`. The narrower `preflight-check` job still runs `cargo check --workspace --locked`; treat it as preflight, not release evidence. | Command transcript, `Cargo.lock` identity, toolchain versions, and full target/feature scope. | Exit code `0` for all workspace targets and all features under the locked dependency graph. |
| Clippy | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | `release-gate-chain.yml` runs the exact release command in `clippy`. | Command transcript, lint output, toolchain versions, and any explicit allow-list decision. | Exit code `0`; no warnings under `-D warnings`; no unexplained allow-list expansion. |
| Nextest | `cargo nextest run --profile ci --workspace --all-features --locked` | `release-gate-chain.yml` installs `cargo-nextest`, records its version, and runs the exact release command in `nextest-ci`. `.config/nextest.toml` sets `retries = 0` for `default`, `ci`, `slow`, and `recovery`. | Nextest output, optional JUnit or archive output when configured, tool version, and test profile details. | Exit code `0`; skipped or flaky tests are recorded as residual risk and are not retried by default. |
| Documentation tests | `cargo test --doc --workspace --all-features --locked` | `release-gate-chain.yml` runs the exact release command in `doc-tests`. | Command transcript, doctest output, toolchain versions, and crate list. | Exit code `0`; failed or ignored doctests are recorded. |
| Cargo audit | `cargo audit --deny warnings` | `05-supply-chain.yml` and `release-gate-chain.yml` run `cargo audit --deny warnings`. | Audit report, advisory database timestamp when available, tool version, and accepted-risk references for any exception. | Exit code `0`; unreviewed warnings or advisories block release. |
| Cargo deny | `cargo deny check --all-features` | `05-supply-chain.yml` and `release-gate-chain.yml` use `cargo-deny-action` with `command: check` and `arguments: --all-features`. | Deny report, `deny.toml` identity, tool/action version, and exception rationale for any policy deviation. | Exit code `0`; license, source, advisory, or duplicate-policy failures require owner disposition. |
| Dependency topology preflight | `python -B tools/testing/supply_chain_preflight.py --json` | Local read-only tooling reports `workspace_topology_guard`, including workspace membership and direct GPU/SIMD exclusions for durable C5 crates. | JSON output, source-state metadata, and owner disposition for any nonzero `workspace_topology_guard.finding_count`. | `workspace_topology_guard.status` is `clean`, or every finding is fixed or explicitly scoped out before release readiness is claimed. |
| Fuzz smoke | `python fuzz/generators/generate_seed_corpus.py --ensure-only`, then `cargo +nightly fuzz run <target> -- -max_total_time=<seconds>` for each target discovered from `tests/fuzzing/targets.toml` | `07-fuzzing.yml` defaults to 15 seconds per target. `release-gate-chain.yml` also runs a 15-second fuzz smoke over manifest targets. | Target list, corpus manifest, seed generation log, per-target logs, `fuzz/artifacts`, `tests/fuzzing/corpus`, duration, nightly toolchain, and `cargo-fuzz` version. | Smoke evidence is preflight only. A release promotion needs sustained target-specific evidence and residual-risk review. |
| Miri | Broad preflight: `cargo +nightly miri setup` and `cargo +nightly miri test --workspace --all-features`. Bounded target: `cargo +nightly miri test -p <owning-crate> <target-name> --all-features`. | `06-nightly-deep-validation.yml` records a targeted subset inventory when `tools/testing/miri_subset.py` is present, runs the broad Miri command as `continue-on-error: true`, and uploads `nightly-miri-advisory-non-release`. Continue-on-error output and advisory artifacts do not count as a blocking release pass. | Miri setup log, command transcript, nightly toolchain, target selection rationale, unsupported-operation notes, and unsafe or aliasing invariant under test. | Exit code `0` in a blocking or release-reviewed run; unsupported targets require explicit residual risk. |
| Loom | `cargo test --manifest-path tests/loom/Cargo.toml`; owner-crate shape: `cargo test -p <owning-crate> --features loom <target-name> --locked -- --nocapture` | `06-nightly-deep-validation.yml` runs the standalone command as `continue-on-error: true` when `tests/loom/Cargo.toml` is present, otherwise it records the missing harness in `nightly-loom-advisory-non-release`. `tests/loom/README.md` defines a minimal standalone WAL durable-before-visible publication model. Treat owner-crate coverage as still required for production release claims. | Model command, feature flags, modeled state, thread/task bounds, explored safety property, log output, and residual risk. | Required when a release claim depends on concurrency interleavings. The standalone model and advisory artifact are partial evidence only; absence of owner-crate or integration evidence remains a blocker unless scope excludes the concurrency-sensitive surface. |
| Crash/recovery | Use the combined crash/recovery command set in this document and `step-11-validation-matrix.md`. | `15-crash-recovery-placeholder.yml` runs a small replay gate. `release-gate-chain.yml` adds selected recovery, backup, restore, and forensic commands. Treat isolated owner suites as partial for end-to-end durable visibility claims. | Command transcripts, scenario ID, crash point, durable state assertion, recovery report, WAL or manifest fixture details when retained, commit/branch, and residual risk. | Release evidence must prove durable WAL before visible commit, recovery replay, post-recovery visibility, and rejected corrupt or incomplete tails for the claimed scope. |
| Local release evidence metadata | `python -B tools/testing/release_evidence.py --json` | The generator is local and read-only. It collects metadata and declared check records; it does not run release gates. | JSON output, source-state metadata, toolchain metadata, declared check records, and residual-risk entries. | Use as packet metadata only. A generated JSON file is not a pass unless each referenced command has retained execution evidence. |

## Combined Crash/Recovery Gate

Use the following command set before claiming durable Procedure execution, WAL durability, storage replay, transaction recovery, or application-visible recovery readiness:

```powershell
cargo test -p andromeda-wal --tests --locked
cargo test -p andromeda-exec --test c5_combined_release_gate --locked -- --nocapture
cargo test -p andromeda-storage --test crash_recovery_impl --locked -- --nocapture
cargo test -p andromeda-storage --test recovery_completeness_contract --locked -- --nocapture
cargo test -p andromeda-storage --test property_recovery_replay --locked -- --nocapture
cargo test -p andromeda-storage --test wal_scan_recovery_contract --locked -- --nocapture
cargo test -p andromeda-storage --test file_wal_recovery_contract --locked -- --nocapture
cargo test -p andromeda-storage --test wal_durability_fence_contract --locked -- --nocapture
cargo test -p andromeda-storage --test disk_manager_durability_crash_safety --locked -- --nocapture
cargo test -p andromeda-storage --test btree_durable_promotion_contract --locked -- --nocapture
cargo test -p andromeda-exec --test recovery_visibility_gates --locked -- --nocapture
cargo test -p andromeda-exec --test c5_commit_rollback_lifecycle --locked -- --nocapture
cargo test -p andromeda-tx --test commit_log_durability --locked -- --nocapture
cargo test -p andromeda-tx --test tx_wal_replay_recovery --locked -- --nocapture
```

When backup, restore, PITR, ForensicStart, HA/DR, manifest publication, catalog publication, or map publication is in scope, add the matching scenario commands from `step-11-validation-matrix.md`.

For Map publication scope, the current owner-suite command is:

```powershell
cargo test -p andromeda-maps --test map_publication_contract --locked -- --nocapture
```

## Evidence Artifacts

Each release gate should retain a directory or CI artifact with:

| Artifact | Required contents |
| --- | --- |
| `evidence-record.md` | Evidence ID, gate ID, command, date, toolchain, commit/branch, result, artifact path, residual risk, owner, and follow-up reference. |
| `source-state.txt` | Full commit SHA, branch, `git status --short`, and workflow checkout ref. |
| `toolchain.txt` | `rustc -Vv`, `cargo -V`, and gate-specific tool versions. |
| `command.log` | Complete stdout and stderr for the command. |
| `workflow.json` or `workflow.txt` | Workflow name, job name, run ID, run attempt, runner image, conclusion, and URL when the command ran in CI. |
| `result-summary.md` | `Pass`, `Fail`, `Skipped`, or `Partial`, with the release effect and residual risk. |
| `release_evidence.json` | Optional output from `python -B tools/testing/release_evidence.py --json`; use it as metadata and declared checks only. |
| Gate-specific output | Nextest archives, audit reports, deny reports, fuzz corpora and crash artifacts, Miri logs, Loom model logs, recovery reports, WAL fixtures, or manifest evidence as applicable. |

The artifact path can be repository-relative or absolute. If it points outside the repository or GitHub Actions artifact storage, record the retention owner and retention period.

## Validation

Validate this documentation by checking it against:

- root project invariants in `AGENTS.md`;
- the current workflow commands in `.github/workflows/**`;
- `release-evidence-template.md`;
- `step-11-validation-matrix.md`;
- `supply-chain-policy.md`;
- `tests/miri/README.md`;
- `tests/loom/README.md`;
- `tests/loom/Cargo.toml`;
- `tests/fuzzing/targets.toml`.

This documentation-only validation does not run Rust gates and does not prove release readiness.

## Troubleshooting

If a workflow command is narrower than the expected release evidence command, keep the workflow evidence but mark it `Partial` until supplemental evidence is attached.

If a gate runs with `continue-on-error`, record the output as advisory unless the release owner explicitly reviews and accepts it as non-blocking for the release scope.

If a workflow uploads an artifact with `advisory`, `inventory`, or `non-release` in its purpose or name, use it to identify missing evidence only. Do not record that artifact as a release pass.

If a fuzz target only ran for the default 15-second smoke duration, keep the artifact as preflight evidence and record sustained fuzz evidence as missing.

If a crash/recovery owner suite passes in isolation, do not infer end-to-end durable visibility. Attach the combined gate or mark the release claim partial.

If `python -B tools/testing/supply_chain_preflight.py --json` reports missing `cargo-nextest`, `cargo-audit`, or `cargo-vet`, keep the preflight result as report-only tooling visibility. Missing `cargo-nextest` leaves the local nextest workspace gate and archive/profile evidence unavailable. Missing `cargo-audit` leaves local RustSec advisory evidence unavailable. Missing `cargo-vet` leaves local vet attestation evidence unavailable when vet governance applies. Use retained CI output, rerun locally after installing the tool, or record the gap as residual risk; do not treat the report-only finding as a release pass or as a default-mode script failure.

If `cargo audit` or `cargo deny` reports a warning, advisory, license issue, source issue, or duplicate-policy concern, classify it before release approval. Do not weaken policy to make the gate pass.

## References

- `AGENTS.md`
- `.github/workflows/release-gate-chain.yml`
- `.github/workflows/05-supply-chain.yml`
- `.github/workflows/06-nightly-deep-validation.yml`
- `.github/workflows/07-fuzzing.yml`
- `.github/workflows/15-crash-recovery-placeholder.yml`
- `documentations/testing/release-evidence-template.md`
- `documentations/testing/step-11-validation-matrix.md`
- `documentations/testing/fuzz-miri-loom-evidence.md`
- `documentations/testing/miri-subset-2026-05-08.md`
- `documentations/governance/release-readiness-gates-2026-05-08.md`
- `documentations/governance/supply-chain-policy.md`
- `docs/codex/rust-critical-quality-gates.md`
- `docs/codex/mission-critical-change-policy.md`
- `tests/fuzzing/targets.toml`
- `fuzz/VALIDATION_MATRIX.md`
- `tests/miri/README.md`
- `tests/loom/README.md`
- `tests/loom/Cargo.toml`
- `tools/testing/miri_subset.py`
- `tools/testing/release_evidence.py`
