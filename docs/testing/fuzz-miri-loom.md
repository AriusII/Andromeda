# Fuzz, Miri, And Loom Evidence

## Purpose

Define evidence requirements for malformed input, undefined-behavior, and
bounded concurrency validation. These gates are deep validation; they do not
replace crash/recovery, durable audit, authorization, typed Procedure, or
visible-commit evidence.

## When Required

| Gate | Required when the release scope touches |
| --- | --- |
| Fuzz | Persisted bytes, network bytes, parsers, codecs, Procedure contract input, ResultStream sequencing, security admission matrices, audit journal bytes, backup or restore inputs. |
| Miri | Unsafe Rust, aliasing-sensitive references, manual layout, pointer conversion, FFI, SIMD fallback, low-level buffers, or memory-sensitive value paths. |
| Loom | Lock ordering, shutdown, cancellation, publication ordering, WAL visibility, backpressure, RPC session state, ResultStream state, buffer ownership, or recovery coordination. |

## Fuzz Evidence

Preflight:

```powershell
python fuzz/generators/generate_seed_corpus.py --check
cargo check --manifest-path fuzz/Cargo.toml --locked
cargo check --manifest-path fuzz/Cargo.toml --bin <target> --locked
```

Sustained run shape:

```powershell
cargo +nightly fuzz run <target> -- -max_total_time=<seconds>
```

Record target, owner surface, corpus path, duration, seed generation result,
toolchain, crash artifacts, minimization notes, result, and residual risk. A
short smoke run is preflight evidence only.

## Miri Evidence

Broad preflight when practical:

```powershell
cargo +nightly miri setup
cargo +nightly miri test --workspace --all-features
```

Bounded owner target:

```powershell
cargo +nightly miri test -p <owning-crate> <target-name> --all-features --locked
```

Record the unsafe or memory invariant, command, nightly version, unsupported
operations, owner disposition, result, and residual risk.

## Loom Evidence

Standalone model command:

```powershell
cargo test --manifest-path tools/loom-models/Cargo.toml --locked
```

The current standalone model lives under `tools/loom-models/` and checks the
WAL durable-before-visible publication rule with bounded interleavings. It is
not a substitute for owner-crate production tests, crash/recovery validation, or
durable visibility evidence.

For each Loom record, capture:

- model path;
- modeled state;
- thread or task bound;
- explored safety property;
- command and feature flags;
- result;
- unmodeled behavior and residual risk.

## No-Go Conditions

- Do not replace fuzzing with Miri.
- Do not replace Loom with sleeps, retries, timing assumptions, or scheduler
  luck.
- Do not use fuzz, Miri, or Loom output as database truth.
- Do not claim release readiness from compile-only, advisory, or
  continue-on-error output.

## References

- `docs/testing/release-evidence-template.md`
- `docs/testing/release-gates.md`
- `tests/README.md`
- `tools/loom-models/README.md`
