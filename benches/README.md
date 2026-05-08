# Benchmark Placeholder Index

## Purpose

This directory is a roadmap placeholder for benchmark governance. It does not define correctness, durability, recovery, or security truth.

Benchmarks are advisory evidence for performance work. They must be observable, bounded, versioned, explainable, and disableable before they can influence a release decision.

## Scope

Use this index for root-level benchmark planning, workload metadata, performance budgets, and links to crate-owned benchmark targets.

| Path | Purpose |
| --- | --- |
| `benches/scenario-evidence/` | Placeholder for ScenarioEvidence benchmark records and workload review criteria. |

Executable benchmarks should stay with the crate or harness that owns the measured behavior.

## Non-goals

- Do not treat benchmark output as C4/C5 truth.
- Do not place GPU work in commit, WAL, rollback, recovery, MVCC short-visibility, catalog publication, or security-critical paths.
- Do not use benchmarks to justify bypassing typed Procedure contracts, WAL durability, IAM, audit, or recovery validation.
- Do not store large generated result artifacts in this placeholder.

## Prerequisites

- A named scenario, owning subsystem, and hypothesis.
- A hardware profile and toolchain version.
- A correctness gate that remains authoritative.
- A repeatable command and clear disable path.

## Procedure

1. Identify the owning subsystem and benchmark scenario.
2. Define the metric, workload shape, input size, hardware profile, and confidence limits.
3. Keep executable benchmarks in the owning crate or established benchmark harness.
4. Record ScenarioEvidence metadata under `benches/scenario-evidence/` only when a future work order defines the record format.
5. Compare benchmark results only after correctness, recovery, and security gates remain passing.

## Acceptance Criteria

- Each benchmark entry names the scenario, owning subsystem, metric, workload, hardware profile, command, and disable path.
- The entry states the correctness gate that remains authoritative.
- Results include confidence limits or repetition policy.
- GPU or SIMD acceleration has a scalar fallback and stays outside C5 critical paths.
- Regressions and improvements are explainable through traces, counters, or documented workload evidence.

## Validation

No root benchmark command exists yet. Use the owning crate or harness command when a benchmark target is added.

Documentation-only validation for this placeholder:

```powershell
git diff --check -- benches/README.md benches/scenario-evidence/README.md tests/README.md tests/crash-recovery/README.md tests/fuzzing/README.md tests/loom/README.md tests/miri/README.md
```

## Troubleshooting

If a benchmark conflicts with a correctness, recovery, or security gate, the gate wins and the benchmark result is advisory only.

If results vary beyond the stated confidence limits, update the workload, hardware profile, or measurement method before making performance claims.

## References

- `benches/scenario-evidence/README.md`
- `tests/README.md`
- `docs/codex/rust-critical-quality-gates.md`
- `documentations/testing/step-11-validation-matrix.md`
