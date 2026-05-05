# BENCHMARK — Workload Registry and Framework

**Version:** 1.0.0  
**Updated:** Q1 2026  
**Language:** American English  
**Style:** Microsoft Documentation  

---

## Overview

This document describes the Andromeda benchmark framework, workload catalog, runner configuration, performance budgets, and CI/regression gate integration. The benchmark system is designed to measure and validate performance of critical Procedures under controlled, repeatable conditions.

Benchmarks are diagnostic operations only. JSON output is produced for CI automation and evidence archival, never as a runtime mutation path.

---

## Framework Architecture

### Key Principles

1. **Isolation:** Each benchmark run executes in a fresh, bounded invocation context.
2. **Determinism:** Identical hardware profiles and parameters produce consistent results within statistical tolerance.
3. **Bounded execution:** All benchmarks have hard duration and sample limits to prevent runaway operations.
4. **Non-intrusive:** Benchmarks do not modify the transaction kernel, recovery engine, or catalog; they exercise existing Procedures.
5. **Evidence archival:** All runs produce timestamped evidence records for auditing and regression detection.

### Architecture Tiers

| Tier | Component | Purpose |
|------|-----------|---------|
| **Framework** | `run_bounded_benchmark()` | Entry point; validates request, spawns executor, collects evidence. |
| **Workload** | `BenchmarkWorkload` struct | Static workload definition (name, duration bounds, sample limits, budget). |
| **Runner** | `BenchmarkRunRequest` | User-supplied parameters (workload ID, duration, sample count). |
| **Executor** | Per-workload runner | Executes the Procedure invocation loop; measures latency and throughput. |
| **Evidence** | `BenchmarkEvidence` struct | Timestamped results (mean, p50, p99 latency, throughput, budget status). |
| **CI Gate** | `evaluate_budget()` | Compares evidence against performance budget; flags regression. |

---

## Workload Catalog

All workloads are statically defined in `crates/andromeda-bench/src/lib.rs`. Each workload:

- Has a globally unique identifier (`id: &str`).
- Declares hardware profile requirements and constraints.
- Specifies duration and sample count bounds.
- Defines a performance budget (latency p99, throughput minimum).
- Maps to an executable Procedure.

### Registered Workloads

#### `inventory-reserve-stock` — Transactional Reservation

**Purpose:** Measure latency and throughput of the `Inventory.ReserveStock` Procedure under normal load.

**Metadata:**

| Field | Value |
|-------|-------|
| **ID** | `inventory-reserve-stock` |
| **Procedure** | `Inventory.ReserveStock` |
| **Purpose** | Business procedure latency and throughput baseline. |
| **Default Duration** | 5 seconds |
| **Maximum Duration** | 60 seconds |
| **Default Samples** | 10 |
| **Maximum Samples** | 100 |
| **Hardware Profile** | Conservative (multicore, 8GB+ RAM) |

**Performance Budget:**

```
Latency P99:   <= 50 ms
Throughput:    >= 1000 invocations/second
```

**Invocation Parameters:**

```
stock_id:  "WIDGET-SKU-001" (constant across run)
quantity:  1 (reserved per invocation)
```

**Expected Output:**

```json
{
  "workload_id": "inventory-reserve-stock",
  "procedure": "Inventory.ReserveStock",
  "samples": 10,
  "mean_latency_ms": 15.3,
  "p50_latency_ms": 14.2,
  "p99_latency_ms": 42.8,
  "throughput_invocations_per_sec": 1250,
  "budget_status": "PASS"
}
```

**Constraints:**

- Executes against a persistent in-memory catalog.
- Does not reset database state between runs; results reflect steady-state performance.
- Throughput calculation: `total_invocations / elapsed_time_sec`.

#### `catalog-resolve-procedure` — Metadata Lookup

**Purpose:** Measure latency of the catalog resolver for Procedure metadata lookups.

**Metadata:**

| Field | Value |
|-------|-------|
| **ID** | `catalog-resolve-procedure` |
| **Purpose** | Catalog read-only performance baseline. |
| **Default Duration** | 2 seconds |
| **Maximum Duration** | 10 seconds |
| **Default Samples** | 20 |
| **Maximum Samples** | 100 |
| **Hardware Profile** | Conservative |

**Performance Budget:**

```
Latency P99:   <= 5 ms
Throughput:    >= 10000 lookups/second
```

**Expected Output:**

