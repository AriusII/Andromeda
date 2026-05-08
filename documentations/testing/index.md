# Testing Documentation Index

## Purpose

Provide the entry point for Andromeda testing, CI release gate, and release evidence documentation.

This index helps release owners find the expected commands, evidence records, fuzz and deep-validation requirements, and Step 11 validation mapping without changing executable workflows or test ownership.

## Scope

This index covers documentation in `documentations/testing`:

- CI release gate evidence requirements;
- release evidence record shape;
- local release evidence generator schema;
- local dependency topology preflight evidence;
- bounded Miri subset inventory;
- Step 11 owner-suite and crash/recovery mapping;
- Step 12 spec-to-test mapping;
- fuzz, Miri, and Loom evidence requirements;
- unsafe, lock-free, sanitizer, and Miri inventory expectations.

Executable tests remain in the crates that own the behavior. GitHub Actions workflows remain under `.github/workflows/**`.

## Non-goals

- Do not move tests into `documentations/testing`.
- Do not modify workflows from this index.
- Do not approve a release from this index alone.
- Do not claim that any gate passed unless a retained evidence artifact proves the exact command, commit, toolchain, and result.
- Do not weaken typed Procedure contracts, WAL-before-visible-commit, IAM, audit, recovery, or RPC boundary requirements.

## Prerequisites

Before using these documents for a release packet:

- select the exact release candidate branch and commit;
- capture `git status --short`;
- capture `rustc -Vv`, `cargo -V`, and gate-specific tool versions;
- identify the affected subsystem and risk class;
- identify whether the release claim touches C4/C5 behavior;
- retain artifacts for every command, workflow job, manual decision, skipped gate, or residual risk.

## Documentation Set

| Document | Use it for | Notes |
| --- | --- | --- |
| [CI Release Gate Evidence](ci-release-gate-evidence.md) | Expected CI and supplemental release evidence for `cargo fmt`, `cargo check`, `cargo clippy`, `cargo nextest`, doc tests, `cargo audit`, `cargo deny`, fuzz, Miri, Loom, and crash/recovery gates. | Defines evidence expectations only. It does not change workflows and does not claim pass status. |
| [Release Evidence Template](release-evidence-template.md) | One evidence record per command, crash/recovery drill, or manual release decision. | Use this template for retained release packet entries. |
| [Release Evidence Schema](../../tools/testing/release_evidence_schema.md) | JSON schema for the local release evidence generator. | The generator captures metadata and declared check results only; it does not run gates or claim readiness. |
| [Miri Subset - 2026-05-08](miri-subset-2026-05-08.md) | Bounded Miri subset commands and exclusions for lightweight owner-crate checks. | Use `tools/testing/miri_subset.py` to print or run the subset; dry-run output is inventory, not release evidence. |
| [Step 11 Validation Matrix](step-11-validation-matrix.md) | Mapping Step 11 roadmap labels to crate-owned suites, C5 combined gates, crash/recovery scenarios, and release gaps. | Treat isolated owner-suite passes as partial when a release claim needs end-to-end durable visibility. |
| [Specification Validation Matrix - 2026-05-08](spec-validation-matrix-2026-05-08.md) | Mapping v0 specifications to owner-suite validation areas and residual release gaps. | Treat this as mapping evidence only; it does not prove that any listed command passed. |
| [Fuzz, Miri, and Loom Evidence](fuzz-miri-loom-evidence.md) | Deep-validation evidence for fuzz targets, Miri targets, and Loom models. | Fuzz, Miri, and Loom evidence is subordinate to crash/recovery and durable visibility for C5 claims. |
| [Unsafe, Lock-Free, and Miri Inventory](unsafe-miri-inventory-2026-05-08.md) | Inventory and evidence requirements for unsafe Rust, lock-free or atomic code, Miri checks, and sanitizer checks. | Inventory output is review evidence only. It does not approve unsafe code, lock-free algorithms, or C4/C5 behavior. |

## Procedure

