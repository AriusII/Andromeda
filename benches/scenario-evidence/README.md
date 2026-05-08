# ScenarioEvidence Benchmark Placeholder

## Purpose

This directory is a roadmap placeholder for ScenarioEvidence benchmark records. It does not contain generated benchmark output or release truth.

ScenarioEvidence records describe why a benchmark scenario exists, what it measures, how it was produced, and which correctness gates remain authoritative.

## Scope

Use this directory for future metadata records that describe benchmark scenarios, workload realism, performance budgets, and evidence quality.

ScenarioEvidence may support optimizer, statistics, maps, storage, RPC, and execution performance decisions only when the evidence is bounded, versioned, explainable, and disableable.

## Non-goals

- Do not store large generated benchmark artifacts here.
- Do not treat ScenarioEvidence as durable database truth.
- Do not use ScenarioEvidence to bypass crash/recovery, security, typed Procedure, WAL, or protocol validation.
- Do not accept learned, GPU, or benchmark-driven decisions in C5 critical paths.

## Prerequisites

- A scenario name and owning subsystem.
- A hypothesis and metric.
- Dataset or workload generation notes.
- Hardware profile, toolchain version, and command.
- Correctness gate, release gate, and residual-risk notes.

## Procedure

1. Define the scenario and the decision it is allowed to inform.
2. Record workload shape, data generation, metric, budget, confidence limits, and command.
3. Link the correctness, recovery, security, or protocol gate that remains authoritative.
4. State whether the evidence is accepted, rejected, provisional, or obsolete.
5. Keep generated outputs out of this directory unless a future work order defines retention and review rules.

## Acceptance Criteria

- Each future record names the scenario, owning subsystem, hypothesis, metric, workload, hardware profile, command, and evidence status.
- Each record links to the correctness gate that remains authoritative.
- Each record states confidence limits, known bias, and residual risk.
- Evidence can be disabled or ignored without changing database correctness.
- No record promotes benchmark, GPU, learned, RAM, or temporary output to C5 truth.

## Validation

No ScenarioEvidence record format exists yet. Until a future work order defines one, validate this placeholder with:

```powershell
git diff --check -- benches/scenario-evidence/README.md benches/README.md tests/README.md
```

When records are added, validation must include schema or lint checks for required fields and links to the authoritative correctness gates.

## Troubleshooting

If evidence lacks a hardware profile, command, confidence limit, or authoritative correctness gate, mark it provisional or reject it.

If evidence is used to justify a correctness, durability, recovery, security, or protocol change, require the owning release gate before accepting the decision.

## References

- `benches/README.md`
- `tests/README.md`
- `docs/codex/rust-critical-quality-gates.md`
- `documentations/testing/step-11-validation-matrix.md`
