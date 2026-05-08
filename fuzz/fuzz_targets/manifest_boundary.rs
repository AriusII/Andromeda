#![no_main]
#![forbid(unsafe_code)]

use andromeda_manifest::{
    validate_manifest_atomic_switch, validate_recovery_floor, ManifestDurabilityBoundary,
};
use andromeda_wal::Lsn;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Require minimum data size for meaningful fuzzing
    if data.len() < 8 {
        return;
    }

    // Extract fuzzer-controlled LSN values
    let manifest_checkpoint_lsn = Lsn::new(read_u64_le(data, 0));
    let wal_checkpoint_lsn = Lsn::new(read_u64_le(data, 8));
    let wal_durable_lsn = Lsn::new(read_u64_le(data, 16));
    let recovery_floor_lsn = Lsn::new(read_u64_le(data, 24));
    let required_wal_start_lsn = Lsn::new(read_u64_le(data, 32));

    // Test atomic switch validation
    let _ = validate_manifest_atomic_switch(manifest_checkpoint_lsn, wal_durable_lsn, wal_checkpoint_lsn);

    // Test recovery floor validation
    let _ = validate_recovery_floor(recovery_floor_lsn, required_wal_start_lsn);

    // Test manifest boundary if we have enough data
    if data.len() >= 104 {
        // 104 bytes = 6 u64s + 32 bytes hash + 4 bytes crc + 4 bytes padding
        let database_id = read_u64_le(data, 40);
        let manifest_version = read_u64_le(data, 48);
        let snapshot_id = read_u64_le(data, 56);
        let manifest_crc = read_u32_le(data, 64);

        // Extract hash from data
        let mut hash = [0u8; 32];
        if let Some(hash_bytes) = data.get(68..100) {
            hash.copy_from_slice(hash_bytes);
        }

        let boundary = ManifestDurabilityBoundary {
            database_id,
            manifest_version,
            snapshot_id,
            base_checkpoint_lsn: manifest_checkpoint_lsn,
            required_wal_start_lsn,
            previous_manifest_hash: hash,
            manifest_crc,
        };

        // Test validation and methods
        let _ = boundary.validate();
        let _ = boundary.recovery_floor_lsn();
        let _ = boundary.checkpoint_lsn();
        let _ = boundary.can_start_recovery_at(manifest_checkpoint_lsn);
    }
});

fn read_u64_le(data: &[u8], offset: usize) -> u64 {
    data.get(offset..offset.checked_add(8).unwrap_or(data.len()))
        .and_then(|bytes| {
            let mut array = [0; 8];
            if bytes.len() == 8 {
                array.copy_from_slice(bytes);
                Some(u64::from_le_bytes(array))
            } else {
                None
            }
        })
        .unwrap_or(0)
}

fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    data.get(offset..offset.checked_add(4).unwrap_or(data.len()))
        .and_then(|bytes| {
            let mut array = [0; 4];
            if bytes.len() == 4 {
                array.copy_from_slice(bytes);
                Some(u32::from_le_bytes(array))
            } else {
                None
            }
        })
        .unwrap_or(0)
}
