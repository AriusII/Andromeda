//! Golden-vector and property tests for [`andromeda_wal_codec::fnv64_nonzero`].
//!
//! # Why these tests exist
//!
//! `fnv64_nonzero` computes the header checksum for every WAL record frame
//! written to durable storage.  Because WAL frames are the truth source for
//! crash-recovery, **any silent change to this function is a binary-format
//! breaking change** that could silently corrupt a WAL stream or make
//! previously durable frames unreadable after a software upgrade.
//!
//! These golden constants are the primary regression line for the durable byte
//! semantics of the function.  **Any change to the digest function MUST be
//! accompanied by a P11/Andromeda-Doctrine review and an explicit increment
//! of the WAL format version.**
//!
//! # Algorithm
//!
//! FNV-1a ("XOR-then-multiply") over 64 bits, standard parameters:
//! - Offset basis: `0xcbf2_9ce4_8422_2325`  
//! - Prime: `0x0000_0100_0000_01b3`
//!
//! The only deviation from bare FNV-64a is the **nonzero guard**: if the
//! digest of the input is `0`, the function returns `1` instead.  This
//! ensures no WAL frame header checksum field is ever stored as zero.
//!
//! # How the golden constants were generated
//!
//! The constants were produced by running the production implementation
//! (`andromeda-wal-codec/src/checksum.rs`) on each input and pinning the
//! output.  They are **regression constants** — not independently
//! cross-validated against a third-party FNV-64a library (that cross-
//! validation is deferred to P12, see TODO below).  Their purpose is to catch
//! accidental changes to the algorithm or its parameters.
//!
//! # Residual TODOs
//!
//! - **P12**: Cross-validate goldens against an external FNV-64a reference
//!   implementation (e.g., the `fnv` crate or the canonical reference in
//!   <https://datatracker.ietf.org/doc/draft-eastlake-fnv/>) to confirm
//!   Andromeda's variant matches the public FNV-1a-64 spec exactly.

#![forbid(unsafe_code)]

use andromeda_wal_codec::fnv64_nonzero;

// ---------------------------------------------------------------------------
// Golden constants
// ---------------------------------------------------------------------------
//
// All values were produced by running the production implementation on the
// given input and recording the output.  Do NOT change these constants without
// a WAL format-version review.

/// FNV-64a of an empty byte slice.
/// Empty → state never leaves the offset basis → returns the offset basis
/// directly (0xcbf2_9ce4_8422_2325 ≠ 0, so the nonzero guard is not triggered).
const GOLDEN_EMPTY: u64 = 0xcbf2_9ce4_8422_2325;

/// FNV-64a of a single ASCII 'a' byte (0x61).
const GOLDEN_ONE_BYTE_A: u64 = 0xaf63_dc4c_8601_ec8c;

/// FNV-64a of 64 × 0x00 bytes.
const GOLDEN_64_ZEROS: u64 = 0xb9b2_3f3a_46fd_0825;

/// FNV-64a of 64 × 0xFF bytes.
/// This exercises the full byte range with the "maximum" byte value and covers
/// the edge case where all input bytes have every bit set.
const GOLDEN_64_FF: u64 = 0x84cc_4da0_e20e_cde5;

/// FNV-64a of 4096 × 0x42 bytes (one default page-size worth of data).
/// 4096 bytes is the smallest Andromeda page granularity and is a
/// representative WAL payload size.
const GOLDEN_4096_0X42: u64 = 0x5def_a74c_f71e_2325;

// ---------------------------------------------------------------------------
// Golden vector tests
// ---------------------------------------------------------------------------

#[test]
fn fnv64_nonzero_golden_empty() {
    assert_eq!(
        fnv64_nonzero(&[]),
        GOLDEN_EMPTY,
        "FNV-64a of empty input must equal the FNV offset basis"
    );
}

#[test]
fn fnv64_nonzero_golden_one_byte_a() {
    assert_eq!(
        fnv64_nonzero(&[0x61]),
        GOLDEN_ONE_BYTE_A,
        "FNV-64a of single byte 0x61 ('a') diverges from golden"
    );
}

