#![no_main]

use andromeda_storage::{
    Lsn, WalRecord, WalRecordKind, decode_wal_record_frame, encode_wal_record,
};
use libfuzzer_sys::fuzz_target;

const MAX_WAL_FRAME_FUZZ_BYTES: usize = 64 * 1024;

fuzz_target!(|data: &[u8]| {
    let frame_data = &data[..data.len().min(MAX_WAL_FRAME_FUZZ_BYTES)];
    if let Ok(Some((record, consumed))) = decode_wal_record_frame(frame_data) {
        let _ = record.validate();
        assert!(consumed <= frame_data.len());
        if let Ok(encoded) = encode_wal_record(&record) {
            let _ = decode_wal_record_frame(&encoded);
        }
    }

    let payload_len = data.len().min(4096);
    let _ = WalRecord::from_parts(
        WalRecordKind::SecurityAuditAppend,
        Lsn::new(1),
        None,
        None,
        data[..payload_len].to_vec(),
    )
    .and_then(|record| {
        let encoded = encode_wal_record(&record)?;
        let _ = decode_wal_record_frame(&encoded)?;
        Ok(record)
    });
});
