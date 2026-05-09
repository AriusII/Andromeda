#![allow(dead_code)]

use andromeda_catalog_recovery::{
    CatalogStorageWalRecord as CatalogWalRecord,
    encode_storage_catalog_record as encode_catalog_record,
};
use andromeda_types::{CatalogObjectId, CatalogVersion, ContractHash, TransactionId};
use andromeda_wal::{Lsn, WalRecord, WalRecordKind};
use sha2::{Digest, Sha256};

pub fn catalog_storage_record(
    kind: WalRecordKind,
    lsn: u64,
    previous_lsn: Option<u64>,
    transaction_id: u64,
    payload: &[u8],
) -> WalRecord {
    WalRecord::from_parts(
        kind,
        Lsn::new(lsn),
        previous_lsn.map(Lsn::new),
        Some(TransactionId::new(transaction_id)),
        payload.to_vec(),
    )
    .expect("catalog storage WAL record")
}

pub fn catalog_frame_from_payload(payload: Vec<u8>) -> Vec<u8> {
    let mut frame = Vec::new();
    frame.extend_from_slice(&1u16.to_le_bytes());
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    let checksum = Sha256::digest(&payload);
    frame.extend_from_slice(&checksum);
    frame.extend_from_slice(&payload);
    frame
}

pub fn catalog_frame_with_declared_payload_tail(encoded: Vec<u8>, tail: &[u8]) -> Vec<u8> {
    assert!(encoded.len() >= 38);
    let mut payload = encoded[38..].to_vec();
    payload.extend_from_slice(tail);
    catalog_frame_from_payload(payload)
}

pub fn procedure_added_record(procedure_id: u64, catalog_version: u64) -> CatalogWalRecord {
    CatalogWalRecord::ProcedureAdded {
        procedure_id: CatalogObjectId::new(procedure_id),
        signature_hash: ContractHash::test_vector(procedure_id as u8),
        new_catalog_version: CatalogVersion::new(catalog_version),
        timestamp_secs: 1704067300 + catalog_version,
    }
}

pub fn encode_catalog_apply_payload(record: &CatalogWalRecord) -> Vec<u8> {
    encode_catalog_record(record).expect("encode catalog apply payload")
}
