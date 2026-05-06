#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = andromeda_storage::WalRecord::from_parts(
        andromeda_storage::WalRecordKind::SecurityAuditAppend,
        andromeda_storage::Lsn::new(1),
        None,
        None,
        data.to_vec(),
    )
    .and_then(|record| record.validate().map(|_| record));
});
