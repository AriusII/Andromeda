#![no_main]
use libfuzzer_sys::fuzz_target;
use andromeda_manifest::ManifestDurabilityBoundary;
use andromeda_wal::Lsn;

fuzz_target!(|data: &[u8]| {
    if data.len() < 48 {
        return;
    }

    // Parse manifest fields from fuzz data
    let database_id = u64::from_le_bytes(data[0..8].try_into().unwrap_or([0; 8]));
    let manifest_version = u64::from_le_bytes(data[8..16].try_into().unwrap_or([0; 8]));
    let snapshot_id = u64::from_le_bytes(data[16..24].try_into().unwrap_or([0; 8]));
    let base_checkpoint_lsn =
        Lsn::new(u64::from_le_bytes(data[24..32].try_into().unwrap_or([0; 8])));
    let required_wal_start_lsn =
        Lsn::new(u64::from_le_bytes(data[32..40].try_into().unwrap_or([0; 8])));
    let manifest_crc = u32::from_le_bytes(data[40..44].try_into().unwrap_or([0; 4]));

    let mut previous_manifest_hash = [0u8; 32];
    if data.len() >= 76 {
        previous_manifest_hash.copy_from_slice(&data[44..76]);
    }

    let manifest = ManifestDurabilityBoundary {
        database_id,
        manifest_version,
        snapshot_id,
        base_checkpoint_lsn,
        required_wal_start_lsn,
        previous_manifest_hash,
        manifest_crc,
    };

    // Fuzz properties
    if manifest.validate().is_ok() {
        // If valid, verify invariants
        let floor = manifest.recovery_floor_lsn();

        // Floor must equal required WAL start
        assert_eq!(floor.get(), required_wal_start_lsn.get());

        // Checkpoint must be <= floor
        assert!(manifest.checkpoint_lsn().get() <= floor.get());

        // Can recover at floor
        assert!(manifest.can_start_recovery_at(floor));
    }
});
