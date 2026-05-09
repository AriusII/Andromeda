use andromeda_manifest::DatabaseManifest;
use andromeda_wal::{Lsn, WalRecord, encode_wal_record};

pub(crate) fn recovery_manifest(
    base_checkpoint_lsn: Lsn,
    required_wal_start_lsn: Lsn,
) -> DatabaseManifest {
    DatabaseManifest {
        database_id: 1,
        manifest_version: 2,
        snapshot_id: 3,
        base_checkpoint_lsn,
        required_wal_start_lsn,
        previous_manifest_hash: [0; 32],
        manifest_crc: 99,
    }
}

pub(crate) fn encode_records(records: &[WalRecord]) -> Vec<u8> {
    let mut encoded = Vec::new();
    for record in records {
        encoded.extend(encode_wal_record(record).unwrap());
    }
    encoded
}
