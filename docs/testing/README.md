# Testing

This directory is the `/docs` home for testing strategy, release gates, evidence
templates, and deep-validation guidance.

## Documents

- `testing-strategy.md` describes risk-based test ownership and the command
  families used by Andromeda.
- `release-gates.md` summarizes the release gate chain from a testing
  perspective.
- `release-evidence-template.md` provides the evidence record shape for command
  runs, skipped gates, drills, and manual decisions.
- `fuzz-miri-loom.md` records fuzz, Miri, and Loom evidence requirements and
  points Loom checks at `tools/loom-models/`.
- `DEVELOPMENT_SPECS.md` is a historical development test inventory. Treat it
  as planning context, not release proof.

## Test Ownership

- Executable Rust tests stay with the crate that owns the behavior, usually
  under `crates/*/tests` or crate-local test modules.
- The root `tests/README.md` is a roadmap index for cross-crate labels.
- Fuzz target and corpus indexes stay under `tests/fuzzing/` and `fuzz/`.
- Standalone Loom models live under `tools/loom-models/`.

## Baseline Commands

```powershell
cargo fmt --all --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo nextest run --profile ci --workspace --all-features --locked
cargo test --doc --workspace --all-features --locked
```

## Rules

- Do not move crate-owned tests into the root `tests/` tree to satisfy a label.
- Do not treat fuzz, Miri, Loom, benchmark, RAM, GPU, or temporary output as a
  substitute for crash/recovery, audit, authorization, or durable visibility.
- Every release claim needs retained evidence from the exact source state.
