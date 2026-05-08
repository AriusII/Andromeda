# Unsafe, Lock-Free, and Miri Inventory

## Purpose

Define the inventory and evidence requirements for unsafe Rust, lock-free or atomic code, Miri checks, and sanitizer checks in Andromeda.

This document turns a source inventory into release evidence expectations. It does not approve unsafe code, lock-free algorithms, C4 behavior, or C5 behavior by itself.

## Scope

This guidance applies to Rust code and release evidence that touches:

- unsafe blocks, unsafe functions, unsafe traits, or unsafe impls;
- raw pointers, manual layout, unchecked aliasing, unchecked lifetimes, FFI, SIMD, allocator, memory map, or generated unsafe boundaries;
- atomic counters, lock-free reads, compare-and-swap loops, `UnsafeCell`, custom synchronization, and future reclamation structures;
- WAL, storage, transaction, recovery, catalog publication, RPC, security, audit, or Procedure execution code where memory safety or concurrency behavior can affect C4/C5 claims.

Use `tools/testing/unsafe_inventory.py` to produce the syntactic unsafe inventory:

```powershell
python tools/testing/unsafe_inventory.py --summary
```

Use the advisory lock-free scan when a change or release claim depends on atomic or lock-free behavior:

```powershell
python tools/testing/unsafe_inventory.py --include-lock-free --summary
```

The script is read-only and reports inventory entries as:

```text
path:line:classification
```

## Non-goals

- Do not use this document to authorize new unsafe code.
- Do not treat a clean unsafe inventory as proof that dependencies, generated target output, platform intrinsics, or build scripts are unsafe-free.
- Do not use Miri as a replacement for crash/recovery, fuzzing, property tests, protocol compatibility, IAM, audit, or WAL durability evidence.
- Do not use sanitizer output as durable database truth.
- Do not promote C4/C5 behavior when Miri, sanitizer, Loom, fuzz, property, or crash/recovery evidence is missing for an affected risk.
- Do not create root-level executable Miri, Loom, or sanitizer harness ownership from this document. Executable tests stay with the owning crate.

## Prerequisites

Before using the inventory for a review or release packet:

- identify the owning crate, subsystem, and risk class;
- run the inventory from the repository root;
- retain the command, branch, commit SHA, `git status --short`, toolchain versions, and output artifact;
- map every unsafe entry to a local `SAFETY:` invariant, safe wrapper, and owner;
- map every lock-free or atomic release claim to a concurrency property, ordering rationale, and owner test;
- classify whether the affected behavior is C0 through C5;
- name the exact Miri, sanitizer, Loom, fuzz, property, crash/recovery, or manual audit evidence required before promotion.

## Procedure

1. Run the unsafe inventory.
2. For each `path:line:classification` entry, identify the owning crate and whether the path is production, test, fuzz, benchmark, generated, or documentation-only code.
3. Confirm that production unsafe code is private by default, has a nearby `SAFETY:` invariant, and is hidden behind a safe wrapper.
4. Reject or escalate public unsafe APIs unless a narrower ADR or owner decision explicitly approves the contract.
5. Run the advisory lock-free scan for any change that depends on atomic counters, compare-and-swap loops, lock-free reads, custom synchronization, or `UnsafeCell`.
6. Map the entry to the evidence matrix in this document.
7. Record retained evidence in the release packet or review record.
8. Treat missing evidence as residual risk. Treat missing evidence on affected C4/C5 behavior as a no-go condition.

## Inventory Classifications

