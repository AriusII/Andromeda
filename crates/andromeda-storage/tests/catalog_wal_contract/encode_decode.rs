use andromeda_core::{CatalogObjectId, CatalogVersion, ContractHash};
use andromeda_storage::{CatalogWalRecord, Lsn, decode_catalog_record, encode_catalog_record};

#[test]
fn definition_batch_applied_preserves_all_fields() {
    let original = CatalogWalRecord::DefinitionBatchApplied {
        batch_id: 123,
        new_catalog_version: CatalogVersion::new(10),
        procedure_count: 3,
        affected_procedure_ids: vec![
            CatalogObjectId::new(100),
            CatalogObjectId::new(101),
            CatalogObjectId::new(102),
        ],
        timestamp_secs: 1704067200,
        operator_principal: "alice@example.com".to_string(),
    };

    let encoded = encode_catalog_record(&original).expect("encode failed");
    let decoded = decode_catalog_record(&encoded).expect("decode failed");

    assert_eq!(original, decoded);
}

#[test]
fn procedure_added_round_trip() {
    let original = CatalogWalRecord::ProcedureAdded {
        procedure_id: CatalogObjectId::new(42),
        signature_hash: ContractHash::test_vector(0xAB),
        new_catalog_version: CatalogVersion::new(5),
        timestamp_secs: 1704067201,
    };

    let encoded = encode_catalog_record(&original).expect("encode failed");
    let decoded = decode_catalog_record(&encoded).expect("decode failed");

    assert_eq!(original, decoded);
}

#[test]
fn procedure_altered_with_different_hashes_round_trip() {
    let original = CatalogWalRecord::ProcedureAltered {
        procedure_id: CatalogObjectId::new(7),
        old_hash: ContractHash::test_vector(0x11),
        new_hash: ContractHash::test_vector(0x22),
        new_catalog_version: CatalogVersion::new(3),
        timestamp_secs: 1704067202,
    };

    let encoded = encode_catalog_record(&original).expect("encode failed");
    let decoded = decode_catalog_record(&encoded).expect("decode failed");

    assert_eq!(original, decoded);
}

#[test]
fn procedure_dropped_round_trip() {
    let original = CatalogWalRecord::ProcedureDropped {
        procedure_id: CatalogObjectId::new(9),
        dropped_version: CatalogVersion::new(2),
        new_catalog_version: CatalogVersion::new(4),
        timestamp_secs: 1704067203,
    };

    let encoded = encode_catalog_record(&original).expect("encode failed");
    let decoded = decode_catalog_record(&encoded).expect("decode failed");

    assert_eq!(original, decoded);
}

#[test]
fn statistics_updated_round_trip() {
    let original = CatalogWalRecord::StatisticsUpdated {
        stats_version: 15,
        table_id: CatalogObjectId::new(200),
        column_id: CatalogObjectId::new(201),
        histogram_data_lsn: Lsn::new(50000),
        timestamp_secs: 1704067204,
    };

    let encoded = encode_catalog_record(&original).expect("encode failed");
    let decoded = decode_catalog_record(&encoded).expect("decode failed");

    assert_eq!(original, decoded);
}

#[test]
fn catalog_checkpoint_round_trip() {
    let original = CatalogWalRecord::CatalogCheckpoint {
        checkpoint_lsn: Lsn::new(100000),
        catalog_version: CatalogVersion::new(20),
        visible_procedure_count: 42,
        timestamp_secs: 1704067205,
    };

    let encoded = encode_catalog_record(&original).expect("encode failed");
    let decoded = decode_catalog_record(&encoded).expect("decode failed");

    assert_eq!(original, decoded);
}

#[test]
fn unicode_operator_principal_round_trip() {
    let original = CatalogWalRecord::DefinitionBatchApplied {
        batch_id: 99,
        new_catalog_version: CatalogVersion::new(1),
        procedure_count: 1,
        affected_procedure_ids: vec![CatalogObjectId::new(1)],
        timestamp_secs: 1704067206,
        operator_principal: "müller@café.com".to_string(),
    };

    let encoded = encode_catalog_record(&original).expect("encode failed");
    let decoded = decode_catalog_record(&encoded).expect("decode failed");

    assert_eq!(original, decoded);
}

#[test]
fn large_batch_with_many_procedures_round_trip() {
    let procedure_ids = (0..1000).map(|i| CatalogObjectId::new(1000 + i)).collect();
    let original = CatalogWalRecord::DefinitionBatchApplied {
        batch_id: 500,
        new_catalog_version: CatalogVersion::new(100),
        procedure_count: 1000,
        affected_procedure_ids: procedure_ids,
        timestamp_secs: 1704067207,
        operator_principal: "bulk_operator".to_string(),
    };

    let encoded = encode_catalog_record(&original).expect("encode failed");
    let decoded = decode_catalog_record(&encoded).expect("decode failed");

    assert_eq!(original, decoded);
}
