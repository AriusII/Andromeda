//! Comprehensive contract tests for catalog WAL record encoding and recovery.
//!
//! Total tests are maintained by `cargo test`.
//!
//! Categories:
//! - Encode/decode round-trip tests (8 tests)
//! - Version monotonicity and ordering tests (3 tests)
//! - Procedure ID validation tests (3 tests)
//! - Checksum and corruption tests (3 tests)
//! - Recovery replay tests (4 tests)
//! - Storage-side publication bridge tests (6 tests)

#[cfg(test)]
mod tests {
    use andromeda_core::{CatalogObjectId, CatalogVersion, ContractHash, TransactionId};
    use andromeda_storage::{
        CatalogWalRecord, Lsn, WalRecord, WalRecordKind, decode_catalog_record,
        encode_catalog_record, replay_catalog_publications_from_wal, replay_catalog_wal_records,
    };
    use sha2::{Digest, Sha256};

    fn catalog_storage_record(
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

    fn catalog_frame_from_payload(payload: Vec<u8>) -> Vec<u8> {
        let mut frame = Vec::new();
        frame.extend_from_slice(&1u16.to_le_bytes());
        frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        let checksum = Sha256::digest(&payload);
        frame.extend_from_slice(&checksum);
        frame.extend_from_slice(&payload);
        frame
    }

    fn catalog_frame_with_declared_payload_tail(encoded: Vec<u8>, tail: &[u8]) -> Vec<u8> {
        assert!(encoded.len() >= 38);
        let mut payload = encoded[38..].to_vec();
        payload.extend_from_slice(tail);
        catalog_frame_from_payload(payload)
    }

    fn procedure_added_record(procedure_id: u64, catalog_version: u64) -> CatalogWalRecord {
        CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(procedure_id),
            signature_hash: ContractHash::test_vector(procedure_id as u8),
            new_catalog_version: CatalogVersion::new(catalog_version),
            timestamp_secs: 1704067300 + catalog_version,
        }
    }

