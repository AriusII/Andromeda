use andromeda_types::{CatalogObjectId, CatalogVersion, ContractHash};

use andromeda_wal::Lsn;

use crate::CatalogStorageWalRecord as CatalogWalRecord;

use super::{
    decode_storage_catalog_record as decode_catalog_record,
    encode_storage_catalog_record as encode_catalog_record,
};

#[test]
fn encode_decode_definition_batch_applied_round_trip() {
    let original = CatalogWalRecord::DefinitionBatchApplied {
        batch_id: 42,
        new_catalog_version: CatalogVersion::new(10),
        procedure_count: 2,
        affected_procedure_ids: vec![CatalogObjectId::new(100), CatalogObjectId::new(101)],
        timestamp_secs: 1234567890,
        operator_principal: "alice".to_string(),
    };

    let encoded = encode_catalog_record(&original).expect("encode failed");
    let decoded = decode_catalog_record(&encoded).expect("decode failed");

    assert_eq!(original, decoded);
}

#[test]
fn encode_decode_procedure_added_round_trip() {
    let original = CatalogWalRecord::ProcedureAdded {
        procedure_id: CatalogObjectId::new(1),
        signature_hash: ContractHash::test_vector(0xAA),
        new_catalog_version: CatalogVersion::new(5),
        timestamp_secs: 1000,
    };

    let encoded = encode_catalog_record(&original).expect("encode failed");
    let decoded = decode_catalog_record(&encoded).expect("decode failed");

    assert_eq!(original, decoded);
}

#[test]
fn encode_decode_procedure_altered_round_trip() {
    let original = CatalogWalRecord::ProcedureAltered {
        procedure_id: CatalogObjectId::new(2),
        old_hash: ContractHash::test_vector(0x11),
        new_hash: ContractHash::test_vector(0x22),
        new_catalog_version: CatalogVersion::new(6),
        timestamp_secs: 2000,
    };

    let encoded = encode_catalog_record(&original).expect("encode failed");
    let decoded = decode_catalog_record(&encoded).expect("decode failed");

    assert_eq!(original, decoded);
}

#[test]
fn encode_decode_procedure_dropped_round_trip() {
    let original = CatalogWalRecord::ProcedureDropped {
        procedure_id: CatalogObjectId::new(3),
        dropped_version: CatalogVersion::new(5),
        new_catalog_version: CatalogVersion::new(6),
        timestamp_secs: 3000,
    };

    let encoded = encode_catalog_record(&original).expect("encode failed");
    let decoded = decode_catalog_record(&encoded).expect("decode failed");

    assert_eq!(original, decoded);
}

#[test]
fn encode_decode_statistics_updated_round_trip() {
    let original = CatalogWalRecord::StatisticsUpdated {
        stats_version: 1,
        table_id: CatalogObjectId::new(10),
        column_id: CatalogObjectId::new(11),
        histogram_data_lsn: Lsn::new(5000),
        timestamp_secs: 4000,
    };

    let encoded = encode_catalog_record(&original).expect("encode failed");
    let decoded = decode_catalog_record(&encoded).expect("decode failed");

    assert_eq!(original, decoded);
}

#[test]
fn encode_decode_catalog_checkpoint_round_trip() {
    let original = CatalogWalRecord::CatalogCheckpoint {
        checkpoint_lsn: Lsn::new(10000),
        catalog_version: CatalogVersion::new(42),
        visible_procedure_count: 17,
        timestamp_secs: 5000,
    };

    let encoded = encode_catalog_record(&original).expect("encode failed");
    let decoded = decode_catalog_record(&encoded).expect("decode failed");

    assert_eq!(original, decoded);
}

#[test]
fn encode_is_deterministic() {
    let record = CatalogWalRecord::ProcedureAdded {
        procedure_id: CatalogObjectId::new(1),
        signature_hash: ContractHash::test_vector(0xFF),
        new_catalog_version: CatalogVersion::new(1),
        timestamp_secs: 1000,
    };

    let encoded1 = encode_catalog_record(&record).expect("encode 1 failed");
    let encoded2 = encode_catalog_record(&record).expect("encode 2 failed");

    assert_eq!(encoded1, encoded2);
}

#[test]
fn decode_rejects_corrupted_checksum() {
    let original = CatalogWalRecord::ProcedureAdded {
        procedure_id: CatalogObjectId::new(1),
        signature_hash: ContractHash::test_vector(0xAA),
        new_catalog_version: CatalogVersion::new(1),
        timestamp_secs: 1000,
    };

    let mut encoded = encode_catalog_record(&original).expect("encode failed");

    // Corrupt the checksum (bytes 6..38)
    if encoded.len() > 38 {
        encoded[6] ^= 0xFF; // Flip bits in checksum
    }

    let result = decode_catalog_record(&encoded);
    assert!(result.is_err());
    assert!(result.unwrap_err().message().contains("checksum mismatch"));
}

#[test]
fn decode_rejects_truncated_frame() {
    let bytes = vec![0u8; 10]; // Too short
    let result = decode_catalog_record(&bytes);
    assert!(result.is_err());
}

#[test]
fn decode_rejects_invalid_version() {
    let mut frame = vec![99u8, 0u8]; // Invalid version
    frame.extend_from_slice(&[0u8; 4]); // payload_len
    frame.extend_from_slice(&[0u8; 32]); // checksum
    frame.push(0u8); // minimal payload

    let result = decode_catalog_record(&frame);
    assert!(result.is_err());
}

#[test]
fn encode_rejects_invalid_record() {
    // Record with zero procedure count
    let record = CatalogWalRecord::DefinitionBatchApplied {
        batch_id: 1,
        new_catalog_version: CatalogVersion::new(1),
        procedure_count: 0,
        affected_procedure_ids: vec![],
        timestamp_secs: 1000,
        operator_principal: "test".to_string(),
    };

    let result = encode_catalog_record(&record);
    assert!(result.is_err());
}
