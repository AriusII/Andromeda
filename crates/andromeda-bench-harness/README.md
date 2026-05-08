# andromeda-bench-harness

## Purpose

`andromeda-bench-harness` owns reusable bounded benchmark harness helpers, timer accounting, and temporary resource isolation.

Engine-specific smoke benchmark execution remains in `andromeda-bench` and uses this crate for shared harness support.

## Scope

This crate owns:

- Timer helpers and temporary file or directory isolation for bounded harnesses.
- Isolation between benchmark execution and production runtime decisions.

## Non-goals

- Do not treat benchmark measurements as authoritative truth.
- Do not run benchmarks in commit, WAL, rollback, recovery, MVCC short-visibility, catalog publication, or security-critical paths.
- Do not let benchmark output select plans, publish statistics, or bypass Procedure contracts by itself.
- Do not require GPU availability for a benchmark to produce valid diagnostic output.
- Do not persist native Rust layouts as benchmark evidence.

## Prerequisites

- Keep engine-specific benchmark behavior in `andromeda-bench`.
- Require explicit workload identity and shape version for every run.
- Require bounded duration, sample count, warmup count, and temporary byte limits.

## Procedure

1. Validate workload identity and run limits before execution.
2. Capture hardware profile and measurement mode.
3. Produce diagnostic output marked advisory and non-authoritative.
4. Convert run output to ScenarioEvidence only through explicit validity and version checks.
5. Preserve current CLI compatibility until callers migrate.

## Validation

Behavior changes should use:

```powershell
cargo check -p andromeda-bench-harness --tests
cargo check -p andromeda-bench --tests
```

## Troubleshooting

- If a run has no workload identity, reject the output.
- If a benchmark exceeds limits, lower run parameters instead of raising global caps without review.
- If a result affects adaptive behavior, require ScenarioEvidence validation and DecisionTrace.

## References

- [Workspace crate rules](../README.md)
- [Current benchmark owner](../andromeda-bench/README.md)
- [Hardware policy owner](../andromeda-hardware/README.md)