```json
{
  "workload_id": "catalog-resolve-procedure",
  "procedure": "System.CatalogResolveProcedure",
  "samples": 20,
  "mean_latency_ms": 0.8,
  "p50_latency_ms": 0.7,
  "p99_latency_ms": 4.2,
  "throughput_invocations_per_sec": 11500,
  "budget_status": "PASS"
}
```

#### `wal-recovery-replay` — Recovery Performance

**Purpose:** Measure WAL recovery throughput for the `wal-recovery-replay` phase.

**Metadata:**

| Field | Value |
|-------|-------|
| **ID** | `wal-recovery-replay` |
| **Purpose** | Recovery and durability verification. |
| **Default Duration** | 10 seconds |
| **Maximum Duration** | 60 seconds |
| **Default Samples** | 5 |
| **Maximum Samples** | 20 |
| **Hardware Profile** | Declared-Local (single-core, < 4GB RAM) |

**Performance Budget:**

```
Throughput:    >= 100 MB/second recovery rate
```

**Constraints:**

- Executes recovery from a pre-generated, known-good WAL segment.
- Does not simulate failures; tests happy-path recovery performance only.

---

## Runner Configuration

### Command Syntax

```bash
andromeda-cli benchmark run <workload-id> [--duration-ms <ms>] [--samples <n>] [--warmups <n>] [--hardware-profile <profile>] [--diagnostic-json]
```

### Parameters

| Parameter | Type | Default | Bounds | Description |
|-----------|------|---------|--------|-------------|
| `<workload-id>` | String | — | Must exist | Workload identifier from catalog. |
| `--duration-ms` | u64 | 5000 | 1–60000 | Run duration in milliseconds. |
| `--samples` | u32 | 10 | 1–100 | Number of latency samples to collect. |
| `--warmups` | u32 | 1 | 0–10 | Warm-up invocations before sampling. |
| `--hardware-profile` | String | Conservative | See below | Hardware class for scaling budgets. |
| `--diagnostic-json` | flag | false | — | Output results in JSON format. |

### Hardware Profiles

**Conservative (default)**

- Target: Multicore systems with 8GB+ RAM.
- Budget scaling: No adjustment (baseline budgets apply).
- Use: CI pipelines, regression testing, production monitoring.

**Declared-Local**

- Target: Single-core or resource-constrained environments.
- Budget scaling: Latency budgets +50%, throughput budgets -30%.
- Use: Developer local testing, embedded environments, low-power deployments.

**Example:**

```bash
# Conservative profile (default), 10 samples, 5 seconds
andromeda-cli benchmark run inventory-reserve-stock

# Declared-Local profile, 5 samples, 10 second warm-up
andromeda-cli benchmark run inventory-reserve-stock \
  --hardware-profile declared-local \
  --warmups 10 \
  --samples 5
```

---

## Evidence Format

All benchmark runs produce a `BenchmarkEvidence` structure that captures the complete result snapshot.

### Top-Level Evidence Contract

```json
{
  "_diagnostic": {
    "timestamp": "2026-01-15T14:30:45Z",
    "framework_version": "1.0.0",
    "source": "andromeda-cli benchmark"
  },
  "workload_id": "<workload-id>",
  "procedure": "<fully-qualified-procedure-name>",
  "run_config": {
    "duration_ms": 5000,
    "samples": 10,
    "warmups": 1,
    "hardware_profile": "Conservative"
  },
  "results": {
    "samples_collected": 10,
    "mean_latency_ms": <f64>,
    "p50_latency_ms": <f64>,
    "p95_latency_ms": <f64>,
    "p99_latency_ms": <f64>,
    "p999_latency_ms": <f64>,
    "min_latency_ms": <f64>,
    "max_latency_ms": <f64>,
    "stdev_latency_ms": <f64>,
    "throughput_invocations_per_sec": <f64>
  },
  "budget": {
    "latency_p99_threshold_ms": 50,
    "throughput_minimum_per_sec": 1000,
    "latency_status": "PASS" | "FAIL",
    "throughput_status": "PASS" | "FAIL",
    "overall_status": "PASS" | "FAIL"
  }
}
```

### Field Definitions