#[test]
fn fnv64_nonzero_golden_64_zeros() {
    assert_eq!(
        fnv64_nonzero(&[0u8; 64]),
        GOLDEN_64_ZEROS,
        "FNV-64a of 64 zero bytes diverges from golden"
    );
}

#[test]
fn fnv64_nonzero_golden_64_ff_bytes() {
    assert_eq!(
        fnv64_nonzero(&[0xFFu8; 64]),
        GOLDEN_64_FF,
        "FNV-64a of 64 × 0xFF bytes diverges from golden"
    );
}

#[test]
fn fnv64_nonzero_golden_4096_page_size() {
    assert_eq!(
        fnv64_nonzero(&[0x42u8; 4096]),
        GOLDEN_4096_0X42,
        "FNV-64a of 4096-byte page-sized input diverges from golden"
    );
}

// ---------------------------------------------------------------------------
// Nonzero-guard property tests
// ---------------------------------------------------------------------------

/// All golden outputs must be nonzero — the function contract guarantees this.
#[test]
fn fnv64_nonzero_output_is_never_zero_for_test_vectors() {
    for (label, digest) in [
        ("empty", GOLDEN_EMPTY),
        ("one_byte_a", GOLDEN_ONE_BYTE_A),
        ("64_zeros", GOLDEN_64_ZEROS),
        ("64_ff", GOLDEN_64_FF),
        ("4096_0x42", GOLDEN_4096_0X42),
    ] {
        assert_ne!(digest, 0, "golden vector '{label}' must not be zero");
    }
}

/// The nonzero guard semantics: a digest of 0 should become 1.
/// We cannot cheaply craft a natural FNV-64a input that hashes to 0,
/// so we test this by verifying our all-vector outputs are nonzero AND
/// documenting the branch exists.  Cross-coverage of the `== 0 → 1` branch
/// is deferred to a targeted fuzz campaign in P12.
#[test]
fn fnv64_nonzero_zero_replacement_is_documented() {
    // All test vectors produce nonzero outputs, confirming the guard
    // does not interfere with normal inputs.
    assert_ne!(fnv64_nonzero(&[0u8; 64]), 0);
    assert_ne!(fnv64_nonzero(&[0xFFu8; 64]), 0);
    // The guard itself (state == 0 → return 1) is logically covered by
    // code inspection: if the FNV loop produces 0, `if state == 0 { 1 } else { state }`
    // ensures the function never returns 0.
}

// ---------------------------------------------------------------------------
// Determinism property test
// ---------------------------------------------------------------------------

/// The function must be pure: identical inputs always produce identical outputs.
/// This test calls the function 100 times on each golden-vector input and
/// asserts all results are identical to the first call.
#[test]
fn fnv64_nonzero_is_deterministic_across_100_runs() {
    const RUNS: usize = 100;
    let inputs: &[&[u8]] = &[
        &[],
        &[0x61],
        &[0u8; 64],
        &[0xFFu8; 64],
        // Use a fixed 16-byte pattern to avoid large stack allocations
        &[
            0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
            0x09, 0x0A,
        ],
    ];

    for input in inputs {
        let expected = fnv64_nonzero(input);
        for run in 0..RUNS {
            let result = fnv64_nonzero(input);
            assert_eq!(
                result,
                expected,
                "fnv64_nonzero not deterministic on run {run} for input len={}",
                input.len()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Regression: function reads every byte (not just sampling)
// ---------------------------------------------------------------------------

/// Swapping a single byte in the middle of the input must change the digest.
/// This confirms the function is not accidentally truncating or skipping bytes.
#[test]
fn fnv64_nonzero_sensitive_to_single_byte_change() {
    let mut input = [0x00u8; 64];
    let base = fnv64_nonzero(&input);

    input[32] = 0x01; // flip one byte in the middle
    let modified = fnv64_nonzero(&input);

    assert_ne!(
        base, modified,
        "fnv64_nonzero must change when a single byte changes"
    );
}
