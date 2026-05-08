# andromeda-columnar

## Purpose

`andromeda-columnar` is the future owner crate for columnar analytical layout descriptors, batch scan contracts, and columnar evidence used by statistics, maps, analytics, and benchmarks.

This crate currently owns runtime-free advisory columnar layout descriptors and the `ColumnarArtifactDescriptor` trait. It does not own persisted bytes, scan execution, WAL coverage, recovery replay, or catalog publication.

## Scope

This crate is expected to own:

- `ColumnarLayoutDescriptor`, `ColumnarVersionBinding`, consumer vocabulary, and acceleration policy vocabulary.
- The advisory boundary trait used to prove columnar artifacts remain version-bound, CPU-fallback-capable, and outside C5 truth.
- Runtime-free columnar layout descriptors and scan-shape contracts.
- Version-bound columnar snapshots for analytical or diagnostic use.
- Explicit codecs if any persisted or networked columnar bytes are accepted later.
- Advisory evidence consumed by statistics, analytics, maps, or benchmark review.
- DecisionTrace inputs for columnar evidence used, ignored, or rejected.

## Non-goals

- Do not make columnar output the source of row, WAL, recovery, catalog, or commit truth.
- Do not bypass durable storage, WAL coverage, MVCC visibility, or Procedure contracts.
- Do not serialize Rust native structs directly to disk or network.
- Do not put columnar refresh, GPU work, or analytics work in C5 commit, WAL, rollback, recovery, MVCC short-visibility, catalog publication, or security-critical paths.
- Do not use benchmark output as columnar correctness proof.

## Prerequisites

- Treat columnar artifacts as derived and advisory unless a later C5 design adds explicit codecs, WAL coverage, and crash/recovery validation.
- Bind every derived view to catalog, storage, statistics, and policy versions.
- Require CPU fallback and disablement for any future acceleration path.

## Procedure

1. Define layout descriptors before adding storage or scan behavior.
2. Keep derived columnar views version-bound and invalidatable.
3. Record DecisionTrace when columnar evidence affects statistics or optimizer decisions.
4. Keep GPU and analytics dependencies outside durable-kernel crates.
5. Preserve existing storage behavior until an explicit extraction work order moves code.

## Validation

Future behavior changes should use:

```powershell
cargo check -p andromeda-columnar --all-targets
cargo test -p andromeda-columnar
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
```

Persisted columnar bytes would also require explicit codec, golden-vector, fuzz, and crash/recovery validation before acceptance. Descriptor and trait changes alone do not prove release readiness.

## Troubleshooting

- If a columnar artifact is used as truth, route correctness through storage, WAL, and recovery owners instead.
- If a scan depends on GPU output, require CPU fallback and prove the path is outside C5.
- If a derived view lacks version binding, reject reuse and rebuild from authoritative owners.

## References

- [Workspace crate rules](../README.md)
- [Current analytics descriptor owner](../andromeda-analytics/README.md)
- [Current Map descriptor owner](../andromeda-maps/README.md)
- [Storage owner](../andromeda-storage/README.md)
- [Hardware policy owner](../andromeda-hardware/README.md)
