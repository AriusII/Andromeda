//! Common test utilities and fixtures for recovery tests.

use andromeda_manifest::ManifestDurabilityBoundary;
use andromeda_wal::Lsn;

/// Builds a standard manifest for testing.
pub fn build_test_manifest(
    base_checkpoint_lsn: u64,
    required_wal_start_lsn: u64,
) -> ManifestDurabilityBoundary {
    ManifestDurabilityBoundary {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: Lsn::new(base_checkpoint_lsn),
        required_wal_start_lsn: Lsn::new(required_wal_start_lsn),
        previous_manifest_hash: [0; 32],
        manifest_crc: 0x12345678,
    }
}

/// Builds a bootstrap manifest (empty database).
#[allow(dead_code)]
pub fn build_bootstrap_manifest() -> ManifestDurabilityBoundary {
    build_test_manifest(0, 0)
}

/// Validates a manifest without recovery errors.
pub fn assert_valid_manifest(manifest: &ManifestDurabilityBoundary) {
    assert!(
        manifest.validate().is_ok(),
        "Manifest validation failed: {:?}",
        manifest
    );
}

/// Asserts recovery can start at LSN.
pub fn assert_can_recover_at(manifest: &ManifestDurabilityBoundary, lsn: u64) {
    assert!(
        manifest.can_start_recovery_at(Lsn::new(lsn)),
        "Cannot recover at LSN {}: manifest floor is {}",
        lsn,
        manifest.recovery_floor_lsn().get()
    );
}

/// Asserts recovery cannot start at LSN (before recovery floor).
#[allow(dead_code)]
pub fn assert_cannot_recover_at(manifest: &ManifestDurabilityBoundary, lsn: u64) {
    assert!(
        !manifest.can_start_recovery_at(Lsn::new(lsn)),
        "Recovery should not be allowed at LSN {}: manifest floor is {}",
        lsn,
        manifest.recovery_floor_lsn().get()
    );
}

/// Verifies LSN ordering is correct.
#[allow(dead_code)]
pub fn assert_lsn_monotonic(lsns: &[u64]) {
    for i in 0..lsns.len().saturating_sub(1) {
        assert!(
            lsns[i] < lsns[i + 1],
            "LSN ordering violated at index {}: {} >= {}",
            i,
            lsns[i],
            lsns[i + 1]
        );
    }
}

/// Verifies no gaps in LSN sequence.
#[allow(dead_code)]
pub fn assert_lsn_continuous(start: u64, lsns: &[u64]) {
    for (idx, &lsn) in lsns.iter().enumerate() {
        assert_eq!(
            lsn,
            start + idx as u64,
            "LSN gap at index {}: expected {}, got {}",
            idx,
            start + idx as u64,
            lsn
        );
    }
}
