# andromeda-gpu

## Purpose

`andromeda-gpu` owns optional GPU advisory selection boundaries for Andromeda analytical work.

## Scope

- Own `OptionalGpuRequest`, `OptionalGpuSelection`, and selection validation.
- Require CPU fallback and a cancellation boundary before GPU selection.
- Reject commit, WAL append, rollback, recovery, MVCC visibility, catalog publication, and security-critical paths.
- Treat selected GPU execution as advisory only.

## Non-goals

- Do not own runtime GPU kernels, device discovery, commit visibility, WAL ordering, rollback, recovery, catalog publication, or security-critical behavior.
- Do not make GPU acceleration mandatory.
- Do not treat GPU output as durable, catalog, transaction, recovery, or security truth.
- Do not introduce implicit SQL or gRPC runtime surfaces.

## Prerequisites

- Callers provide an `andromeda-hardware::GpuProfile`.
- Callers keep optional GPU usage outside C5 truth paths.

## Procedure

1. Build an `OptionalGpuRequest` for an advisory analytical pipeline.
2. Set CPU fallback and cancellation boundary requirements.
3. Call `select_optional_gpu` with a `GpuProfile`.
4. Treat `OptionalGpuSelection::CpuFallback` as authoritative fallback.
5. Treat `OptionalGpuSelection::AdvisoryGpu` as advisory only.

## Validation

- Inspect `src/lib.rs` for advisory-only selection and C5 rejection.
- When validating by command, use `cargo check -p andromeda-gpu --all-targets`.

## Troubleshooting

- If this crate touches C5 truth paths, remove that integration and keep GPU selection outside the path.
- If a caller lacks CPU fallback or cancellation, select CPU fallback or reject the request.

## References

- `AGENTS.md`
- `crates/README.md`
- `crates/andromeda-hardware/README.md`