1. Start with `ci-release-gate-evidence.md` to identify the required workspace, supply-chain, dependency topology, fuzz, Miri, Loom, and crash/recovery gate evidence.
2. Use `step-11-validation-matrix.md` to map the affected roadmap label to crate-owned commands and crash/recovery scenarios.
3. Use `spec-validation-matrix-2026-05-08.md` to map the affected v0 specification to owner-suite validation areas and release gaps.
4. Use `fuzz-miri-loom-evidence.md` when the release claim touches malformed input, explicit byte formats, unsafe or aliasing-sensitive code, or concurrency interleavings.
5. Use `unsafe-miri-inventory-2026-05-08.md` when a change or release claim touches unsafe Rust, atomics, lock-free behavior, sanitizer evidence, or Miri evidence.
6. Record each command or manual decision with `release-evidence-template.md`.
7. Use `tools/testing/miri_subset.py` to list or run the bounded Miri subset when memory-sensitive crates are in scope.
8. Use `python -B tools/testing/supply_chain_preflight.py --json` to capture dependency topology preflight output, including `andromeda-protocol` workspace membership and C5 GPU/SIMD direct dependency exclusions.
9. Use `tools/testing/release_evidence.py` only to capture local metadata and declared check records; do not treat its output as release approval.
10. Mark missing, failed, skipped, partial, smoke-only, or continue-on-error evidence as residual risk.
11. Require release-owner review before changing any readiness disposition.

The supply-chain preflight is report-only unless strict mode is selected. Missing `cargo-nextest`, `cargo-audit`, or `cargo-vet` in that report produces a `findings` status and means the matching local evidence is unavailable; it does not prove the gate failed, and it does not approve release readiness. Attach CI evidence, rerun with the tool installed, or record the missing tool as residual risk with owner disposition.

## Validation

For documentation-only updates in this directory, validate by checking:

- root project invariants in `AGENTS.md`;
- Microsoft Learn-style structure;
- consistency with `.github/workflows/**` when workflow commands are referenced;
- consistency with `docs/codex/rust-critical-quality-gates.md`;
- consistency with `docs/codex/mission-critical-change-policy.md`;
- consistency with `tools/testing/release_evidence_schema.md` when local generator output is referenced;
- consistency with `tools/testing/supply_chain_preflight.py` when dependency topology output is referenced;
- consistency with `tools/testing/miri_subset.py` when bounded Miri subset commands are referenced;
- consistency with `tools/testing/unsafe_inventory.py` when unsafe inventory
  guidance is referenced;
- no pass claims without retained evidence.

For release validation, use the commands and evidence requirements in
`ci-release-gate-evidence.md`, `step-11-validation-matrix.md`,
`spec-validation-matrix-2026-05-08.md`, `fuzz-miri-loom-evidence.md`, and
`unsafe-miri-inventory-2026-05-08.md`.

## Troubleshooting

If a document lists a command that is not present in the current CI workflow, treat the document as release evidence guidance and attach supplemental evidence. Do not edit workflows unless a separate work order owns that change.

If a release claim depends on crash/recovery but only isolated owner tests are available, record the result as partial and link the missing combined drill.

If fuzz, Miri, or Loom evidence is missing for an affected C4/C5 surface, record the release claim as blocked or out of scope for that surface.

If an artifact cannot be retained, do not use the command as release proof. Rerun the gate with retained output or record the missing artifact as residual risk.

## References

- `AGENTS.md`
- `.github/workflows/release-gate-chain.yml`
- `.github/workflows/05-supply-chain.yml`
- `.github/workflows/06-nightly-deep-validation.yml`
- `.github/workflows/07-fuzzing.yml`
- `.github/workflows/15-crash-recovery-placeholder.yml`
- `docs/codex/rust-critical-quality-gates.md`
- `docs/codex/mission-critical-change-policy.md`
- `tools/testing/release_evidence.py`
- `tools/testing/release_evidence_schema.md`
- `tools/testing/supply_chain_preflight.py`
- `tools/testing/miri_subset.py`
- `tools/testing/unsafe_inventory.py`
- `tests/fuzzing/targets.toml`
- `fuzz/VALIDATION_MATRIX.md`
- `tests/README.md`
- `tests/miri/README.md`
- `tests/loom/README.md`
- `tests/loom/Cargo.toml`
- `documentations/testing/spec-validation-matrix-2026-05-08.md`
