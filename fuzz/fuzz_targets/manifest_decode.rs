#![no_main]

use andromeda_storage::{ReplayContext, replay_wal_record};
use andromeda_wal::{Lsn, WalRecord, WalRecordKind};
use libfuzzer_sys::fuzz_target;

mod common;

const MANIFEST_SWITCH_PAYLOAD_LEN: usize = 68;
const MANIFEST_VERSION_OFFSET: usize = 0;
const MANIFEST_CRC_OFFSET: usize = 64;
const DEFAULT_RECORD_LSN: u64 = 128;

fuzz_target!(|data: &[u8]| {
    let payload = common::bounded_input(data, common::MAX_4K_PAYLOAD_BYTES);

    let record_lsn = read_u64_le(data, MANIFEST_SWITCH_PAYLOAD_LEN)
        .filter(|value| *value != 0)
        .unwrap_or(DEFAULT_RECORD_LSN);
    let previous_lsn = record_lsn.checked_sub(1).filter(|value| *value != 0);

    let Ok(record) = WalRecord::from_parts(
        WalRecordKind::ManifestSwitch,
        Lsn::new(record_lsn),
        previous_lsn.map(Lsn::new),
        None,
        payload.to_vec(),
    ) else {
        return;
    };

    let mut ctx = ReplayContext::new();
    ctx.observe_checkpoint_end(record.header.lsn);

    if payload.len() == MANIFEST_SWITCH_PAYLOAD_LEN {
        if let (Some(manifest_version), Some(manifest_crc)) = (
            read_u64_le(payload, MANIFEST_VERSION_OFFSET),
            read_u32_le(payload, MANIFEST_CRC_OFFSET),
        ) {
            if manifest_version != 0 && manifest_crc != 0 {
                ctx.known_manifest_crc_by_version
                    .insert(manifest_version, manifest_crc);
            }
        }
    }

    let _ = replay_wal_record(&mut ctx, &record);
});

fn read_u64_le(data: &[u8], offset: usize) -> Option<u64> {
    let bytes = data.get(offset..offset.checked_add(8)?)?;
    let mut array = [0; 8];
    array.copy_from_slice(bytes);
    Some(u64::from_le_bytes(array))
}

fn read_u32_le(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset.checked_add(4)?)?;
    let mut array = [0; 4];
    array.copy_from_slice(bytes);
    Some(u32::from_le_bytes(array))
}
