//! Comprehensive contract tests for catalog WAL record encoding and recovery.
//!
//! Total tests: 21
//!
//! Categories:
//! - Encode/decode round-trip tests (8 tests)
//! - Version monotonicity and ordering tests (3 tests)
//! - Procedure ID validation tests (3 tests)
//! - Checksum and corruption tests (3 tests)
//! - Recovery replay tests (4 tests)

#[cfg(test)]
mod tests {
    use andromeda_core::{CatalogObjectId, CatalogVersion, ContractHash};
    use andromeda_storage::{
        decode_catalog_record, encode_catalog_record, replay_catalog_wal_records, CatalogWalRecord,
        Lsn,
    };

    // =========================================================================
    // ENCODE/DECODE ROUND-TRIP TESTS (8 tests)
    // =========================================================================

    #[test]
    fn test_encode_decode_definition_batch_applied_preserves_all_fields() {
        let original = CatalogWalRecord::DefinitionBatchApplied {
            batch_id: 123,
            new_catalog_version: CatalogVersion::new(10),
            procedure_count: 3,
            affected_procedure_ids: vec![
                CatalogObjectId::new(100),
                CatalogObjectId::new(101),
                CatalogObjectId::new(102),
            ],
            timestamp_secs: 1704067200, // 2024-01-01
            operator_principal: "alice@example.com".to_string(),
        };

        let encoded = encode_catalog_record(&original).expect("encode failed");
        let decoded = decode_catalog_record(&encoded).expect("decode failed");

        assert_eq!(original, decoded);
    }

