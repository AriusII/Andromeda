use andromeda_core::TransactionId;
use andromeda_wal::{
    Lsn, WAL_RECORD_HEADER_LEN, WalRecord, WalRecordKind, WalScanStopReason,
    decode_wal_record_frame, encode_wal_record, scan_wal_records_from,
};
use proptest::prelude::*;
use proptest::test_runner::Config;

const PROPTEST_CASES: u32 = 64;
const MAX_PAYLOAD_LEN: usize = 64;
const MAX_SEQUENCE_LEN: usize = 16;

fn payload_strategy() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(any::<u8>(), 0..=MAX_PAYLOAD_LEN)
}

fn non_empty_payload_strategy() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(any::<u8>(), 1..=MAX_PAYLOAD_LEN)
}

fn transaction_id_strategy() -> impl Strategy<Value = TransactionId> {
    (1u64..=10_000).prop_map(TransactionId::new)
}

fn owner_record(
    lsn: u64,
    previous_lsn: Option<u64>,
    transaction_id: TransactionId,
    payload: Vec<u8>,
) -> WalRecord {
    WalRecord::from_parts(
        WalRecordKind::RowInsert,
        Lsn::new(lsn),
        previous_lsn.map(Lsn::new),
        Some(transaction_id),
        payload,
    )
    .expect("generated owner WAL record should be valid")
}

fn encode_sequence(records: &[WalRecord]) -> Vec<u8> {
    records
        .iter()
        .flat_map(|record| encode_wal_record(record).expect("generated WAL record should encode"))
        .collect()
}

proptest! {
    #![proptest_config(Config {
        cases: PROPTEST_CASES,
        ..Config::default()
    })]

    #[test]
    fn encoding_is_deterministic_for_owner_records(
        lsn in 1u64..=10_000,
        transaction_id in transaction_id_strategy(),
        payload in payload_strategy(),
    ) {
        let record = owner_record(lsn, None, transaction_id, payload);

        let first = encode_wal_record(&record).expect("owner WAL record should encode");
        let second = encode_wal_record(&record).expect("owner WAL record should encode deterministically");

        prop_assert_eq!(first, second);
    }

    #[test]
    fn roundtrip_preserves_payload_and_transaction_id(
        lsn in 1u64..=10_000,
        transaction_id in transaction_id_strategy(),
        payload in payload_strategy(),
    ) {
        let record = owner_record(lsn, None, transaction_id, payload.clone());
        let encoded = encode_wal_record(&record).expect("owner WAL record should encode");

        let Some((decoded, consumed)) = decode_wal_record_frame(&encoded)
            .expect("encoded owner WAL record should decode") else {
                prop_assert!(false, "non-empty WAL frame should decode to a record");
                return Ok(());
            };

        prop_assert_eq!(consumed, encoded.len());
        prop_assert_eq!(decoded.payload(), payload.as_slice());
        prop_assert_eq!(decoded.transaction_id(), Some(transaction_id));
        prop_assert_eq!(decoded.header.lsn, Lsn::new(lsn));
    }

    #[test]
    fn checksum_corruption_is_rejected(
        lsn in 1u64..=10_000,
        transaction_id in transaction_id_strategy(),
        payload in non_empty_payload_strategy(),
    ) {
        let record = owner_record(lsn, None, transaction_id, payload);
        let mut encoded = encode_wal_record(&record).expect("owner WAL record should encode");
        encoded[WAL_RECORD_HEADER_LEN] ^= 0x01;

        prop_assert!(decode_wal_record_frame(&encoded).is_err());
    }

    #[test]
    fn scan_accepts_generated_sequential_records(
        start_lsn in 1u64..=1_000,
        transaction_id in transaction_id_strategy(),
        payloads in proptest::collection::vec(payload_strategy(), 1..=MAX_SEQUENCE_LEN),
    ) {
        let records: Vec<_> = payloads
            .into_iter()
            .enumerate()
            .map(|(index, payload)| {
                let lsn = start_lsn + index as u64;
                let previous_lsn = (index > 0).then_some(lsn - 1);
                owner_record(lsn, previous_lsn, transaction_id, payload)
            })
            .collect();
        let encoded = encode_sequence(&records);

        let scan = scan_wal_records_from(&encoded, Lsn::new(start_lsn), None);

        prop_assert!(scan.is_complete());
        prop_assert_eq!(scan.valid_bytes, encoded.len());
        prop_assert_eq!(scan.last_valid_lsn, Some(Lsn::new(start_lsn + records.len() as u64 - 1)));
        prop_assert_eq!(scan.records, records);
    }

    #[test]
    fn scan_reports_valid_prefix_before_corrupt_tail(
        start_lsn in 1u64..=1_000,
        transaction_id in transaction_id_strategy(),
        prefix_payloads in proptest::collection::vec(payload_strategy(), 1..=MAX_SEQUENCE_LEN),
        tail_payload in non_empty_payload_strategy(),
    ) {
        let records: Vec<_> = prefix_payloads
            .into_iter()
            .enumerate()
            .map(|(index, payload)| {
                let lsn = start_lsn + index as u64;
                let previous_lsn = (index > 0).then_some(lsn - 1);
                owner_record(lsn, previous_lsn, transaction_id, payload)
            })
            .collect();

        let tail_lsn = start_lsn + records.len() as u64;
        let tail = owner_record(tail_lsn, Some(tail_lsn - 1), transaction_id, tail_payload);
        let mut encoded = encode_sequence(&records);
        let valid_bytes = encoded.len();
        let mut corrupt_tail = encode_wal_record(&tail).expect("tail WAL record should encode");
        corrupt_tail[WAL_RECORD_HEADER_LEN] ^= 0x01;
        encoded.extend_from_slice(&corrupt_tail);

        let scan = scan_wal_records_from(&encoded, Lsn::new(start_lsn), None);

        prop_assert_eq!(scan.records, records);
        prop_assert_eq!(scan.valid_bytes, valid_bytes);
        prop_assert_eq!(scan.last_valid_lsn, Some(Lsn::new(tail_lsn - 1)));
        prop_assert_eq!(scan.stopped.map(|stop| stop.reason), Some(WalScanStopReason::CorruptRecord));
    }
}
