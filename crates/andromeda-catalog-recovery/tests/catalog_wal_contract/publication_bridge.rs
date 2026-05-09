use super::support::{
    catalog_storage_record, encode_catalog_apply_payload, procedure_added_record,
};
use andromeda_catalog_recovery::replay_storage_catalog_publications_from_wal as replay_catalog_publications_from_wal;
use andromeda_types::{CatalogVersion, TransactionId};
use andromeda_wal::{Lsn, WalRecordKind};

#[test]
fn storage_publication_bridge_reconstructs_complete_catalog_publication() {
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
fn storage_publication_bridge_ignores_incomplete_tail_without_commit() {
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
fn storage_publication_bridge_reconstructs_last_durable_before_tail() {
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
fn storage_publication_bridge_rejects_apply_before_begin() {
    let records = vec![catalog_storage_record(
        WalRecordKind::CatalogChangeApply,
        40,
        None,
        99,
        b"apply-orphan",
    )];

    let err = replay_catalog_publications_from_wal(&records)
        .expect_err("apply before begin must be rejected");
    assert!(err.message().contains("apply observed before"));
}

#[test]
fn storage_publication_bridge_rejects_commit_without_apply() {
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

    let err = replay_catalog_publications_from_wal(&records)
        .expect_err("commit without apply must be rejected");
    assert!(err.message().contains("requires at least one apply record"));
}

#[test]
fn storage_publication_bridge_preserves_multiple_decoded_apply_records() {
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
fn storage_publication_bridge_rejects_malformed_apply_payload_before_publication() {
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

    let err = replay_catalog_publications_from_wal(&records)
        .expect_err("malformed apply payload must be rejected");
    let message = err.message().to_string();
    assert!(message.contains("apply payload at LSN 71"));
    assert!(message.contains("not a valid catalog record"));
}

#[test]
fn storage_publication_bridge_rejects_truncated_apply_payload_before_publication() {
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

    let err = replay_catalog_publications_from_wal(&records)
        .expect_err("truncated apply payload must be rejected");
    let message = err.message().to_string();
    assert!(message.contains("apply payload at LSN 81"));
    assert!(message.contains("frame length mismatch"));
}

#[test]
fn storage_publication_bridge_rejects_commit_before_begin() {
    let records = vec![catalog_storage_record(
        WalRecordKind::CatalogChangeCommit,
        90,
        None,
        104,
        b"commit-orphan",
    )];

    let err = replay_catalog_publications_from_wal(&records)
        .expect_err("commit before begin must be rejected");
    assert!(err.message().contains("commit observed before"));
}