| Classification | Meaning | Minimum review record |
| --- | --- | --- |
| `unsafe_block` | A local block requires the compiler's unsafe contract. | Path, line, owning crate, `SAFETY:` invariant, safe wrapper, caller-visible behavior, tests that exercise valid and invalid preconditions. |
| `unsafe_function` | A function requires callers or internal code to uphold unsafe preconditions. | Visibility, public contract review, caller set, precondition enforcement, safe alternative analysis, Miri target, and owner approval. |
| `unsafe_trait` | Implementors must uphold a safety contract that the compiler cannot verify. | Trait safety contract, implementor list, external implementation policy, compatibility risk, and review owner. |
| `unsafe_impl` | An implementation asserts a safety property, commonly `Send`, `Sync`, FFI, or layout behavior. | Type invariant, aliasing and thread-safety proof, safe wrapper, Miri target where applicable, and Loom or sanitizer evidence for concurrency-sensitive impls. |
| `lock_free_atomic_type` | Advisory scan found atomic types. | Owner determines whether the atomic is advisory metrics, synchronization, admission, visibility, or durable evidence. |
| `lock_free_atomic_ordering` | Advisory scan found explicit atomic ordering. | Ordering rationale, affected invariant, and proof that relaxed or acquire/release use cannot change C4/C5 truth. |
| `lock_free_compare_exchange` | Advisory scan found compare-and-swap behavior. | Loop termination, ABA or stale-read analysis, publication property, and Loom or stress evidence. |
| `lock_free_fetch_update` | Advisory scan found atomic mutation helpers. | Invariant being updated, overflow or wrap policy, ordering rationale, and concurrency evidence if used outside metrics. |

The lock-free classifications are advisory. They locate code that may need concurrency evidence; they do not prove that an algorithm is lock-free or correct.

## Evidence Mapping

| Inventory surface | Required Miri evidence | Required sanitizer evidence | Additional evidence |
| --- | --- | --- | --- |
| Unsafe block around pointer, slice, layout, alignment, initialization, or aliasing behavior. | Bounded owner-crate Miri test that exercises the safe wrapper and states the unsafe invariant. | AddressSanitizer when memory access, allocation, or buffer bounds are material. | Unit and property tests for rejected preconditions. Fuzz malformed bytes when the boundary decodes persisted or network input. |
| Unsafe function. | Miri target for the smallest safe public or crate-visible wrapper. Public unsafe functions require ADR or owner approval before release use. | AddressSanitizer for memory or FFI behavior. | Caller inventory, visibility review, and regression tests that prevent widening the unsafe contract. |
| Unsafe trait or unsafe impl. | Miri where aliasing, initialization, layout, or pointer behavior can be exercised. | ThreadSanitizer where supported for data-race-sensitive `Send`, `Sync`, or custom synchronization claims. AddressSanitizer for memory-sensitive impls. | Loom model for interleavings when the impl affects concurrency, publication, backpressure, lock ordering, or visibility. |
| FFI, SIMD, allocator, memory map, or generated unsafe boundary. | Miri target for the Rust-side safe wrapper when supported. Unsupported operations must be recorded as residual risk. | AddressSanitizer for buffer and allocation behavior. ThreadSanitizer for shared-state FFI where supported. | Generator/version record for generated code, platform matrix, scalar fallback tests for SIMD, and manual audit. |
| Atomic counters used only for advisory metrics. | Not required unless unsafe, aliasing, or layout behavior is present. | Not required by default. | Unit tests for monotonicity or reset behavior. Metrics cannot become C5 truth. |
| Atomic or lock-free state used for admission, publication, visibility, backpressure, shutdown, or recovery coordination. | Required if the code also has unsafe, aliasing-sensitive, or layout-sensitive behavior. | ThreadSanitizer where supported for race-sensitive execution. | Loom or equivalent bounded interleaving evidence, owner-crate tests, and C4/C5 integration gates. |
| Persisted bytes, WAL frames, page buffers, manifest data, audit journal bytes, or network frames. | Miri where unsafe or aliasing-sensitive code touches the bytes. | AddressSanitizer for buffer safety when applicable. | Explicit codec golden vectors, property tests, fuzz targets, compatibility tests, and crash/recovery evidence before durable claims. |

Use these command shapes for retained evidence:

```powershell
cargo +nightly miri setup
cargo +nightly miri test -p <owning-crate> <target-name> --all-features
```

```powershell
$env:RUSTFLAGS="-Zsanitizer=address"
$env:RUSTDOCFLAGS="-Zsanitizer=address"
cargo +nightly test -p <owning-crate> --all-features -Z build-std
```

ThreadSanitizer evidence is target- and toolchain-sensitive. Use it only where the selected nightly, target, dependencies, and owner-crate test can support it, and record unsupported operations as residual risk.

