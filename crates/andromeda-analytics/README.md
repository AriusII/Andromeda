# andromeda-analytics

## Purpose

`andromeda-analytics` is the future owner crate for offline and batch analytical operators that support statistics refresh, map refresh, benchmark review, and operational diagnostics.

This crate currently owns runtime-free advisory job descriptors and the `AdvisoryAnalyticsJob` trait. No analytical operator execution or durable publication behavior has moved from existing owners.

## Scope

This crate is expected to own:

- `AnalyticsJobDescriptor`, `AnalyticsExecutionBounds`, `AnalyticsWorkloadKind`, and acceleration policy vocabulary.
- The advisory boundary trait used to prove analytics jobs stay bounded, CPU-fallback-capable, and outside C5 truth.
- Batch analytical descriptors and bounded execution policy for non-critical paths.
- CPU-first analytical kernels with optional acceleration only after fallback policy exists.
- Advisory outputs that can feed statistics, maps, or investigation workflows after validation.
- DecisionTrace inputs for analytical evidence used, ignored, or rejected.
- Explicit separation from C5 durable kernel behavior.

## Non-goals

- Do not put analytics work in commit, WAL, rollback, recovery, MVCC short-visibility, catalog publication, or security-critical paths.
- Do not treat analytical output, RAM state, temporary files, GPU output, or benchmark output as truth.
- Do not bypass typed Procedure contracts or catalog publication rules.
- Do not make GPU acceleration required for correctness.
- Do not serialize native Rust structs directly to disk or network.

## Prerequisites

- Keep analytics output advisory until a runtime owner validates and publishes a version-bound result.
- Require CPU fallback and disablement before any future GPU or SIMD acceleration.
- Keep all analytical jobs bounded by input size, temporary bytes, cancellation, and stop rules.

## Procedure

1. Define analytical job identity and limits before moving work into this crate.
2. Keep output advisory until validated by the owning statistics, map, or benchmark path.
3. Emit DecisionTrace evidence when analytical output affects an adaptive decision.
4. Reject any dependency edge from C5 durable-kernel crates to analytics.
5. Preserve current behavior until a registered extraction work order moves code.

## Validation

Future behavior changes should use:

```powershell
cargo check -p andromeda-analytics --all-targets
cargo test -p andromeda-analytics
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
cargo test -p andromeda-cli --test orphan_source_invariants -- --nocapture
```

Descriptor and trait changes must keep analytics advisory, bounded, disableable, and outside C5 durable-kernel dependency edges.

## Troubleshooting

- If analytics output is required for commit or recovery correctness, reject the design.
- If a GPU path has no CPU fallback, keep it disabled.
- If an analytical result affects plan choice, require version binding and DecisionTrace.

## References

- [Workspace crate rules](../README.md)
- [Current Map descriptor owner](../andromeda-maps/README.md)
- [Current columnar descriptor owner](../andromeda-columnar/README.md)
- [Hardware policy owner](../andromeda-hardware/README.md)
- [Current benchmark owner](../andromeda-bench/README.md)
