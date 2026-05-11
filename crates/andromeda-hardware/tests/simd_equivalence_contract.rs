//! Equivalence-test contract for CPU SIMD acceleration — P11 baseline.
//!
//! # Purpose
//!
//! This module documents and enforces the **scalar-vs-accelerated equivalence
//! contract** that every future SIMD kernel addition MUST satisfy.
//!
//! > *Every accelerated kernel variant MUST produce bit-for-bit identical
//! > output to the scalar reference implementation for every possible input.*
//!
//! When P12+ phases add real SIMD kernels (AVX-2, AVX-512, NEON, SVE2), they
//! MUST ship a test file in the form of this one that:
//!
//! 1. Defines a `scalar_reference` function (safe Rust, no intrinsics).
//! 2. Defines one or more `accelerated_*` functions (the SIMD variants).
//! 3. Asserts `scalar_reference(input) == accelerated_*(input)` for:
//!    - An empty input slice.
//!    - A 1-element slice.
//!    - A 64-element slice (L1-cache-line typical).
//!    - A 4096-element slice (page-size typical).
//!    - An all-zero slice, an all-0xFF slice, and a mixed-pattern slice.
//! 4. Asserts that if the host does not support the required CPU feature,
//!    [`CpuKernelRegistry::detect_and_dispatch`] selects
//!    [`CpuKernelVariant::Scalar`].
//!
//! # Current state (P11 baseline)
//!
//! No accelerated kernels exist yet.  The `fake_accelerated_for_equivalence`
//! function below is a **placeholder** — intentionally identical to the scalar
//! reference — so that the test infrastructure is in place and CI validates
//! the pattern before any real SIMD code is merged.
//!
//! # Note on file naming (Windows UAC heuristic)
//!
//! This file is named `simd_equivalence_contract.rs` rather than
//! `kernel_dispatch_pattern.rs` or `cpu_dispatch_pattern.rs` because Windows
//! Application Compatibility flags any test executable whose filename contains:
//! - the word "kernel" (device-driver heuristic), or
//! - the substring "patch" (installer heuristic — present inside "dispatch").
//!
//! Both trigger OS error 740 (elevation required), preventing the test binary
//! from running.  The stubs `kernel_dispatch_pattern.rs` and
//! `cpu_dispatch_pattern.rs` are kept in the repository for documentation.
//!
//! # Files to update when adding real SIMD
//!
//! - Replace `fake_accelerated_for_equivalence` with the real intrinsic-backed
//!   implementation (unsafe internally, exposed behind a safe feature-check
//!   wrapper).
//! - Update this file to import the real function and assert equivalence.
//! - Add the variant to [`CpuKernelRegistry`] variants once the fn-pointer
//!   table is wired (deferred per P11 audit finding A2).
//! - Do NOT use "kernel", "patch", "setup", or "install" in the new filename.

#![forbid(unsafe_code)]

use andromeda_hardware::{CpuKernelKind, CpuKernelRegistry, CpuKernelVariant, CpuRuntimeProfile};

// ===========================================================================
// Reference kernel implementations
// ===========================================================================
//
// CANONICAL reference implementations for equivalence testing.  Written in
// plain, readable safe Rust.
//
// IMPORTANT: Do NOT optimise these functions.  Their role is to be obviously
// correct, not fast.  The accelerated variants must match them.

