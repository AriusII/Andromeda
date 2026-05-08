# MSRV Dependency Risk Evidence 2026-05-08

## Purpose

Record the supply-chain evidence for the workspace `rust-version = "1.95.0"` baseline and the dependency risk created when locked dependencies require a newer Rust compiler.

This evidence supports release review. It records the retired Rust 1.85 compatibility risk, the active Rust 1.95.0 baseline, and the required release evidence. It does not approve a dependency update or release claim by itself.

## Scope

This evidence covers:

- The root workspace Rust baseline in `Cargo.toml`.
- The current `Cargo.lock` dependency set.
- The optional read-only checker in `tools/testing/msrv_dependency_check.py`.
- Dependency `rust_version` metadata that is visible from local Cargo metadata on 2026-05-08.

The risk class is important for routine development and release-blocking for any release that claims compatibility with Rust 1.95.0.

## Non-goals

- This document does not approve release readiness by itself.
- This document does not mutate `Cargo.lock` or dependency versions.
- This document does not replace `cargo audit`, `cargo deny`, or cargo-vet governance.
- This document does not certify all target triples, optional features, or platform-specific dependency paths.
- This document does not treat dependency tooling output as database truth.

## Prerequisites

Use these inputs for repeatable evidence:

- `Cargo.toml` with `[workspace.package] rust-version = "1.95.0"`.
- `Cargo.lock` generated for the current workspace.
- Python with the standard library for `tools/testing/msrv_dependency_check.py`.
- Optional Cargo metadata evidence from:

```bash
cargo metadata --locked --offline --format-version 1
```

## Procedure

1. Confirm the workspace Rust baseline from `Cargo.toml`.
2. Scan `Cargo.lock` for package-level `rust-version` or `rust_version` metadata.
3. Run the optional checker:

```bash
python tools/testing/msrv_dependency_check.py
```

4. If `Cargo.lock` has no dependency MSRV metadata, record the result as inconclusive, not passing.
5. For release evidence, run a richer metadata or build gate before claiming Rust 1.95.0 compatibility.
6. If a dependency declares `rust_version` greater than `1.95.0`, classify the release claim as blocked until one of these actions is accepted:
   - downgrade or pin the dependency to a Rust 1.95.0-compatible version;
   - isolate the dependency outside every release-critical target path and document the proof;
   - raise the workspace baseline through an accepted decision record.

## Current Evidence

| Source | Finding                                                                                                                       | Implication                                                                               | Decision                                                                                                              |
| --- |-------------------------------------------------------------------------------------------------------------------------------|-------------------------------------------------------------------------------------------|-----------------------------------------------------------------------------------------------------------------------|
| `Cargo.toml` | `[workspace.package] rust-version = "1.95.0"` is the active baseline.                                                         | Release evidence must not assume a newer compiler unless a decision changes the baseline. | Treat Rust 1.95.0 as the compatibility claim under review.                                                            |
| `Cargo.lock` | No package-level `rust-version` or `rust_version` metadata is present.                                                        | A lockfile-only checker cannot prove dependency MSRV compatibility for this lockfile.     | The optional checker reports the lockfile evidence as inconclusive when no metadata is available.                     |
| `cargo metadata --locked --format-version 1 --no-deps` | Metadata generation completed under the pinned Rust 1.95.0 toolchain.                                                         | The active manifest and lockfile can be read under the current baseline.                  | Treat this as preflight evidence, not a complete dependency MSRV proof.                                               |
| `cargo check --workspace --all-targets --all-features --locked` | The full workspace check completed under the pinned Rust 1.95.0 toolchain from a Visual Studio developer command environment. | The prior Rust 1.85 compatibility claim is retired for this working state.                | Rust 1.95.0 is the active compatibility baseline, while release remains blocked on retained evidence and other gates. |

The prior Rust 1.85 review detected packages above that retired baseline:

| Package | Version | Declared `rust_version` | Source |
| --- | --- | --- | --- |
| `wasip2` | `1.0.3+wasi-0.2.9` | `1.87.0` | crates.io registry |
| `wasip3` | `0.4.0+wasi-0.3.0-rc-2026-01-06` | `1.87.0` | crates.io registry |
| `wit-bindgen` | `0.51.0` | `1.87.0` | crates.io registry |
| `wit-bindgen-core` | `0.51.0` | `1.87.0` | crates.io registry |
| `wit-bindgen-rust` | `0.51.0` | `1.87.0` | crates.io registry |

## Validation

Run the optional lockfile checker:

```bash
python tools/testing/msrv_dependency_check.py
```

Expected result for the current lockfile: inconclusive when `Cargo.lock` does not include package-level `rust-version` metadata.

Run Cargo metadata evidence when local registry metadata is available:

```bash
cargo metadata --locked --offline --format-version 1
```

Expected result for the retired Rust 1.85 snapshot: the richer metadata source detected five registry packages requiring Rust 1.87.0.

Before release, validate the actual claimed baseline with the pinned Rust 1.95.0 toolchain:

```bash
cargo +1.95.0 check --workspace --all-targets --all-features --locked
```

If that command is unavailable locally, record the missing toolchain as a validation gap. Local Windows runs must use a shell where `link.exe` is available, such as the configured Visual Studio developer command environment.

## Troubleshooting

If the checker reports `result=inconclusive`, inspect whether the current Cargo version records dependency `rust-version` metadata in `Cargo.lock`. Do not record an inconclusive result as a pass.

If Cargo metadata reports dependencies above Rust 1.95.0, identify the owning dependency path before changing versions. Target-specific packages can still block a release compatibility claim if the release includes that target, feature set, or generated artifact path.

If the Rust 1.95.0 build gate fails, either pin or downgrade the offending dependency, raise the workspace baseline through governance, or narrow the release claim to the validated toolchain.

If the Rust 1.95.0 toolchain is not installed, install it or run the gate in CI before approving a Rust 1.95.0 compatibility statement.

## References

- `Cargo.toml`
- `Cargo.lock`
- `tools/testing/msrv_dependency_check.py`
- `documentations/governance/supply-chain-policy.md`
- `documentations/governance/adr-backlog-2026-05-08.md`
