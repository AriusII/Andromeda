# Benchmarking

**Status:** Current operational runbook
**Updated:** 2026-05-08
**Owner:** Benchmark Evidence
**Surface:** Administration diagnostics only

## Purpose

This document describes the current `andromeda-bench` and `andromeda-cli benchmark` implementation. It records what benchmark evidence means today, how to run the available workloads, and which claims are intentionally out of scope.

Benchmark output is diagnostic evidence. It is not durable engine truth, not an optimizer mandate, and not a runtime mutation path.

## Scope

This runbook covers:

- The benchmark CLI surface in `crates/andromeda-cli/src/benchmark`.
- The benchmark evidence model in `crates/andromeda-bench`.
- The current workload registry and CRUD scenario registry.
- Budget evaluation based on p50 latency, p95 latency, and error-rate evidence.
- Advisory-only `BenchmarkScenarioEvidence` boundaries.
- Known limitations around hardware profiles, GPU, CI, and future crate splits.

## Non-goals

This runbook does not claim:

- Application-facing ad hoc SQL support.
- A production database benchmark suite.
- Runtime optimizer authority from benchmark output.
- GPU benchmark execution.
- Network throughput benchmarking for QUIC sockets.
- Automatic CI regression gating unless a caller wires it externally.
- Final crate ownership after the planned workspace split.

## Current Implementation

`andromeda-bench` is a bounded diagnostics crate. It defines workload metadata, request validation, synthetic and harness-based latency collection, evidence records, budget evaluation, regression comparison helpers, history records, and advisory ScenarioEvidence export shapes.

The current benchmark runner is `run_bounded_benchmark(request: &BenchmarkRunRequest)`. It:

1. Validates the workload id, duration, samples, warmups, temp budget, and hardware profile.
2. Dispatches the workload to either a synthetic diagnostic model or a local harness.
3. Computes `p50_latency_us` and `p95_latency_us`.
4. Sets `error_count` to `0` for successful standard benchmark runs.
5. Evaluates the static workload budget against p50, p95, and error-rate ppm.
6. Returns `BenchmarkEvidence` with `diagnostic_only: true`, `authoritative: false`, `can_select_plan_alone: false`, and `optimizer_boundary: "advisory-only"`.

The standard evidence contract is intentionally smaller than older documentation claimed. It does not contain mean latency, p99 latency, p999 latency, min/max latency, standard deviation, or throughput for standard `benchmark run` evidence.

## Evidence Contract

Standard benchmark evidence is represented by `BenchmarkEvidence`.

Required evidence fields currently emitted by the CLI contract are:

| Field | Meaning |
| --- | --- |
| `workload_id` | Registered workload identifier. |
| `workload_hypothesis` | The workload question under test. |
| `workload_shape_version` | Versioned shape of the synthetic or harness scenario. |
| `workload_size` | Bounded input size and shape description. |
| `primary_metric` | Current metric contract. Standard workloads use `p50_latency_us,p95_latency_us,error_rate_ppm`. |
| `baseline_ref` | Logical baseline-history reference. |
| `budget_origin` | Static budget source, currently `static-workload-registry-v1`. |
| `decision_linkage` | Advisory optimizer linkage requirement. |
| `hardware_profile` | Requested CLI profile, `conservative` or `declared-local`. |
| `duration_ms` | Requested duration cap. |
| `samples` | Requested sample cap. |
| `warmups` | Requested warmup cap. |
| `temp_budget_bytes` | Requested temporary byte budget. |
| `started_at_unix_ms` | Placeholder start timestamp for standard benchmark runs. Current runner returns `0`. |
| `elapsed_ms` | Deterministic bounded elapsed placeholder derived from request shape. |
| `sample_count` | Sample count used for statistics. |
| `p50_latency_us` | Median latency in microseconds. |
| `p95_latency_us` | 95th percentile latency in microseconds. |
| `error_count` | Count of observed workload errors. Current standard runner returns `0` after successful dispatch. |
| `budget_status` | `passed` or `failed`. |
| `diagnostic_only` | Always `true` for benchmark CLI evidence. |
| `measurement_mode` | `synthetic-diagnostic` or `harness-diagnostic`. |
| `latency_source` | Synthetic model or harness source. |
| `engine_harness` | Harness name for harness diagnostics, otherwise `null`. |
| `synthetic_model_version` | Synthetic model version for synthetic diagnostics, otherwise `null`. |
| `authoritative` | Always `false`. |
| `can_select_plan_alone` | Always `false`. |
| `optimizer_boundary` | Always `advisory-only`. |