/// Scalar reference: sum of XOR-folded 4-byte little-endian chunks.
///
/// Remaining bytes (tail) are XOR-folded one byte at a time.  The accumulator
/// wraps on overflow.
///
/// This is the "ground truth" for the equivalence test.  Any accelerated
/// variant (AVX-2, NEON, …) MUST return the same `u32` for every input.
fn scalar_reference_xor_sum(data: &[u8]) -> u32 {
    let mut acc: u32 = 0;
    let chunks = data.chunks_exact(4);
    let tail = chunks.remainder();
    for chunk in chunks {
        acc = acc.wrapping_add(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    for (i, &byte) in tail.iter().enumerate() {
        acc ^= (u32::from(byte)) << (8 * i);
    }
    acc
}

/// Fake accelerated implementation — placeholder for the P12+ real variant.
///
/// This function is **intentionally identical** to `scalar_reference_xor_sum`.
/// Its sole purpose is to demonstrate the equivalence-test shape so that CI
/// validates the infrastructure before any real SIMD code is merged.
///
/// **In P12+: replace this body with the real intrinsic implementation and
/// remove this doc comment.**
fn fake_accelerated_for_equivalence(data: &[u8]) -> u32 {
    // PLACEHOLDER — replace with real SIMD implementation in P12.
    scalar_reference_xor_sum(data)
}

// ===========================================================================
// Equivalence-test vectors
// ===========================================================================
//
// These vectors MUST be used in equivalence tests for every real SIMD kernel
// merged in P12+.  They cover:
//   - empty input (boundary / zero-length)
//   - 1 element  (below any SIMD lane width)
//   - 64 bytes   (L1 cache line, typical SIMD batch size)
//   - all-zero bytes  (zero-fold path)
//   - all-0xFF bytes  (max-value path)
//   - mixed pattern   (realistic WAL/page data)

const EQUIV_VECTORS: &[(&str, &[u8])] = &[
    ("empty", &[]),
    ("one_byte", &[0x42u8]),
    (
        "64_bytes_ascending",
        &[
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
            0x0F, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C,
            0x1D, 0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A,
            0x2B, 0x2C, 0x2D, 0x2E, 0x2F, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38,
            0x39, 0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x40,
        ],
    ),
    ("32_zeros", &[0u8; 32]),
    (
        "32_max_bytes",
        &[
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF,
        ],
    ),
    (
        "mixed_16_deadbeef",
        &[
            0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE, 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB,
            0xCD, 0xEF,
        ],
    ),
];

// ===========================================================================
// Equivalence tests
// ===========================================================================

/// **Core pattern test**: scalar and accelerated produce identical output for
/// all equivalence vectors.
///
/// This is the EXACT SHAPE that every P12+ SIMD equivalence test MUST follow.
/// Replace `fake_accelerated_for_equivalence` with the real accelerated
/// function to validate a SIMD variant.
#[test]
fn scalar_and_accelerated_produce_identical_output() {
    for (label, data) in EQUIV_VECTORS {
        let scalar = scalar_reference_xor_sum(data);
        let accel = fake_accelerated_for_equivalence(data);
        assert_eq!(
            scalar, accel,
            "EQUIVALENCE FAILURE on vector '{label}': \
             scalar={scalar:#010x} vs accelerated={accel:#010x}. \
             Every SIMD kernel MUST produce bit-for-bit identical output to \
             the scalar reference."
        );
    }
}

/// The 4096-byte (page-size) input is tested explicitly; it exercises the
/// full SIMD batch path and is the most common WAL/page payload size.
#[test]
fn equivalence_holds_for_page_size_input() {
    let data = vec![0xA5u8; 4096];
    let scalar = scalar_reference_xor_sum(&data);
    let accel = fake_accelerated_for_equivalence(&data);
    assert_eq!(
        scalar, accel,
        "equivalence must hold for 4096-byte page-size input"
    );
}

/// Scalar reference is deterministic (pure function, no hidden state).
#[test]
fn scalar_reference_is_deterministic() {
    const RUNS: usize = 100;
    for (label, data) in EQUIV_VECTORS {
        let expected = scalar_reference_xor_sum(data);
        for run in 0..RUNS {
            assert_eq!(
                scalar_reference_xor_sum(data),
                expected,
                "scalar not deterministic on run {run} for '{label}'"
            );
        }
    }
}

/// Accelerated variant is deterministic (pure function, no hidden state).
#[test]
fn accelerated_variant_is_deterministic() {
    const RUNS: usize = 100;
    for (label, data) in EQUIV_VECTORS {
        let expected = fake_accelerated_for_equivalence(data);
        for run in 0..RUNS {
            assert_eq!(
                fake_accelerated_for_equivalence(data),
                expected,
                "accelerated not deterministic on run {run} for '{label}'"
            );
        }
    }
}

// ===========================================================================
// CpuKernelRegistry dispatch tests
// ===========================================================================

/// `CpuKernelRegistry::v0()` resolves a valid variant for every kernel kind.
/// Conservative profile guarantees a Scalar fallback.
#[test]
fn registry_v0_resolves_variant_for_every_kernel_kind() {
    let registry = CpuKernelRegistry::v0();
    for kind in [
        CpuKernelKind::Hash,
        CpuKernelKind::Crc32,
        CpuKernelKind::Compression,
        CpuKernelKind::Scan,
    ] {
        let d = registry.dispatch_for_runtime_profile(kind, CpuRuntimeProfile::Conservative);
        assert_eq!(
            d.selected_variant,
            CpuKernelVariant::Scalar,
            "Conservative profile must resolve to Scalar for {kind:?}"
        );
        assert!(
            d.uses_scalar_fallback(),
            "uses_scalar_fallback() must be true for {kind:?}"
        );
        assert_eq!(
            d.scalar_fallback_variant,
            CpuKernelVariant::Scalar,
            "scalar_fallback_variant must be Scalar for {kind:?}"
        );
    }
}

/// Conservative profile → `Scalar` for the Hash kernel family.
#[test]
fn registry_selects_scalar_when_no_acceleration_available() {
    let d = CpuKernelRegistry::v0()
        .dispatch_for_runtime_profile(CpuKernelKind::Hash, CpuRuntimeProfile::Conservative);
    assert_eq!(d.selected_variant, CpuKernelVariant::Scalar);
}

/// `detect_and_dispatch` always marks runtime detection and retains Scalar
/// as the declared fallback, regardless of the selected variant.
#[test]
fn detect_and_dispatch_always_provides_scalar_fallback() {
    for kind in [
        CpuKernelKind::Hash,
        CpuKernelKind::Crc32,
        CpuKernelKind::Compression,
        CpuKernelKind::Scan,
    ] {
        let d = CpuKernelRegistry::v0().detect_and_dispatch(kind);
        assert!(d.runtime_detected, "runtime_detected must be true for {kind:?}");
        assert_eq!(
            d.scalar_fallback_variant,
            CpuKernelVariant::Scalar,
            "scalar_fallback_variant must be Scalar for {kind:?}"
        );
        let variants = CpuKernelRegistry::v0().variants(kind);
        assert!(
            variants.contains(&d.selected_variant),
            "selected_variant {:?} must be in registered variants for {kind:?}",
            d.selected_variant
        );
    }
}

// ===========================================================================
// P12 SIMD addition checklist (authoritative reference, reviewed in CI)
// ===========================================================================
//
// When adding a real SIMD kernel in P12+:
//
//   1. Implement `fn scalar_reference_<name>(data: &[u8]) -> T` — pure safe Rust.
//   2. Implement `fn accelerated_<name>_<arch>(data: &[u8]) -> T` — unsafe
//      intrinsics MUST be behind a safe wrapper that checks CPU features at
//      runtime (e.g. `std::arch::is_x86_feature_detected!`).
//   3. Add a test matching this file's shape:
//        - All EQUIV_VECTORS asserted: scalar == accelerated.
//        - Determinism test for accelerated.
//        - Test that detect_and_dispatch selects accelerated on a host with
//          the required CPU feature.
//   4. Wire the fn-pointer into CpuKernelRegistry (A2 follow-up item).
//   5. Update golden-vector tests for the affected digest function with an
//      accelerated-path golden vector.
//   6. Do NOT use "kernel", "patch" (or "dispatch"), "setup", or "install"
//      in the test filename — Windows UAC will block the binary (error 740).
//
// ENFORCEMENT: A PR that adds a SIMD kernel without a passing equivalence
// test MUST NOT be merged.
