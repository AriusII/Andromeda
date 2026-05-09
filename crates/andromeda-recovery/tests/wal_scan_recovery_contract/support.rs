use andromeda_manifest::DatabaseManifest;
use andromeda_types::TransactionId;
use andromeda_wal::{Lsn, WalRecord, WalRecordKind, encode_wal_record};

pub(crate) fn tx_record(lsn: u64, previous_lsn: Option<u64>, payload: &[u8]) -> WalRecord {
    WalRecord::from_parts(
        WalRecordKind::RowInsert,
        Lsn::new(lsn),
        previous_lsn.map(Lsn::new),
        Some(TransactionId::new(42)),
        payload,
    )
    .unwrap()
}

pub(crate) fn record(
    kind: WalRecordKind,
    lsn: u64,
    previous_lsn: Option<u64>,
    transaction_id: Option<u64>,
    payload: &[u8],
) -> WalRecord {
    WalRecord::from_parts(
        kind,
        Lsn::new(lsn),
        previous_lsn.map(Lsn::new),
        transaction_id.map(TransactionId::new),
        payload,
    )
    .unwrap()
}

pub(crate) fn encode_records(records: &[WalRecord]) -> Vec<u8> {
    let mut encoded = Vec::new();
    for record in records {
        encoded.extend(encode_wal_record(record).unwrap());
    }
    encoded
}

pub(crate) fn manifest(required_wal_start_lsn: Lsn) -> DatabaseManifest {
    DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: required_wal_start_lsn,
        required_wal_start_lsn,
        previous_manifest_hash: [0; 32],
        manifest_crc: 7,
    }
}