### Budget Logic

Standard workload budgets are represented by `PerformanceBudget`:

```text
max_p50_latency_us
max_p95_latency_us
max_error_rate_ppm
```

`evaluate_budget()` fails a run if any of these conditions is true:

```text
p50_latency_us > max_p50_latency_us
p95_latency_us > max_p95_latency_us
error_rate_ppm(error_count, sample_count) > max_error_rate_ppm
```

The runner rejects empty statistics and invalid error counts:

```text
sample_count == 0                  -> InsufficientSamplesForStatistics
error_count > sample_count          -> ErrorCountExceedsSamples
```

### Regression Evidence

Regression comparison is available through `RegressionAnalysis`. It compares current evidence to a `BenchmarkBaseline` using:

- Current and baseline p50 latency.
- Current and baseline p95 latency.
- Current and baseline error counts.
- Current and baseline sample counts.
- Derived current and baseline error-rate ppm.

The current regression helper flags latency degradation above the configured threshold and flags error-rate increases. It is a library helper; the repository does not currently provide a complete always-on CI workflow that archives benchmark evidence and gates every push.

## ScenarioEvidence Boundary

Benchmark-derived ScenarioEvidence is advisory only.

`BenchmarkScenarioEvidence` can be built from a `BenchmarkEvidence` value or a benchmark history record, but it must carry explicit target and validity context:

- `ProcedureId`
- `CatalogVersion`
- `ContractHash`
- `StatsVersion`
- `BenchmarkPlanClass`
- Confidence
- Validity window
- Bounded duration, sample, and temp budgets
- Provenance context such as hardware profile, measurement mode, latency source, timing source, engine harness, or synthetic model version

The advisory boundary is enforced by stable flags:

```text
is_authoritative()        -> false
can_select_plan_alone()   -> false
optimizer_consumption_role() -> "advisory-only"
```

Benchmark evidence may help an optimizer integration layer explain or compare choices, but it cannot select a plan by itself and cannot replace catalog statistics, Procedure contracts, a valid plan-cache key, or a DecisionTrace.

## Workload Registry

Standard workloads are registered in `crates/andromeda-bench/src/workload.rs`.

| Workload id | Class | Scope | Max duration ms | Max samples | Budget p50 us | Budget p95 us | Error budget ppm |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| `vertical-v0-smoke` | Synthetic diagnostic | Vertical V0 invocation plus WAL recovery accounting. | 10000 | 30 | 50000 | 150000 | 0 |
| `protocol-smoke-contract` | Synthetic diagnostic | Protocol contract inspection without network sockets. | 5000 | 20 | 10000 | 50000 | 0 |
| `wal-append-smoke` | Synthetic diagnostic | Synthetic WAL append accounting without file IO. | 10000 | 30 | 20000 | 75000 | 0 |
| `btree-lookup-smoke` | Harness diagnostic | Mock B-Tree single-key lookup over a read-only key set. | 5000 | 20 | 50 | 500 | 0 |
| `btree-range-scan-smoke` | Harness diagnostic | Mock B-Tree range scan over a bounded read-only key subset. | 5000 | 20 | 500 | 5000 | 0 |
| `btree-node-codec-smoke` | Harness diagnostic | B-Tree durable node V1 encode/decode over page images. | 5000 | 20 | 2500 | 10000 | 0 |
| `storage-page-store-smoke` | Harness diagnostic | DiskPageStore and BufferPool page write, flush, and readback. | 10000 | 20 | 5000000 | 10000000 | 0 |
| `wal-append-file-smoke` | Harness diagnostic | File-backed WAL append and durable flush. | 10000 | 20 | 5000000 | 10000000 | 0 |
| `recovery-replay-wal-smoke` | Harness diagnostic | File-backed WAL scan and recovery replay planning. | 10000 | 20 | 5000000 | 10000000 | 0 |
| `audit-append-file-sink-smoke` | Harness diagnostic | File-backed durable audit sink append and replay. | 10000 | 20 | 5000000 | 10000000 | 0 |
| `srpl-compile-optimize-smoke` | Harness diagnostic | SRPL parse, lower, and optimize compiler pipeline. | 5000 | 20 | 1000000 | 5000000 | 0 |

