# andromeda-bench

## Purpose

`andromeda-bench` owns bounded benchmark contracts, deterministic smoke workloads, performance budgets, regression comparison helpers, and benchmark evidence models for Andromeda operational diagnostics.

Benchmark evidence is advisory. It can support investigation, regression triage, and operator decisions, but it is not storage truth, catalog truth, optimizer authority, or a standalone plan-selection input.

## Scope

This crate provides:

- Bounded benchmark requests, limits, and workload registry entries.
- Deterministic smoke workloads for storage, WAL, B-tree, SRPL compiler, audit file sink, CRUD, and related diagnostic paths.
- Evidence records with workload identity, hardware profile, timing source, sample counts, budget status, and advisory optimizer boundary fields.
- Benchmark history and regression comparison helpers.
- Scenario evidence context, confidence, validity, budgets, and target descriptors.

## Non-goals

- Do not use benchmark output as authoritative truth for commit, WAL, recovery, catalog publication, MVCC visibility, or security decisions.
- Do not allow benchmark evidence to select an optimizer plan by itself.
- Do not add unbounded workloads, opaque timing sources, or uncontrolled temporary storage growth.
- Do not place GPU work or benchmark execution in commit, rollback, WAL, recovery, MVCC visibility, catalog publication, or security-critical paths.
- Do not expose this crate as a runtime wire protocol surface.

## Prerequisites

- Run benchmarks from a controlled workspace state.
- Select an explicit workload from the registry.
- Use conservative hardware profiles unless the local environment is intentionally declared.
- Keep duration, sample, warmup, and temporary byte limits within the crate-level caps.
- Preserve the evidence fields that identify workload shape, budget origin, decision linkage, and measurement mode.

## Procedure

1. Inspect the allowed workloads through `andromeda-cli benchmark workloads`.
2. Inspect the evidence contract through `andromeda-cli benchmark contract`.
3. Run a bounded workload through `andromeda-cli benchmark run <workload>`.
4. Export diagnostic JSON only when an operator workflow or test needs structured advisory evidence.
5. Compare results against compatible baselines only when the workload shape, hardware profile, budget origin, and evidence context match.

## Validation

For benchmark evidence boundary changes, use:

```powershell
cargo test -p andromeda-bench --test scenario_evidence_boundary -- --nocapture
cargo test -p andromeda-bench --test regression_detection -- --nocapture
cargo test -p andromeda-bench --test cross_commit_history -- --nocapture
```

For CLI exposure of benchmark contracts, use:

```powershell
cargo test -p andromeda-cli --test benchmark_cli_commands -- --nocapture
```

Before accepting source changes, use the broader workspace gates listed in `crates/README.md`.

## Troubleshooting

- If evidence is missing advisory fields, check that the workload path fills `diagnostic_only`, `authoritative`, `can_select_plan_alone`, and `optimizer_boundary`.
- If a comparison is rejected, verify that the baseline context matches the current workload identity, hardware profile, and shape version.
- If a run exceeds limits, reduce duration, samples, warmups, or temporary byte budget instead of raising global caps.
- If a result appears faster on a declared-local profile, treat it as local diagnostic evidence until reproduced under the required controlled profile.

## References

- [Workspace crate rules](../README.md)
- [`src/lib.rs`](src/lib.rs)
- [`src/evidence`](src/evidence)
- [`src/scenario_boundary`](src/scenario_boundary)
- [`src/regression_detection`](src/regression_detection)
