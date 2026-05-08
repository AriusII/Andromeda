# andromeda-bench-harness

## Purpose

`andromeda-bench-harness` is the future owner crate for bounded benchmark execution harnesses, measurement control, run envelopes, and diagnostic export policy.

This directory is a scaffold only. It is not registered as a Cargo workspace member, and current benchmark execution remains in `andromeda-bench`.

## Scope

This crate is expected to own:

- Harness run envelopes, measurement modes, timers, sample controls, and stop rules.
- Isolation between benchmark execution and production runtime decisions.
- Diagnostic export metadata for advisory results.
- Hardware profile capture and reproducibility warnings.
- Links to ScenarioEvidence and regression records after validation.

## Non-goals

- Do not treat benchmark measurements as authoritative truth.
- Do not run benchmarks in commit, WAL, rollback, recovery, MVCC short-visibility, catalog publication, or security-critical paths.
- Do not let benchmark output select plans, publish statistics, or bypass Procedure contracts by itself.
- Do not require GPU availability for a benchmark to produce valid diagnostic output.
- Do not persist native Rust layouts as benchmark evidence.

## Prerequisites

- Keep current harness behavior in `andromeda-bench` until a registered extraction work order moves it.
- Require explicit workload identity and shape version for every run.
- Require bounded duration, sample count, warmup count, and temporary byte limits.

## Procedure

1. Validate workload identity and run limits before execution.
2. Capture hardware profile and measurement mode.
3. Produce diagnostic output marked advisory and non-authoritative.
4. Convert run output to ScenarioEvidence only through explicit validity and version checks.
5. Preserve current CLI compatibility until callers migrate.

## Validation

Future behavior changes should use:

```powershell
cargo test -p andromeda-bench-harness
cargo test -p andromeda-bench --test regression_detection -- --nocapture
cargo test -p andromeda-cli --test benchmark_cli_commands -- --nocapture
```

This scaffold was designed for documentation review only.

## Troubleshooting

- If a run has no workload identity, reject the output.
- If a benchmark exceeds limits, lower run parameters instead of raising global caps without review.
- If a result affects adaptive behavior, require ScenarioEvidence validation and DecisionTrace.

## References

- [Workspace crate rules](../README.md)
- [Current benchmark owner](../andromeda-bench/README.md)
- [Hardware policy owner](../andromeda-hardware/README.md)