### Workload Classes

`BenchmarkWorkloadClass` currently has three variants:

| Class | Current use |
| --- | --- |
| `synthetic-diagnostic` | Registered and executable through the standard runner. |
| `harness-diagnostic` | Registered and executable through local harnesses. |
| `real-runtime` | Defined in the type system but not currently used by registered workloads. |

Do not document a workload as `real-runtime` unless the registered workload actually uses `BenchmarkWorkloadClass::RealRuntime`.

## CRUD Scenario Registry

CRUD diagnostics are exposed through separate CLI subcommands and are not part of `WORKLOADS`.

Use:

```bash
andromeda-cli benchmark crud-scenarios
andromeda-cli benchmark crud <scenario> [--seed <u64>] [--diagnostic-json]
```

Current CRUD scenarios are registered in `crates/andromeda-bench/src/crud/scenario.rs`.

| Scenario id | Threads | Batch size | Rows | Insert | Update | Delete | Scan | Max duration ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `crud-single-1` | 1 | 1 | 10000 | 25% | 25% | 25% | 25% | 30000 |
| `crud-single-100` | 1 | 100 | 10000 | 25% | 25% | 25% | 25% | 30000 |
| `crud-multi4-10` | 4 | 10 | 100000 | 25% | 25% | 25% | 25% | 30000 |
| `crud-multi8-100` | 8 | 100 | 100000 | 25% | 25% | 25% | 25% | 30000 |
| `crud-scan-1m` | 1 | 1000000 | 1000000 | 10% | 10% | 10% | 70% | 60000 |
| `crud-write-heavy` | 4 | 10 | 50000 | 40% | 40% | 20% | 0% | 30000 |

CRUD output includes operation metrics such as `p50_us`, `p95_us`, `p99_us`, `throughput_ops_sec`, and `error_count`. That richer CRUD result shape should not be confused with the standard `BenchmarkEvidence` shape emitted by `benchmark run`.

## Prerequisites

- Build the workspace before relying on benchmark output:

```bash
cargo check --workspace --all-targets --all-features
```

- Use the CLI binary or `cargo run` from the repository root.
- Treat output as local diagnostics unless a separate release or CI process records the exact commit, hardware context, and baseline.

## Procedure

### List Standard Workloads

```bash
andromeda-cli benchmark workloads
andromeda-cli benchmark workloads --diagnostic-json
```

Equivalent through Cargo:

```bash
cargo run -p andromeda-cli -- benchmark workloads
```

### Display the Benchmark Contract

```bash
andromeda-cli benchmark contract
andromeda-cli benchmark contract --diagnostic-json
```

The contract output reports global limits, default limits, conservative hardware materialization, advisory optimizer flags, and the required evidence field list.

### Run a Standard Workload

```bash
andromeda-cli benchmark run <workload> \
  [--duration-ms <ms>] \
  [--samples <n>] \
  [--warmups <n>] \
  [--temp-budget-bytes <bytes>] \
  [--hardware-profile conservative|declared-local] \
  [--diagnostic-json]
```

Example:

```bash
cargo run -p andromeda-cli -- benchmark run btree-node-codec-smoke --samples 2 --warmups 0 --diagnostic-json
```

### Run a CRUD Scenario

```bash
cargo run -p andromeda-cli -- benchmark crud crud-single-1 --seed 42 --diagnostic-json
```

## Configuration

### Global Limits

| Limit | Value |
| --- | ---: |
| `MAX_DURATION_MS` | 60000 |
| `MAX_SAMPLES` | 100 |
| `MAX_WARMUPS` | 10 |
| `MAX_TEMP_BYTES` | 67108864 |
| `MAX_EVIDENCE_TTL_MS` | 604800000 |

### Defaults

| Default | Value |
| --- | ---: |
| `DEFAULT_DURATION_MS` | 5000 |
| `DEFAULT_SAMPLES` | 10 |
| `DEFAULT_WARMUPS` | 1 |
| `DEFAULT_TEMP_BYTES` | 8388608 |