## C4/C5 No-Go Conditions

Block or escalate a C4/C5 claim when any of these conditions apply:

| Condition | Required disposition |
| --- | --- |
| Unsafe code appears in commit visibility, WAL append or flush, rollback, recovery replay, MVCC short visibility, catalog publication, Procedure admission, authorization, or security-critical admission without a narrower ADR and retained evidence. | No-go. Remove the unsafe path or escalate to architecture and release owners with exact invariant, owner, Miri or sanitizer evidence, crash/recovery evidence, and rollback plan. |
| An unsafe block lacks a local `SAFETY:` invariant. | No-go for promotion. Add the invariant, safe wrapper, tests, and review evidence before release use. |
| A public unsafe API is exposed from a production crate. | No-go unless an ADR explicitly approves the public unsafe contract and caller obligations. |
| Miri reports undefined behavior for an affected C4/C5 path. | No-go until fixed and rerun with retained passing evidence. |
| AddressSanitizer or ThreadSanitizer reports memory errors, data races, or unsupported critical operations that leave the invariant untested. | No-go until fixed or scoped out with owner-approved residual risk. |
| Lock-free or atomic code decides visibility, admission, durability, recovery, audit, or terminal ResultStream state without ordering rationale and interleaving evidence. | No-go until Loom, sanitizer, owner tests, and integration evidence cover the property. |
| Fuzz, benchmark, RAM, temp file, GPU, or advisory metrics output is the only evidence for an unsafe or lock-free optimization. | No-go. Add correctness, Miri or sanitizer, property, fuzz, and C4/C5 gates as applicable. |
| Persisted or network bytes depend on native Rust layout. | No-go. Replace with explicit little-endian codecs and validate with golden vectors, fuzz, and compatibility tests. |
| Crash/recovery evidence is missing for unsafe or lock-free behavior that can affect durable C5 state. | No-go for C5 release claims. Miri and sanitizers are supplemental, not recovery proof. |

## Validation

For this documentation and inventory tooling, validate with:

```powershell
python -c "from pathlib import Path; source=Path('tools/testing/unsafe_inventory.py').read_text(encoding='utf-8'); compile(source, 'tools/testing/unsafe_inventory.py', 'exec')"
python tools/testing/unsafe_inventory.py --summary
python tools/testing/unsafe_inventory.py --include-lock-free --summary
```

For a production Rust change that introduces or changes inventory entries, add the smallest applicable owner gate:

```powershell
cargo fmt --all --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo +nightly miri test -p <owning-crate> <target-name> --all-features
```

For C4/C5 behavior, add the relevant property, fuzz, Loom, sanitizer, crash/recovery, security, RPC, catalog, or audit evidence. A documentation-only inventory update does not run or imply those gates.

## Troubleshooting

If the unsafe inventory is empty, record the command and output, but do not claim that dependency code, generated target output, platform intrinsics, or build scripts are unsafe-free.

If a false positive appears in the advisory lock-free scan, classify it as advisory-only and retain the owner disposition. Do not remove the heuristic from release evidence without confirming the line is not part of a C4/C5 claim.

If Miri cannot run because a dependency, platform operation, FFI call, or async runtime is unsupported, isolate the unsafe boundary behind a smaller deterministic owner-crate test or record the unsupported operation as residual risk.

If sanitizer flags are unavailable on the target platform, use the supported target that best exercises the owner boundary and record the target limitation.

If Miri, sanitizer, Loom, fuzz, or crash/recovery evidence conflicts, treat the strongest failing gate for the C4/C5 claim as the blocker.

## References

- `AGENTS.md`
- `docs/adr/ADR-0013-unsafe-rust-policy.md`
- `docs/codex/rust-critical-quality-gates.md`
- `tests/miri/README.md`
- `tests/loom/README.md`
- `documentations/testing/fuzz-miri-loom-evidence.md`
- `documentations/testing/ci-release-gate-evidence.md`
- `documentations/testing/step-11-validation-matrix.md`
- `.github/workflows/06-nightly-deep-validation.yml`
- `tools/testing/unsafe_inventory.py`
