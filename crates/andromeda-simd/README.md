# andromeda-simd

## Purpose

`andromeda-simd` owns optional SIMD dispatch boundaries for Andromeda analytical CPU acceleration.

## Scope

- Own `SimdDispatchRequest`, `SimdExecutionMode`, and dispatch validation.
- Require scalar CPU fallback before optional SIMD selection.
- Reject commit, WAL append, rollback, recovery, MVCC visibility, catalog publication, and security-critical paths.
- Keep scalar execution as the authoritative fallback.

## Non-goals

- Do not own SIMD kernel implementations, commit visibility, WAL ordering, rollback, recovery, catalog publication, or security-critical behavior.
- Do not make SIMD acceleration mandatory.
- Do not treat SIMD output as durable, catalog, transaction, recovery, or security truth.
- Do not introduce implicit SQL or gRPC runtime surfaces.

## Prerequisites

- Callers provide an `andromeda-hardware::CpuProfile`.
- Callers keep optional SIMD usage outside C5 truth paths.

## Procedure

1. Build a `SimdDispatchRequest` for an advisory analytical pipeline.
2. Set scalar fallback availability.
3. Enable SIMD only when the caller has a supported, bounded kernel.
4. Call `select_simd_dispatch` with a `CpuProfile`.
5. Treat `SimdExecutionMode::ScalarFallback` as authoritative fallback.

## Validation

- Inspect `src/lib.rs` for optional dispatch, scalar fallback, and C5 rejection.
- When validating by command, use `cargo check -p andromeda-simd --all-targets`.

## Troubleshooting

- If this crate touches C5 truth paths, remove that integration and keep SIMD dispatch outside the path.
- If a caller lacks scalar fallback, reject optional SIMD selection.

## References

- `AGENTS.md`
- `crates/README.md`
- `crates/andromeda-hardware/README.md`