### Hardware Profiles

The CLI accepts two hardware profile names:

| CLI profile | Current materialization | Notes |
| --- | --- | --- |
| `conservative` | `HardwareProfile::conservative()` | Unknown architecture, one hardware thread, no SIMD, no direct IO, GPU disabled. |
| `declared-local` | `HardwareProfile::conservative()` | Accepted as an operator declaration, but currently materializes to the same conservative profile. |

Current hardware profile handling is intentionally limited. It is not real hardware discovery, does not scale budgets, and does not enable GPU execution. The conservative profile has `gpu.available == false` and `GpuExecutionPolicy::Disabled`.

## Validation

For this document or other documentation-only benchmark changes, run at least:

```bash
cargo test -p andromeda-bench --all-targets
cargo test -p andromeda-cli --test benchmark_cli_commands --all-features
```

For changes that alter benchmark code, evidence semantics, ScenarioEvidence export, or regression logic, add or run targeted tests for:

- Request validation bounds.
- Workload registry uniqueness and metadata consistency.
- Budget pass/fail behavior for p50, p95, and error-rate ppm.
- Harness evidence fields and measurement mode.
- Diagnostic JSON schema strings.
- ScenarioEvidence target validation, expiry, and advisory-only flags.
- Regression comparison with p50, p95, and error-rate evidence.

Before release-readiness claims, also run the broader Rust gates selected for the affected crates:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Benchmark output alone is never sufficient validation for commit visibility, WAL durability, recovery, MVCC visibility, catalog publication, security policy, or optimizer correctness.

## Troubleshooting

| Symptom | Likely cause | Action |
| --- | --- | --- |
| `unknown benchmark workload` | The id is not in `WORKLOADS`. | Run `andromeda-cli benchmark workloads`. |
| `unknown CRUD scenario` | The id is not in `CRUD_SCENARIOS`. | Run `andromeda-cli benchmark crud-scenarios`. |
| `--json` is rejected | Benchmark commands require explicit diagnostic output naming. | Use `--diagnostic-json`. |
| Duration, sample, warmup, or temp-budget validation fails | Request exceeds global or workload-specific caps. | Check `benchmark contract` and `benchmark workloads`. |
| Budget fails on p50 or p95 | Current latency exceeds static workload budget. | Preserve the evidence, compare with an accepted baseline, and inspect the harness or code path before changing budgets. |
| Budget fails on error rate | Error-rate ppm exceeds the workload budget. | Treat as a correctness or harness failure first; do not mask it by raising latency budgets. |
| `declared-local` does not change observed budget behavior | The current implementation materializes it to the conservative profile. | Do not document budget scaling until code implements it. |
| GPU is unavailable | The conservative hardware profile disables GPU. | Do not add GPU benchmark claims until a real off-critical-path GPU runtime and tests exist. |

## Governance Notes

- Benchmark output is advisory evidence only.
- ScenarioEvidence must remain bounded, expirable, target-validated, and non-authoritative.
- Benchmark evidence cannot force a plan and cannot be used as optimizer truth.
- Benchmark crates are R5 tools/evidence crates. Production crates must not depend upward on them.
- Several future crate splits remain pending. Current documentation should name existing crates and avoid implying that planned benchmark split crates already exist.
- Persistent engine truth still comes from durable storage, WAL, catalog contracts, validated statistics, and recovery evidence, not from benchmark or GPU output.

## References

- `crates/andromeda-bench/src/lib.rs`
- `crates/andromeda-bench/src/workload.rs`
- `crates/andromeda-bench/src/evidence.rs`
- `crates/andromeda-bench/src/evidence/model.rs`
- `crates/andromeda-bench/src/budget.rs`
- `crates/andromeda-bench/src/request.rs`
- `crates/andromeda-bench/src/runner.rs`
- `crates/andromeda-bench/src/scenario_boundary/evidence.rs`
- `crates/andromeda-bench/src/regression_detection/comparison.rs`
- `crates/andromeda-bench/src/crud/scenario.rs`
- `crates/andromeda-cli/src/benchmark`
- `crates/andromeda-hardware/src/integration.rs`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `documentations/architecture/WORKSPACE_RESTRUCTURE_BASELINE_2026.md`
