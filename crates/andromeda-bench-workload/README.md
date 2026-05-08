# andromeda-bench-workload

## Purpose

`andromeda-bench-workload` owns bounded benchmark workload definitions, workload shape versions, input limits, run request validation, and diagnostic workload metadata.

`andromeda-bench` uses this crate as the workload contract source. Benchmark execution remains outside this crate.

## Scope

This crate owns:

- Workload identities, shape versions, parameters, limits, and stop rules.
- Hardware profile requirements and reproducibility descriptors.
- Budget evaluation for advisory workload output.
- Compatibility metadata used by benchmark harness, regression, and ScenarioEvidence crates.
- DecisionTrace input descriptors when benchmark evidence is later considered by adaptive systems.

## Non-goals

- Do not execute benchmark workloads from this crate.
- Do not treat benchmark output as storage, catalog, WAL, recovery, optimizer, or security truth.
- Do not let benchmark output select plans or publish statistics by itself.
- Do not put benchmark execution, analytics, or GPU work in C5 commit, WAL, rollback, recovery, MVCC short-visibility, catalog publication, or security-critical paths.
- Do not create unbounded workloads or uncontrolled temporary storage growth.

## Prerequisites

- Use this crate as the source of workload identity, limits, and request validation.
- Version every workload shape before comparing results.
- Define stop rules before adding new workload families.

## Procedure

1. Define workload identity and shape version first.
2. Add explicit limits for duration, samples, warmup, input size, and temporary bytes.
3. Bind workload output to hardware profile and measurement mode.
4. Mark all output advisory and non-authoritative.
5. Preserve current benchmark CLI compatibility until callers migrate.

## Validation

Behavior changes should use:

```powershell
cargo check -p andromeda-bench-workload --tests
cargo check -p andromeda-bench --tests
```

## Troubleshooting

- If two workload results have different shape versions, reject direct comparison.
- If a workload has no stop rule, do not add it.
- If output is consumed as truth, route it through advisory ScenarioEvidence validation.

## References

- [Workspace crate rules](../README.md)
- [Current benchmark owner](../andromeda-bench/README.md)
- [Hardware policy owner](../andromeda-hardware/README.md)
