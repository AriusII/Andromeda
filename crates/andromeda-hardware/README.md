# andromeda-hardware

## Purpose

`andromeda-hardware` defines conservative CPU, RAM, GPU, pipeline, resource, and optional acceleration policy descriptors for Andromeda crates.

Use this crate when code needs to describe hardware capability classes, resource budgets, GPU eligibility, optional SIMD admission, or vector advisory validation without depending on runtime execution, benchmarking, analytics, or device-specific crates.

## Scope

This crate owns:

- CPU architecture and capability descriptors.
- RAM section budget descriptors and budget validation.
- GPU availability and execution policy descriptors.
- Optional GPU, SIMD, and vector advisory admission contracts.
- `PipelineClass` values that separate critical engine truth paths from non-critical work.
- `HardwareProfile` and `ResourceBudget` aggregation types.

The crate describes policy and capability inputs. It does not detect hardware, allocate memory, schedule work, run GPU kernels, or prove benchmark results.

## Non-goals

- Do not put GPU work in commit, WAL append, rollback, recovery, MVCC visibility, catalog publication, or security-critical paths.
- Do not add GPU kernels, concrete SIMD dispatch implementations, hardware probing, async scheduling, direct I/O implementations, or benchmark execution.
- Do not treat RAM, temp storage, GPU output, or benchmark output as durable truth.
- Do not add dependencies on storage, WAL, transaction, catalog, SRPL, execution, RPC runtime, benchmark, analytics, or GPU crates.
- Do not serialize Rust native profile structs directly to disk or network.

## Allowed Dependencies

`andromeda-hardware` may depend on:

- `andromeda-error`
- The Rust standard library

No other workspace or external dependency is allowed without an ADR and topology validation update.

## Invariants

- `unsafe` code is forbidden by `src/lib.rs`.
- Conservative profiles must disable GPU execution and assume no SIMD.
- `PipelineClass::is_critical_path()` must include commit, WAL append, rollback, recovery, MVCC visibility, catalog publication, and security-critical paths.
- GPU policy validation must reject every critical engine truth path.
- Optional acceleration requests must require an advisory pipeline and an explicit CPU/scalar/source-truth fallback.
- `GpuExecutionPolicy::BatchAnalyticsOnly` may allow only statistics refresh, map refresh, and batch analytics.
- RAM section totals must not exceed a nonzero declared RAM total.
- Saturating arithmetic is used when summing declared RAM sections to avoid overflow.
- Hardware descriptors are policy inputs; they do not authorize unsafe acceleration or durable publication by themselves.

## Prerequisites

Before changing this crate:

1. Read `../../AGENTS.md`.
2. Read `../AGENTS.md`.
3. Check `../README.md` and `../../docs/adr/ADR-0002-WORKSPACE_AND_CRATE_BOUNDARIES.md` for R0 dependency rules.
4. Review GPU exclusion, optional acceleration, RAM budget, CPU profile, and pipeline tests in `src/`.

## Procedure

To use this crate:

1. Start from `HardwareProfile::conservative()` when capabilities are unknown.
2. Validate GPU eligibility with `GpuProfile::validate_pipeline` or `select_optional_gpu` before allowing any GPU-backed path.
3. Validate RAM declarations with `validate_ram_budgets` before using section budgets as policy input.
4. Keep actual resource allocation, device probing, and scheduling in the owning runtime crate.

To extend this crate:

1. Add a descriptor only when it is runtime-free and shared across multiple owners.
2. Add tests that prove critical-path exclusion and conservative defaults.
3. Keep acceleration optional, observable, bounded, explainable, and disableable.
4. Recheck dependency topology if `Cargo.toml` changes.

## Validation

Run the focused crate test after changing source or documentation that describes source behavior:

```powershell
cargo test -p andromeda-hardware
```

Run the topology guard if dependencies or crate-boundary text changes:

```powershell
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
```

## Troubleshooting

| Symptom | Action |
|---|---|
| A pipeline wants GPU execution but validation rejects it. | Treat the rejection as authoritative unless an ADR changes the pipeline class or GPU policy. |
| A caller needs hardware detection. | Implement detection in a runtime owner and convert the result into these descriptors. |
| RAM section budgets exceed the declared total. | Reduce section declarations or use an explicit zero total only when total memory is unknown. |

## References

- `Cargo.toml`
- `src/lib.rs`
- `src/acceleration.rs`
- `src/cpu.rs`
- `src/gpu.rs`
- `src/ram.rs`
- `src/pipeline.rs`
- `src/integration.rs`
- `../README.md`
- `../../docs/adr/ADR-0002-WORKSPACE_AND_CRATE_BOUNDARIES.md`
