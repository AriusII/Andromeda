# andromeda-bench-workload

## Purpose

`andromeda-bench-workload` is the future owner crate for bounded benchmark workload definitions, workload shape versions, input limits, and diagnostic workload metadata.

This directory is a scaffold only. It is not registered as a Cargo workspace member, and current benchmark behavior remains in `andromeda-bench`.

## Scope

This crate is expected to own:

- Workload identities, shape versions, parameters, limits, and stop rules.
- Hardware profile requirements and reproducibility descriptors.
- Advisory evidence boundaries for workload output.
- Compatibility metadata used by benchmark harness, regression, and ScenarioEvidence crates.
- DecisionTrace input descriptors when benchmark evidence is later considered by adaptive systems.

## Non-goals

- Do not execute benchmark workloads from this crate.
- Do not treat benchmark output as storage, catalog, WAL, recovery, optimizer, or security truth.
- Do not let benchmark output select plans or publish statistics by itself.
- Do not put benchmark execution, analytics, or GPU work in C5 commit, WAL, rollback, recovery, MVCC short-visibility, catalog publication, or security-critical paths.
- Do not create unbounded workloads or uncontrolled temporary storage growth.

## Prerequisites

- Keep current workload behavior in `andromeda-bench` until a registered extraction work order moves it.
- Version every workload shape before comparing results.
- Define stop rules before adding new workload families.

## Procedure

1. Define workload identity and shape version first.
2. Add explicit limits for duration, samples, warmup, input size, and temporary bytes.
3. Bind workload output to hardware profile and measurement mode.
4. Mark all output advisory and non-authoritative.
5. Preserve current benchmark CLI compatibility until callers migrate.

## Validation

Future behavior changes should use:

```powershell
cargo test -p andromeda-bench-workload
cargo test -p andromeda-bench --test scenario_evidence_boundary -- --nocapture
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
```

This scaffold was designed for documentation review only.

## Troubleshooting

- If two workload results have different shape versions, reject direct comparison.
- If a workload has no stop rule, do not add it.
- If output is consumed as truth, route it through advisory ScenarioEvidence validation.

## References

- [Workspace crate rules](../README.md)
- [Current benchmark owner](../andromeda-bench/README.md)
- [Hardware policy owner](../andromeda-hardware/README.md)