| Field | Type | Meaning |
|-------|------|---------|
| `timestamp` | ISO 8601 | UTC time of benchmark execution. |
| `framework_version` | String | Benchmark framework version. |
| `workload_id` | String | Workload identifier from catalog. |
| `procedure` | String | Fully qualified Procedure name (e.g., `Inventory.ReserveStock`). |
| `samples_collected` | u32 | Actual samples collected (may be less if timeout or error occurs). |
| `mean_latency_ms` | f64 | Mean invocation latency. |
| `p99_latency_ms` | f64 | 99th percentile latency. |
| `p50_latency_ms` | f64 | Median (50th percentile) latency. |
| `throughput_invocations_per_sec` | f64 | Invocations per second: `samples_collected / elapsed_seconds`. |
| `latency_p99_threshold_ms` | f64 | Budget threshold for p99 latency. |
| `throughput_minimum_per_sec` | f64 | Budget threshold for minimum throughput. |
| `latency_status` | String | "PASS" if p99 ≤ threshold, "FAIL" otherwise. |
| `throughput_status` | String | "PASS" if throughput ≥ threshold, "FAIL" otherwise. |
| `overall_status` | String | "PASS" if all metrics pass, "FAIL" if any metric fails. |

### Example Evidence

```json
{
  "_diagnostic": {
    "timestamp": "2026-01-15T14:30:45Z",
    "framework_version": "1.0.0",
    "source": "andromeda-cli benchmark"
  },
  "workload_id": "inventory-reserve-stock",
  "procedure": "Inventory.ReserveStock",
  "run_config": {
    "duration_ms": 5000,
    "samples": 10,
    "warmups": 1,
    "hardware_profile": "Conservative"
  },
  "results": {
    "samples_collected": 10,
    "mean_latency_ms": 15.3,
    "p50_latency_ms": 14.2,
    "p99_latency_ms": 42.8,
    "p999_latency_ms": 48.5,
    "min_latency_ms": 12.1,
    "max_latency_ms": 49.2,
    "stdev_latency_ms": 11.7,
    "throughput_invocations_per_sec": 2000
  },
  "budget": {
    "latency_p99_threshold_ms": 50,
    "throughput_minimum_per_sec": 1000,
    "latency_status": "PASS",
    "throughput_status": "PASS",
    "overall_status": "PASS"
  }
}
```

---

## Performance Budget Interpretation

### Budget Status Logic

The `evaluate_budget()` function compares evidence against configured thresholds:

```text
latency_status := if (p99_latency_ms <= threshold) then "PASS" else "FAIL"
throughput_status := if (throughput >= threshold) then "PASS" else "FAIL"
overall_status := if (latency_status == "PASS" AND throughput_status == "PASS") then "PASS" else "FAIL"
```

### Regression Detection

A regression is detected when:

1. **Latency increases:** p99 latency exceeds threshold (e.g., 50 ms → 65 ms).
2. **Throughput decreases:** Invocations per second falls below minimum (e.g., 1200 → 800).
3. **Hardware degradation:** Same workload on declared-local profile shows larger variance or repeated failures.

### Budget Adjustment Policy

Budgets are locked at workload definition time and updated only through:

1. **Formal decision record** (DEC-*): Documented justification, stakeholder approval, compatibility impact.
2. **Performance analysis:** Root cause of regression identified and fixed in the codebase.
3. **Hardware profile change:** Workload is explicitly moved to a different hardware profile.

Budgets are never dynamically adjusted based on single evidence runs.

---

## CI Integration and Regression Gates

### Benchmark CI Workflow

**Trigger:** On every push to the main branch.

**Steps:**

1. Build all crates with optimizations (`cargo build --release`).
2. Run all registered workloads with `--diagnostic-json` output.
3. Parse JSON evidence and compare `overall_status` against previous baseline.
4. If any workload shows `overall_status: FAIL`, fail the CI job.
5. Archive all evidence records to S3 (or artifact store).
6. Emit regression alert to observability-forensic-architect (DEC-033).

**CI Configuration (GitHub Actions Example):**

```yaml
name: Benchmark Regression Gate

on:
  push:
    branches:
      - main

jobs:
  benchmark:
    runs-on: ubuntu-latest-large
    steps:
      - uses: actions/checkout@v3
      
      - name: Build Release
        run: cargo build --release
      
      - name: Run Benchmarks
        run: |
          cargo run --release -p andromeda-cli -- benchmark run inventory-reserve-stock --diagnostic-json > /tmp/evidence-1.json
          cargo run --release -p andromeda-cli -- benchmark run catalog-resolve-procedure --diagnostic-json > /tmp/evidence-2.json
          cargo run --release -p andromeda-cli -- benchmark run wal-recovery-replay --diagnostic-json > /tmp/evidence-3.json
      
      - name: Evaluate Regression
        run: |
          python3 scripts/benchmark_regression_check.py /tmp/evidence-*.json --baseline .benchmark/baseline.json
      
      - name: Archive Evidence
        run: |
          aws s3 cp /tmp/evidence-*.json s3://andromeda-bench/$(date +%Y%m%d-%H%M%S)/
```

