#![no_main]

use andromeda_storage::{
    decode_wal_record_frame, encode_wal_record, Lsn, WalRecord, WalRecordKind,
};
use libfuzzer_sys::fuzz_target;

mod common;

fuzz_target!(|data: &[u8]| {
    let frame_data = common::bounded_input(data, common::MAX_64K_INPUT_BYTES);
    if let Ok(Some((record, consumed))) = decode_wal_record_frame(frame_data) {
        let _ = record.validate();
        assert!(consumed <= frame_data.len());
        if let Ok(encoded) = encode_wal_record(&record) {
            let _ = decode_wal_record_frame(&encoded);
        }
    }

    let _ = WalRecord::from_parts(
        WalRecordKind::SecurityAuditAppend,
        Lsn::new(1),
        None,
        None,
        common::bounded_input(data, common::MAX_4K_PAYLOAD_BYTES).to_vec(),
    )
    .and_then(|record| {
        let encoded = encode_wal_record(&record)?;
        let _ = decode_wal_record_frame(&encoded)?;
        Ok(record)
    });
});
