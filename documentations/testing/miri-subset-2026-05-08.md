# Miri Subset for Memory and Codec-Sensitive Crates

## Purpose

Define a concrete, bounded Miri subset for memory and codec-sensitive Andromeda crates.

This subset is an advisory undefined-behavior smoke gate. It does not establish release readiness, durable correctness, crash/recovery correctness, Procedure contract compatibility, or security readiness.

## Scope

This subset targets crates that are small, deterministic, and do not currently own heavy build scripts:

| Crate | Command | Focus |
| --- | --- | --- |
| `andromeda-codec` | `cargo +nightly miri test -p andromeda-codec --lib --all-features --locked` | Little-endian codec helpers and bounded byte handling. |
| `andromeda-maps` | `cargo +nightly miri test -p andromeda-maps --lib --all-features --locked` | Map descriptor and publication value invariants without GPU work. |
| `andromeda-policy` | `cargo +nightly miri test -p andromeda-policy --lib --all-features --locked` | Admission policy value handling and fail-closed checks. |
| `andromeda-resource` | `cargo +nightly miri test -p andromeda-resource --lib --all-features --locked` | Resource limits, checked arithmetic, and error construction. |
| `andromeda-procedure-store` | `cargo +nightly miri test -p andromeda-procedure-store --lib --all-features --locked` | Procedure evidence, status, identity, and sink value paths. |
| `andromeda-contract` | `cargo +nightly miri test -p andromeda-contract --lib --all-features --locked` | ProcedureContract shape, compatibility, and hash input handling. |

Use `tools/testing/miri_subset.py` to print or run the exact commands.

## Non-goals

- Do not claim release readiness from this subset.
- Do not treat a passing Miri subset as proof of WAL durability, visible-commit ordering, crash/recovery correctness, catalog publication correctness, IAM correctness, or RPC compatibility.
- Do not use Miri as a replacement for fuzzing malformed persisted or network bytes.
- Do not use Miri as a replacement for Loom when the claim depends on concurrency interleavings.
- Do not use Miri as a replacement for crash/recovery tests when behavior affects durable C4 or C5 state.
- Do not add root-owned executable Miri harnesses from this document. Executable tests remain owned by crates.

## Prerequisites

- Run commands from the repository root.
- Use a nightly Rust toolchain with the Miri component installed.
- Keep `Cargo.lock` authoritative by using `--locked`.
- Ensure the selected crate does not require external services, wall-clock timing assumptions, GPU output, generated benchmark output, or unsupported platform behavior.
- Record `rustc +nightly -Vv`, `cargo +nightly --version`, the Miri command, and the result when retaining evidence.

## Procedure

Print the default dry-run command list:

```powershell
python tools/testing/miri_subset.py
```

Print the command list with default exclusions:

```powershell
python tools/testing/miri_subset.py --show-exclusions
```

Check whether the local Miri environment is usable without running crate tests:

```powershell
python tools/testing/miri_subset.py --check-env
```

Run one target when Miri is installed and the crate status is `ready`:

```powershell
python tools/testing/miri_subset.py --only andromeda-codec --run
```

Run the full subset only when a local target cache write is acceptable:

```powershell
python tools/testing/miri_subset.py --run
```

## No-Go Conditions

Do not run or promote this subset as evidence when any of these conditions apply:

| Condition | Required disposition |
| --- | --- |
| The nightly toolchain or Miri component is missing. | Record the exact `cargo +nightly miri --version` failure and keep the result as dry-run only. |
| A target crate has a `build.rs` or requires generated code. | Keep it outside the light subset unless an owner creates a smaller Miri target. |
| A test requires external services, network runtime behavior, timing assumptions, GPU output, fuzz artifacts, benchmark output, or temporary files as truth. | Reject the target for this subset and move the claim to its owning validation gate. |
| Miri reports undefined behavior. | Treat the target as failed until the owning crate fixes the issue and reruns the exact command. |
| The affected behavior is WAL append, WAL flush, visible commit, rollback, recovery replay, MVCC short visibility, catalog publication, Procedure admission, authorization, durable audit, or security-critical admission. | Require owner-crate tests plus crash/recovery, fuzz, Loom, sanitizer, security, or protocol evidence as applicable. |
| The release claim depends on persisted or network bytes. | Pair Miri with explicit codec golden vectors, malformed-input fuzzing, compatibility tests, and crash/recovery gates before promotion. |

## Validation

Worker H validation on May 8, 2026:

```powershell
rustup toolchain list
rustup component list --toolchain nightly --installed
cargo +nightly miri --version
cargo +nightly --version
rustc +nightly -Vv
```

Observed result:

- `nightly-x86_64-pc-windows-msvc` is installed.
- The installed nightly components did not include `miri`.
- `cargo +nightly miri --version` failed with: `error: 'cargo-miri.exe' is not installed for the toolchain 'nightly-x86_64-pc-windows-msvc'.`
- No crate Miri command was run.

Validate the script itself:

```powershell
python -c "from pathlib import Path; source=Path('tools/testing/miri_subset.py').read_text(encoding='utf-8'); compile(source, 'tools/testing/miri_subset.py', 'exec')"
python tools/testing/miri_subset.py
python tools/testing/miri_subset.py --check-env
```

After installing Miri, collect the smallest first signal:

```powershell
cargo +nightly miri test -p andromeda-codec --lib --all-features --locked
```

## Troubleshooting

If `cargo +nightly miri --version` says `cargo-miri.exe` is not installed, install the Miri component for the exact nightly toolchain and rerun `python tools/testing/miri_subset.py --check-env`.

If a crate becomes blocked by a new `build.rs`, generated code, or platform-specific dependency, remove it from this light subset until an owner defines a smaller deterministic Miri target.

If Miri fails on unsupported platform behavior, keep the result as blocked rather than passing. Create a smaller owner-crate test around the unsafe, aliasing, layout, or byte-handling boundary.

If a target passes under Miri but fails fuzz, crash/recovery, security, or protocol gates, treat the stronger failed gate as authoritative for release decisions.

## References

- `tools/testing/miri_subset.py`
- `tests/miri/README.md`
- `documentations/testing/fuzz-miri-loom-evidence.md`
- `documentations/testing/unsafe-miri-inventory-2026-05-08.md`
- `docs/codex/rust-critical-quality-gates.md`
- `docs/codex/mission-critical-change-policy.md`