**Baseline Maintenance:**

- **Initial baseline:** Established on first merge (hardware profile, framework version recorded).
- **Update trigger:** Only after approved performance optimization merge.
- **Stability margin:** Baselines include 10% tolerance band to reduce flaky CI gates.

---

## Available Subcommands

### `benchmark workloads`

**Purpose:** List all registered workloads and their budgets.

**Syntax:**

```bash
andromeda-cli benchmark workloads [--diagnostic-json]
```

**Example Output:**

```
Registered Workloads:

1. inventory-reserve-stock
   Procedure: Inventory.ReserveStock
   Duration: 5–60 seconds (default 5s)
   Samples: 1–100 (default 10)
   Latency Budget (P99): ≤ 50 ms
   Throughput Budget: ≥ 1000 invocations/sec

2. catalog-resolve-procedure
   Procedure: System.CatalogResolveProcedure
   Duration: 2–10 seconds (default 2s)
   Samples: 1–100 (default 20)
   Latency Budget (P99): ≤ 5 ms
   Throughput Budget: ≥ 10000 lookups/sec

3. wal-recovery-replay
   Procedure: Internal.WalRecoveryReplay
   Duration: 10–60 seconds (default 10s)
   Samples: 1–20 (default 5)
   Throughput Budget: ≥ 100 MB/sec
```

### `benchmark contract`

**Purpose:** Display the benchmark framework contract and parameter bounds.

**Syntax:**

```bash
andromeda-cli benchmark contract [--diagnostic-json]
```

**Example Output:**

```
Benchmark Framework Contract (V1.0.0)

Global Limits:
  Max Duration: 60000 ms
  Max Samples: 100
  Max Warm-ups: 10

Hardware Profiles:
  Conservative (default): Multicore, 8GB+ RAM, no budget scaling
  Declared-Local: Single-core, <4GB RAM, latency +50%, throughput -30%

Evidence Format: JSON diagnostic only; never used as runtime mutation path
Audit Ledger: All benchmark runs emit audit events (DEC-033)
CI Gate: Regression detection via overall_status field
```

### `benchmark run`

**Purpose:** Execute a workload and produce evidence.

**Syntax:**

```bash
andromeda-cli benchmark run <workload-id> [options]
```

---

## Error Handling

### Validation Errors

| Error | Meaning | Remediation |
|-------|---------|-------------|
| `EmptyWorkloadId` | Workload ID is empty. | Provide a non-empty workload ID from the catalog. |
| `UnknownWorkload` | Workload not registered. | Run `benchmark workloads` to list available IDs. |
| `ZeroDuration` | Duration is 0. | Specify `--duration-ms` ≥ 1. |
| `ZeroSamples` | Sample count is 0. | Specify `--samples` ≥ 1. |
| `DurationExceedsGlobalLimit` | Duration > 60000 ms. | Use `--duration-ms` ≤ 60000. |
| `SamplesExceedsGlobalLimit` | Samples > 100. | Use `--samples` ≤ 100. |
| `DurationExceedsWorkloadLimit` | Duration > workload's max. | Check `benchmark workloads` for limits. |
| `SamplesExceedsWorkloadLimit` | Samples > workload's max. | Check `benchmark workloads` for limits. |
| `InsufficientSamplesForStatistics` | Runner collected 0 samples. | Increase `--duration-ms` or reduce `--samples`. |

---

## Related Documentation

- **DEC-033:** Durable Audit Ledger (benchmark evidence archival)
- **README.md:** Local V0 vertical prototype commands
- **CLI.md:** General command reference

---

## Versioning and Stability

- **Baseline:** V1.0.0 (Q1 2026)
- **Workload Additions:** New workloads will not break existing runs (backward compatible).
- **Budget Changes:** Require formal decision records and release notes.
- **Framework Changes:** Major version bumps documented in release notes.

---

## Glossary

| Term | Meaning |
|------|---------|
| **Workload** | Static definition of a benchmark (Procedure, budgets, bounds). |
| **Invocation** | Single execution of a Procedure within a benchmark run. |
| **Sample** | Latency measurement of one successful Procedure invocation. |
| **Warm-up** | Pre-run invocations to stabilize CPU cache and JIT; not counted in results. |
| **Evidence** | Timestamped result snapshot from one benchmark run. |
| **Performance Budget** | Threshold for latency (p99) and throughput (invocations/sec). |
| **Regression** | Evidence shows overall_status: FAIL due to latency or throughput violation. |
| **CI Gate** | Automated check that fails the pipeline if regression is detected. |

