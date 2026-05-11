# andromeda-gpu

Optional GPU batch acceleration primitives for advisory-only analytical workloads.

## Status

**Scaffold v0.** Types and contracts only. No GPU runtime. No default features.
Requires CPU fallback and validation gate before any result publication.

## Design invariants

- **Never C5.** This crate must not appear in the dependency graph of any C5
  durable-kernel crate (`andromeda-wal`, `andromeda-recovery`, `andromeda-mvcc`,
  `andromeda-transaction`, `andromeda-security`, etc.).
- **No GPU runtime.** No `wgpu`, `cuda`, `ash`, `opencl3`, or equivalent crates
  are or will ever be direct dependencies. GPU closures are injected by callers.
- **CPU fallback mandatory.** Every GPU execution path requires a `CpuFallback`
  implementation. The CPU path is the authoritative source of truth.
- **Kill switch.** `KillSwitchHandle` provides cooperative cancellation via an
  `Arc<AtomicU8>`. Cancelled jobs short-circuit without invoking CPU fallback.
- **Validation gate before publish.** GPU-produced statistics or analytics output
  must be validated by CPU shadow before any result can be published. Silent GPU
  output publication is explicitly rejected.
- **No unsafe code.** `#![forbid(unsafe_code)]` is enforced at crate root.
- **No native struct layout for wire/persistent data.** This crate has no
  persistence requirements in v0; all types are in-memory only.

## Features

| Feature | Default | Description |
|---|---|---|
| `trace` | off | Reserved: future integration with `andromeda-observe` (v1+). |

## Module layout

```
andromeda_gpu
├── budget       — GpuBudget, GpuBudgetRequest
├── fallback     — CpuFallback trait, ValidationState
├── job          — GpuJobClass, FallbackReason
├── kill_switch  — KillSwitch, KillReason, KillSwitchHandle
├── policy       — re-exports from andromeda_hardware
├── trace        — GpuExecutionTrace, GpuExecutionResult
├── validation   — GpuStatsValidationGate, Histogram
└── prototypes
    ├── stats    — GpuStatsBindingContext (GPU_STATS prototype)
    └── analytics— GpuAnalyticsBindingContext (GPU_ANALYTICS prototype)
```

## Validation gate (publish gate)

GPU output is only returnable if `GpuStatsValidationGate::validate_histogram`
returns `ValidationState::Validated`. Any divergence beyond the configured
deviation threshold causes the gate to return `ValidationState::Rejected(...)`,
and the CPU shadow result is used instead.

## P13+ residual TODO

- Wire `GpuExecutionTrace` emission to `andromeda-observe` via the `trace` feature.
- Add real timestamps from a monotonic clock abstraction.
- Add `GpuAnalyticsValidationGate` with configurable sample-based validation.
- Add benchmark job class prototype (`GpuJobClass::Benchmark`).
