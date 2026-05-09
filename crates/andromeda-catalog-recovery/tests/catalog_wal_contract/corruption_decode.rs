use super::support::{catalog_frame_from_payload, catalog_frame_with_declared_payload_tail};
use andromeda_catalog_recovery::{
    CatalogStorageWalRecord as CatalogWalRecord,
    decode_storage_catalog_record as decode_catalog_record,
    encode_storage_catalog_record as encode_catalog_record,
};
use andromeda_types::{CatalogObjectId, CatalogVersion, ContractHash};

#[test]
fn decode_rejects_corrupted_checksum() {
    let original = CatalogWalRecord::ProcedureAdded {
        procedure_id: CatalogObjectId::new(1),
        signature_hash: ContractHash::test_vector(0xCC),
        new_catalog_version: CatalogVersion::new(1),
        timestamp_secs: 1000,
    };
    let mut encoded = encode_catalog_record(&original).expect("encode failed");

    encoded[10] ^= 0xFF;

    let err = decode_catalog_record(&encoded).expect_err("corrupted checksum must be rejected");
    assert!(err.message().contains("checksum mismatch"));
}

#[test]
fn decode_rejects_truncated_frame() {
    let bytes = vec![0x01, 0x00, 0x02, 0x00];
    assert!(decode_catalog_record(&bytes).is_err());
}

#[test]
fn decode_rejects_invalid_record_version() {
    let mut frame = vec![0xFF, 0xFF];
    frame.extend_from_slice(&[0u8; 4]);
    frame.extend_from_slice(&[0u8; 32]);
    frame.push(1u8);

    let err = decode_catalog_record(&frame).expect_err("invalid version must be rejected");
    assert!(
        err.message()
            .contains("unsupported catalog WAL record version")
    );
}

#[test]
fn decode_rejects_trailing_bytes_after_declared_payload() {
    let original = CatalogWalRecord::ProcedureAdded {
        procedure_id: CatalogObjectId::new(1),
        signature_hash: ContractHash::test_vector(0xAC),
        new_catalog_version: CatalogVersion::new(1),
        timestamp_secs: 1000,
    };
    let mut encoded = encode_catalog_record(&original).expect("encode failed");
    encoded.extend_from_slice(b"tail");

    let err = decode_catalog_record(&encoded).expect_err("frame tail must be rejected");
    assert!(err.message().contains("frame length mismatch"));
}

#[test]
fn decode_rejects_trailing_bytes_inside_declared_payload() {
    let original = CatalogWalRecord::ProcedureAdded {
        procedure_id: CatalogObjectId::new(1),
        signature_hash: ContractHash::test_vector(0xAD),
        new_catalog_version: CatalogVersion::new(1),
        timestamp_secs: 1000,
    };
    let encoded = encode_catalog_record(&original).expect("encode failed");
    let frame = catalog_frame_with_declared_payload_tail(encoded, b"semantic-tail");

    let err = decode_catalog_record(&frame).expect_err("semantic payload tail must be rejected");
    assert!(err.message().contains("payload has trailing bytes"));
}

#[test]
fn decode_rejects_definition_batch_count_before_large_allocation() {
    let mut payload = Vec::new();
    payload.push(0u8);
    payload.extend_from_slice(&1u64.to_le_bytes());
    payload.extend_from_slice(&CatalogVersion::new(1).get().to_le_bytes());
    payload.extend_from_slice(&u32::MAX.to_le_bytes());

    let frame = catalog_frame_from_payload(payload);
    let err = decode_catalog_record(&frame).expect_err("procedure count must be bounded");

    assert!(err.message().contains("procedure_count"));
}
