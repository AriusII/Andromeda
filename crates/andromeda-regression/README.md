# andromeda-regression

## Purpose

`andromeda-regression` is the future owner crate for advisory regression comparison, baseline compatibility rules, threshold policy, and diagnostic regression reports.

This directory is a scaffold only. It is not registered as a Cargo workspace member, and current regression behavior remains in `andromeda-bench`.

## Scope

This crate is expected to own:

- Baseline identity and compatibility rules across workload shape, hardware profile, measurement mode, and versioned targets.
- Threshold policy for latency, throughput, allocation, temporary bytes, and error counts.
- Diagnostic reports that classify regressions without becoming engine truth.
- ScenarioEvidence and benchmark history comparison helpers after validation.
- DecisionTrace inputs when regression evidence affects adaptive review or operator explanation.

## Non-goals

- Do not treat regression output as storage, catalog, WAL, recovery, optimizer, or security truth.
- Do not compare incompatible benchmark shapes, hardware profiles, catalog versions, stats versions, policy versions, or plan classes.
- Do not let regression output select a plan, publish statistics, or bypass typed Procedure contracts by itself.
- Do not put regression analysis, benchmark execution, analytics, or GPU work in C5 commit, WAL, rollback, recovery, MVCC short-visibility, catalog publication, or security-critical paths.
- Do not persist reports through native Rust struct layout.

## Prerequisites

- Keep current regression behavior in `andromeda-bench` until a registered extraction work order moves it.
- Require compatible baselines before comparison.
- Treat every report as diagnostic and advisory unless a separate owner validates a version-bound action.

## Procedure

1. Validate baseline compatibility before comparison.
2. Apply explicit thresholds and stop rules.
3. Record whether evidence is accepted, rejected, stale, or incompatible.
4. Emit DecisionTrace when regression evidence affects adaptive review.
5. Preserve current benchmark compatibility paths until callers migrate.

## Validation

Future behavior changes should use:

```powershell
cargo test -p andromeda-regression
cargo test -p andromeda-bench --test regression_detection -- --nocapture
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
```

This scaffold was designed for documentation review only.

## Troubleshooting

- If two baselines are not compatible, reject comparison instead of normalizing away the mismatch.
- If regression output is treated as truth, route it through the owning runtime validation path.
- If a GPU-assisted comparison has no CPU fallback, keep that path disabled.

## References

- [Workspace crate rules](../README.md)
- [Current benchmark owner](../andromeda-bench/README.md)
- [Current observability owner](../andromeda-observe/README.md)