    fn encode_catalog_apply_payload(record: &CatalogWalRecord) -> Vec<u8> {
        encode_catalog_record(record).expect("encode catalog apply payload")
    }

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
        assert!(
            result
                .unwrap_err()
                .message()
                .contains("non-existent procedure")
        );
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
        assert!(
            result
                .unwrap_err()
                .message()
                .contains("unsupported catalog WAL record version")
        );
    }

    #[test]
    fn test_decode_rejects_trailing_bytes_after_declared_payload() {
        let original = CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(1),
            signature_hash: ContractHash::test_vector(0xAC),
            new_catalog_version: CatalogVersion::new(1),
            timestamp_secs: 1000,
        };

        let mut encoded = encode_catalog_record(&original).expect("encode failed");
        encoded.extend_from_slice(b"tail");

        let result = decode_catalog_record(&encoded);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .message()
                .contains("frame length mismatch")
        );
    }

    #[test]
    fn test_decode_rejects_trailing_bytes_inside_declared_payload() {
        let original = CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(1),
            signature_hash: ContractHash::test_vector(0xAD),
            new_catalog_version: CatalogVersion::new(1),
            timestamp_secs: 1000,
        };

        let encoded = encode_catalog_record(&original).expect("encode failed");
        let frame = catalog_frame_with_declared_payload_tail(encoded, b"semantic-tail");

        let result = decode_catalog_record(&frame);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .message()
                .contains("payload has trailing bytes")
        );
    }

    #[test]
    fn test_decode_rejects_definition_batch_count_before_large_allocation() {
        let mut payload = Vec::new();
        payload.push(0u8);
        payload.extend_from_slice(&1u64.to_le_bytes());
        payload.extend_from_slice(&CatalogVersion::new(1).get().to_le_bytes());
        payload.extend_from_slice(&u32::MAX.to_le_bytes());

        let frame = catalog_frame_from_payload(payload);
        let result = decode_catalog_record(&frame);

        assert!(result.is_err());
        assert!(result.unwrap_err().message().contains("procedure_count"));
    }

    #[test]
    fn test_storage_publication_bridge_reconstructs_complete_catalog_publication() {
        let apply_record = procedure_added_record(501, 11);
        let apply_payload = encode_catalog_apply_payload(&apply_record);
        let records = vec![
            catalog_storage_record(WalRecordKind::CatalogChangeBegin, 10, None, 77, b"begin-v1"),
            catalog_storage_record(
                WalRecordKind::CatalogChangeApply,
                11,
                Some(10),
                77,
                &apply_payload,
            ),
            catalog_storage_record(
                WalRecordKind::CatalogChangeCommit,
                12,
                Some(11),
                77,
                b"commit-v1",
            ),
        ];

        let report = replay_catalog_publications_from_wal(&records).expect("publication replay");
        let publication = report
            .last_durable_publication()
            .expect("last durable publication");

        assert_eq!(report.catalog_records_seen, 3);
        assert_eq!(report.incomplete_tail_records, 0);
        assert_eq!(report.publications.len(), 1);
        assert_eq!(publication.transaction_id, TransactionId::new(77));
        assert_eq!(publication.begin_lsn, Lsn::new(10));
        assert_eq!(publication.commit_lsn, Lsn::new(12));
        assert_eq!(publication.apply_count(), 1);
        assert_eq!(publication.record_count(), 3);
        assert_eq!(publication.begin_payload, b"begin-v1".to_vec());
        assert_eq!(
            publication.apply_records[0].payload.as_slice(),
            apply_payload.as_slice()
        );
        assert_eq!(&publication.apply_records[0].catalog_record, &apply_record);
        assert_eq!(
            publication.last_catalog_version(),
            Some(CatalogVersion::new(11))
        );
        assert_eq!(publication.commit_payload, b"commit-v1".to_vec());
    }

    #[test]
    fn test_storage_publication_bridge_ignores_incomplete_tail_without_commit() {
        let apply_record = procedure_added_record(502, 12);
        let apply_payload = encode_catalog_apply_payload(&apply_record);
        let records = vec![
            catalog_storage_record(WalRecordKind::CatalogChangeBegin, 20, None, 88, b"begin-v2"),
            catalog_storage_record(
                WalRecordKind::CatalogChangeApply,
                21,
                Some(20),
                88,
                &apply_payload,
            ),
        ];

        let report = replay_catalog_publications_from_wal(&records).expect("publication replay");

        assert!(report.last_durable_publication().is_none());
        assert!(report.publications.is_empty());
        assert_eq!(report.catalog_records_seen, 2);
        assert_eq!(report.incomplete_tail_records, 2);
    }

    #[test]
    fn test_storage_publication_bridge_reconstructs_last_durable_before_tail() {
        let durable_apply_record = procedure_added_record(503, 13);
        let durable_apply_payload = encode_catalog_apply_payload(&durable_apply_record);
        let tail_apply_record = procedure_added_record(504, 14);
        let tail_apply_payload = encode_catalog_apply_payload(&tail_apply_record);
        let records = vec![
            catalog_storage_record(WalRecordKind::CatalogChangeBegin, 30, None, 90, b"begin-v3"),
            catalog_storage_record(
                WalRecordKind::CatalogChangeApply,
                31,
                Some(30),
                90,
                &durable_apply_payload,
            ),
            catalog_storage_record(
                WalRecordKind::CatalogChangeCommit,
                32,
                Some(31),
                90,
                b"commit-v3",
            ),
            catalog_storage_record(
                WalRecordKind::CatalogChangeBegin,
                33,
                Some(32),
                91,
                b"begin-v4",
            ),
            catalog_storage_record(
                WalRecordKind::CatalogChangeApply,
                34,
                Some(33),
                91,
                &tail_apply_payload,
            ),
        ];

        let report = replay_catalog_publications_from_wal(&records).expect("publication replay");
        let publication = report
            .last_durable_publication()
            .expect("last durable publication");

        assert_eq!(report.publications.len(), 1);
        assert_eq!(report.incomplete_tail_records, 2);
        assert_eq!(publication.transaction_id, TransactionId::new(90));
        assert_eq!(publication.commit_lsn, Lsn::new(32));
        assert_eq!(publication.commit_payload, b"commit-v3".to_vec());
        assert_eq!(
            &publication.apply_records[0].catalog_record,
            &durable_apply_record
        );
    }

    #[test]
    fn test_storage_publication_bridge_rejects_apply_before_begin() {
        let records = vec![catalog_storage_record(
            WalRecordKind::CatalogChangeApply,
            40,
            None,
            99,
            b"apply-orphan",
        )];

        let result = replay_catalog_publications_from_wal(&records);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .message()
                .contains("apply observed before")
        );
    }

    #[test]
    fn test_storage_publication_bridge_rejects_commit_without_apply() {
        let records = vec![
            catalog_storage_record(
                WalRecordKind::CatalogChangeBegin,
                50,
                None,
                100,
                b"begin-empty",
            ),
            catalog_storage_record(
                WalRecordKind::CatalogChangeCommit,
                51,
                Some(50),
                100,
                b"commit-empty",
            ),
        ];

        let result = replay_catalog_publications_from_wal(&records);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .message()
                .contains("requires at least one apply record")
        );
    }

    #[test]
    fn test_storage_publication_bridge_preserves_multiple_decoded_apply_records() {
        let first_record = procedure_added_record(505, 15);
        let first_payload = encode_catalog_apply_payload(&first_record);
        let second_record = procedure_added_record(506, 16);
        let second_payload = encode_catalog_apply_payload(&second_record);
        let records = vec![
            catalog_storage_record(
                WalRecordKind::CatalogChangeBegin,
                60,
                None,
                101,
                b"begin-v5",
            ),
            catalog_storage_record(
                WalRecordKind::CatalogChangeApply,
                61,
                Some(60),
                101,
                &first_payload,
            ),
            catalog_storage_record(
                WalRecordKind::CatalogChangeApply,
                62,
                Some(61),
                101,
                &second_payload,
            ),
            catalog_storage_record(
                WalRecordKind::CatalogChangeCommit,
                63,
                Some(62),
                101,
                b"commit-v5",
            ),
        ];

        let report = replay_catalog_publications_from_wal(&records).expect("publication replay");
        let publication = report
            .last_durable_publication()
            .expect("last durable publication");

        assert_eq!(publication.apply_count(), 2);
        assert_eq!(publication.apply_records[0].lsn, Lsn::new(61));
        assert_eq!(publication.apply_records[1].lsn, Lsn::new(62));
        assert_eq!(&publication.apply_records[0].catalog_record, &first_record);
        assert_eq!(&publication.apply_records[1].catalog_record, &second_record);
        assert_eq!(
            publication.last_catalog_version(),
            Some(CatalogVersion::new(16))
        );
    }

    #[test]
    fn test_storage_publication_bridge_rejects_malformed_apply_payload_before_publication() {
        let records = vec![
            catalog_storage_record(
                WalRecordKind::CatalogChangeBegin,
                70,
                None,
                102,
                b"begin-v6",
            ),
            catalog_storage_record(
                WalRecordKind::CatalogChangeApply,
                71,
                Some(70),
                102,
                b"not-a-catalog-frame",
            ),
            catalog_storage_record(
                WalRecordKind::CatalogChangeCommit,
                72,
                Some(71),
                102,
                b"commit-v6",
            ),
        ];

        let result = replay_catalog_publications_from_wal(&records);

        assert!(result.is_err());
        let message = result.unwrap_err().message().to_string();
        assert!(message.contains("apply payload at LSN 71"));
        assert!(message.contains("not a valid catalog record"));
    }

    #[test]
    fn test_storage_publication_bridge_rejects_truncated_apply_payload_before_publication() {
        let apply_record = procedure_added_record(507, 17);
        let mut apply_payload = encode_catalog_apply_payload(&apply_record);
        apply_payload.truncate(apply_payload.len() - 1);
        let records = vec![
            catalog_storage_record(
                WalRecordKind::CatalogChangeBegin,
                80,
                None,
                103,
                b"begin-v7",
            ),
            catalog_storage_record(
                WalRecordKind::CatalogChangeApply,
                81,
                Some(80),
                103,
                &apply_payload,
            ),
            catalog_storage_record(
                WalRecordKind::CatalogChangeCommit,
                82,
                Some(81),
                103,
                b"commit-v7",
            ),
        ];

        let result = replay_catalog_publications_from_wal(&records);

        assert!(result.is_err());
        let message = result.unwrap_err().message().to_string();
        assert!(message.contains("apply payload at LSN 81"));
        assert!(message.contains("frame length mismatch"));
    }

    #[test]
    fn test_storage_publication_bridge_rejects_commit_before_begin() {
        let records = vec![catalog_storage_record(
            WalRecordKind::CatalogChangeCommit,
            90,
            None,
            104,
            b"commit-orphan",
        )];

        let result = replay_catalog_publications_from_wal(&records);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .message()
                .contains("commit observed before")
        );
    }

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