    #[test]
    fn test_encode_decode_procedure_added_round_trip() {
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
    fn test_encode_decode_procedure_altered_with_different_hashes() {
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
    fn test_encode_decode_procedure_dropped_round_trip() {
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
    fn test_encode_decode_statistics_updated_round_trip() {
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
    fn test_encode_decode_catalog_checkpoint_round_trip() {
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
    fn test_encode_decode_with_unicode_operator_principal() {
        let original = CatalogWalRecord::DefinitionBatchApplied {
            batch_id: 99,
            new_catalog_version: CatalogVersion::new(1),
            procedure_count: 1,
            affected_procedure_ids: vec![CatalogObjectId::new(1)],
            timestamp_secs: 1704067206,
            operator_principal: "müller@café.com".to_string(), // Unicode characters
        };

        let encoded = encode_catalog_record(&original).expect("encode failed");
        let decoded = decode_catalog_record(&encoded).expect("decode failed");

        assert_eq!(original, decoded);
    }

    #[test]
    fn test_encode_decode_large_batch_with_many_procedures() {
        let mut procedure_ids = Vec::new();
        for i in 0..1000 {
            procedure_ids.push(CatalogObjectId::new(1000 + i));
        }

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

    // =========================================================================
    // VERSION MONOTONICITY AND ORDERING TESTS (3 tests)
    // =========================================================================

    #[test]
    fn test_replay_rejects_version_going_backward() {
        let records = vec![
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(1),
                signature_hash: ContractHash::test_vector(0xFF),
                new_catalog_version: CatalogVersion::new(10),
                timestamp_secs: 1000,
            },
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(2),
                signature_hash: ContractHash::test_vector(0xEE),
                new_catalog_version: CatalogVersion::new(5), // REORDERING!
                timestamp_secs: 2000,
            },
        ];

        let result = replay_catalog_wal_records(&records, CatalogVersion::new(100));
        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_msg = err.message();
        assert!(err_msg.contains("version reordering"));
    }

    #[test]
    fn test_replay_rejects_duplicate_version() {
        let records = vec![
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(1),
                signature_hash: ContractHash::test_vector(0xFF),
                new_catalog_version: CatalogVersion::new(5),
                timestamp_secs: 1000,
            },
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(2),
                signature_hash: ContractHash::test_vector(0xEE),
                new_catalog_version: CatalogVersion::new(5), // DUPLICATE!
                timestamp_secs: 2000,
            },
        ];

        let result = replay_catalog_wal_records(&records, CatalogVersion::new(100));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message().contains("version reordering"));
    }

    #[test]
    fn test_replay_accepts_strictly_increasing_versions() {
        let records = vec![
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(1),
                signature_hash: ContractHash::test_vector(0xFF),
                new_catalog_version: CatalogVersion::new(1),
                timestamp_secs: 1000,
            },
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(2),
                signature_hash: ContractHash::test_vector(0xEE),
                new_catalog_version: CatalogVersion::new(2),
                timestamp_secs: 2000,
            },
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(3),
                signature_hash: ContractHash::test_vector(0xDD),
                new_catalog_version: CatalogVersion::new(5), // Gap is OK
                timestamp_secs: 3000,
            },
        ];

        let snapshot =
            replay_catalog_wal_records(&records, CatalogVersion::new(100)).expect("replay failed");
        assert_eq!(snapshot.visible_procedure_count(), 3);
        assert_eq!(snapshot.catalog_version, CatalogVersion::new(5));
    }

    // =========================================================================
    // PROCEDURE ID VALIDATION TESTS (3 tests)
    // =========================================================================

    #[test]
    fn test_replay_rejects_alter_on_non_existent_procedure() {
        let records = vec![CatalogWalRecord::ProcedureAltered {
            procedure_id: CatalogObjectId::new(999), // Never added!
            old_hash: ContractHash::test_vector(0x11),
            new_hash: ContractHash::test_vector(0x22),
            new_catalog_version: CatalogVersion::new(1),
            timestamp_secs: 1000,
        }];

        let result = replay_catalog_wal_records(&records, CatalogVersion::new(100));
        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_msg = err.message();
        assert!(err_msg.contains("non-existent procedure"));
    }

    #[test]
    fn test_replay_rejects_drop_on_non_existent_procedure() {
        let records = vec![CatalogWalRecord::ProcedureDropped {
            procedure_id: CatalogObjectId::new(888), // Never added!
            dropped_version: CatalogVersion::new(1),
            new_catalog_version: CatalogVersion::new(2),
            timestamp_secs: 1000,
        }];

        let result = replay_catalog_wal_records(&records, CatalogVersion::new(100));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .message()
            .contains("non-existent procedure"));
    }

    #[test]
    fn test_replay_validates_all_batch_procedures_affect_visible_set() {
        let records = vec![CatalogWalRecord::DefinitionBatchApplied {
            batch_id: 1,
            new_catalog_version: CatalogVersion::new(1),
            procedure_count: 2,
            affected_procedure_ids: vec![CatalogObjectId::new(1), CatalogObjectId::new(2)],
            timestamp_secs: 1000,
            operator_principal: "test".to_string(),
        }];

        let snapshot =
            replay_catalog_wal_records(&records, CatalogVersion::new(100)).expect("replay failed");

        // Both procedures should be in the visible set
        assert_eq!(snapshot.visible_procedure_count(), 2);
        assert!(snapshot.procedure_ids.contains(&CatalogObjectId::new(1)));
        assert!(snapshot.procedure_ids.contains(&CatalogObjectId::new(2)));
    }

    // =========================================================================
    // CHECKSUM AND CORRUPTION TESTS (3 tests)
    // =========================================================================

    #[test]
    fn test_decode_rejects_corrupted_checksum() {
        let original = CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(1),
            signature_hash: ContractHash::test_vector(0xCC),
            new_catalog_version: CatalogVersion::new(1),
            timestamp_secs: 1000,
        };

        let mut encoded = encode_catalog_record(&original).expect("encode failed");

        // Corrupt a byte in the checksum (bytes 6-37 are the checksum)
        if encoded.len() > 10 {
            encoded[10] ^= 0xFF;
        }

        let result = decode_catalog_record(&encoded);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message().contains("checksum mismatch"));
    }

    #[test]
    fn test_decode_rejects_truncated_frame() {
        let bytes = vec![0x01, 0x00, 0x02, 0x00]; // Too short
        let result = decode_catalog_record(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_rejects_invalid_record_version() {
        let mut frame = vec![0xFF, 0xFF]; // Invalid version (u16::MAX)
        frame.extend_from_slice(&[0u8; 4]); // payload_len
        frame.extend_from_slice(&[0u8; 32]); // checksum
        frame.push(1u8); // minimal payload

        let result = decode_catalog_record(&frame);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .message()
            .contains("unsupported catalog WAL record version"));
    }

    // =========================================================================
    // RECOVERY REPLAY TESTS (4 tests)
    // =========================================================================

    #[test]
    fn test_replay_complete_procedure_lifecycle_add_alter_drop() {
        let records = vec![
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(1),
                signature_hash: ContractHash::test_vector(0x11),
                new_catalog_version: CatalogVersion::new(1),
                timestamp_secs: 1000,
            },
            CatalogWalRecord::ProcedureAltered {
                procedure_id: CatalogObjectId::new(1),
                old_hash: ContractHash::test_vector(0x11),
                new_hash: ContractHash::test_vector(0x22),
                new_catalog_version: CatalogVersion::new(2),
                timestamp_secs: 2000,
            },
            CatalogWalRecord::ProcedureDropped {
                procedure_id: CatalogObjectId::new(1),
                dropped_version: CatalogVersion::new(2),
                new_catalog_version: CatalogVersion::new(3),
                timestamp_secs: 3000,
            },
        ];

        let snapshot =
            replay_catalog_wal_records(&records, CatalogVersion::new(100)).expect("replay failed");

        // Final state: procedure is dropped
        assert_eq!(snapshot.catalog_version, CatalogVersion::new(3));
        assert_eq!(snapshot.visible_procedure_count(), 0);
    }

    #[test]
    fn test_replay_checkpoint_validates_procedure_count_match() {
        let records = vec![
            CatalogWalRecord::DefinitionBatchApplied {
                batch_id: 1,
                new_catalog_version: CatalogVersion::new(1),
                procedure_count: 1,
                affected_procedure_ids: vec![CatalogObjectId::new(1)],
                timestamp_secs: 1000,
                operator_principal: "test".to_string(),
            },
            CatalogWalRecord::CatalogCheckpoint {
                checkpoint_lsn: Lsn::new(100),
                catalog_version: CatalogVersion::new(1),
                visible_procedure_count: 1,
                timestamp_secs: 1001,
            },
        ];

        let result = replay_catalog_wal_records(&records, CatalogVersion::new(100));
        assert!(result.is_ok());
    }

    #[test]
    fn test_replay_checkpoint_rejects_procedure_count_mismatch() {
        let records = vec![
            CatalogWalRecord::DefinitionBatchApplied {
                batch_id: 1,
                new_catalog_version: CatalogVersion::new(1),
                procedure_count: 1,
                affected_procedure_ids: vec![CatalogObjectId::new(1)],
                timestamp_secs: 1000,
                operator_principal: "test".to_string(),
            },
            CatalogWalRecord::CatalogCheckpoint {
                checkpoint_lsn: Lsn::new(100),
                catalog_version: CatalogVersion::new(1),
                visible_procedure_count: 42, // Mismatch!
                timestamp_secs: 1001,
            },
        ];

        let result = replay_catalog_wal_records(&records, CatalogVersion::new(100));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message().contains("visible_procedure_count mismatch"));
    }

    #[test]
    fn test_replay_respects_target_version_filter() {
        let records = vec![
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(1),
                signature_hash: ContractHash::test_vector(0xFF),
                new_catalog_version: CatalogVersion::new(1),
                timestamp_secs: 1000,
            },
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(2),
                signature_hash: ContractHash::test_vector(0xEE),
                new_catalog_version: CatalogVersion::new(10),
                timestamp_secs: 2000,
            },
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(3),
                signature_hash: ContractHash::test_vector(0xDD),
                new_catalog_version: CatalogVersion::new(20),
                timestamp_secs: 3000,
            },
        ];

        let snapshot =
            replay_catalog_wal_records(&records, CatalogVersion::new(11)).expect("replay failed");

        // Only records up to version 11 should be replayed
        assert_eq!(snapshot.catalog_version, CatalogVersion::new(10));
        assert_eq!(snapshot.visible_procedure_count(), 2);
        assert!(!snapshot.procedure_ids.contains(&CatalogObjectId::new(3)));
    }
}
