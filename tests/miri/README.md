# Miri Test Index

## Purpose

This directory is a roadmap index for Miri validation. It does not own executable Rust tests.

Miri coverage is used to check undefined-behavior risks in unsafe code, aliasing-sensitive code, layout-sensitive code, FFI boundaries, and low-level codecs.

## Scope

Use this index for Rust code that touches unsafe blocks, pointer aliasing, manual memory layout, FFI adapters, binary codecs, page buffers, WAL buffers, SIMD fallbacks, or concurrency primitives that Miri can exercise.

The executable tests remain in the owning crate.

## Non-goals

- Do not create a root-level Miri harness here.
- Do not use Miri as a replacement for crash/recovery, fuzzing, property tests, or protocol compatibility tests.
- Do not serialize Rust native structs directly to disk or network.
- Do not widen unsafe visibility to make a Miri test easier to write.

## Prerequisites

- Nightly Rust with the Miri component installed.
- A crate-owned test that can run under Miri without external services.
- Feature flags or test filters that keep the Miri target bounded.
- Documented unsafe invariants for code under test.

## Procedure

1. Identify the unsafe, aliasing, layout, or FFI-sensitive boundary.
2. Add or update the executable test in the owning crate.
3. Keep the test deterministic and bounded.
4. Document the unsafe invariant and exact Miri command in this index when the target exists.
5. Pair Miri with fuzz, property, or crash/recovery validation when persisted bytes or C5 behavior are involved.

## Concrete Subsets

The May 8, 2026 light subset is documented in `docs/testing/fuzz-miri-loom.md`.

Use it for a bounded dry-run list of memory and codec-sensitive crates that currently avoid heavy build scripts:

```powershell
python tools/testing/miri_subset.py
```

When nightly Miri is installed, run one target first:

```powershell
python tools/testing/miri_subset.py --only andromeda-maps --run
```

This subset currently covers `andromeda-maps`, `andromeda-policy`, `andromeda-resource`, `andromeda-procedure-store`, and `andromeda-contract`. It is advisory evidence only and does not imply release readiness.

## Acceptance Criteria

- Each Miri entry names the owning crate, unsafe or aliasing invariant, target command, feature flags, and residual risk.
- The test does not require external services, wall-clock timing, GPU output, or generated benchmark output.
- Persistent and network formats use explicit codecs, not native struct layout.
- Unsafe code remains private, documented, tested, and reviewable.
- C5 behavior still has deterministic crash/recovery or protocol validation outside Miri.

## Validation

Recommended broad preflight when feasible:

```powershell
cargo +nightly miri test --workspace --all-features
```

For bounded targets, prefer an owning-crate command:

```powershell
cargo +nightly miri test -p <owning-crate> <target-name> --all-features
```

## Troubleshooting

If Miri cannot run a target because of unsupported platform behavior, isolate the unsafe boundary behind a smaller deterministic test and document the unsupported dependency as residual risk.

If Miri reports undefined behavior in a C4/C5 path, treat the result as a release blocker until the owning crate has a fix and a repeatable validation command.

## References

- `tests/README.md`
- `docs/testing/fuzz-miri-loom.md`
- `docs/testing/release-gates.md`
