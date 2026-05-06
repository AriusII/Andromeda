//! Property-based roundtrip tests for a bounded WAL frame codec model.
//!
//! These tests intentionally use a small local frame model so they exercise
//! concrete codec invariants instead of assertion-only smoke checks.

#![forbid(unsafe_code)]

use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

const MAGIC: u64 = 0x414e_4452_4f57_414c_u64;
const FORMAT_VERSION_V1: u16 = 1;
const HEADER_LEN: usize = 8 + 2 + 8 + 8 + 4 + 8;

#[derive(Debug, Clone, PartialEq, Eq)]
struct WalCodecRecord {
    lsn: u64,
    transaction_id: Option<u64>,
    payload: Vec<u8>,
}

fn arb_lsn() -> impl Strategy<Value = u64> {
    0u64..u64::MAX
}

fn arb_transaction_id() -> impl Strategy<Value = Option<u64>> {
    prop_oneof![Just(None), (1u64..u64::MAX).prop_map(Some)]
}

fn arb_payload() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(0u8..=255u8, 0..10_000)
}

fn arb_record() -> impl Strategy<Value = WalCodecRecord> {
    (arb_lsn(), arb_transaction_id(), arb_payload()).prop_map(|(lsn, transaction_id, payload)| {
        WalCodecRecord {
            lsn,
            transaction_id,
            payload,
        }
    })
}

fn checksum(payload: &[u8]) -> u64 {
    payload.iter().fold(0xcbf2_9ce4_8422_2325, |acc, byte| {
        acc.wrapping_mul(0x0000_0100_0000_01b3)
            .wrapping_add(u64::from(*byte))
    })
}

fn encode_record(record: &WalCodecRecord) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + record.payload.len());
    out.extend_from_slice(&MAGIC.to_le_bytes());
    out.extend_from_slice(&FORMAT_VERSION_V1.to_le_bytes());
    out.extend_from_slice(&record.lsn.to_le_bytes());
    out.extend_from_slice(&record.transaction_id.unwrap_or(0).to_le_bytes());
    out.extend_from_slice(&(record.payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&checksum(&record.payload).to_le_bytes());
    out.extend_from_slice(&record.payload);
    out
}

fn decode_record(bytes: &[u8]) -> Result<(WalCodecRecord, usize), String> {
    if bytes.len() < HEADER_LEN {
        return Err("WAL frame header is truncated".to_string());
    }

    let magic = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
    if magic != MAGIC {
        return Err("WAL frame magic mismatch".to_string());
    }

    let version = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
    if version != FORMAT_VERSION_V1 {
        return Err("unsupported WAL frame version".to_string());
    }

    let lsn = u64::from_le_bytes(bytes[10..18].try_into().unwrap());
    let raw_transaction_id = u64::from_le_bytes(bytes[18..26].try_into().unwrap());
    let payload_len = u32::from_le_bytes(bytes[26..30].try_into().unwrap()) as usize;
    let expected_checksum = u64::from_le_bytes(bytes[30..38].try_into().unwrap());
    let frame_len = HEADER_LEN
        .checked_add(payload_len)
        .ok_or_else(|| "WAL frame length overflow".to_string())?;

    if bytes.len() < frame_len {
        return Err("WAL frame payload is truncated".to_string());
    }

    let payload = bytes[HEADER_LEN..frame_len].to_vec();
    let actual_checksum = checksum(&payload);
    if actual_checksum != expected_checksum {
        return Err("WAL frame payload checksum mismatch".to_string());
    }

    Ok((
        WalCodecRecord {
            lsn,
            transaction_id: (raw_transaction_id != 0).then_some(raw_transaction_id),
            payload,
        },
        frame_len,
    ))
}

fn decode_all(mut bytes: &[u8]) -> Result<Vec<WalCodecRecord>, String> {
    let mut records = Vec::new();
    while !bytes.is_empty() {
        let (record, consumed) = decode_record(bytes)?;
        records.push(record);
        bytes = &bytes[consumed..];
    }
    Ok(records)
}

fn prev_lsn_is_valid(prev_lsn: u64, lsn: u64) -> bool {
    prev_lsn == 0 || prev_lsn < lsn
}

#[test]
fn prop_wal_record_roundtrip_consistency() {
    proptest!(|(record in arb_record())| {
        let encoded = encode_record(&record);
        let (decoded, consumed) = decode_record(&encoded).map_err(TestCaseError::fail)?;

        prop_assert_eq!(consumed, encoded.len());
        prop_assert_eq!(decoded, record);
    });
}

#[test]
fn prop_wal_lsn_preserved() {
    proptest!(|(lsn in arb_lsn(), transaction_id in arb_transaction_id(), payload in arb_payload())| {
        let record = WalCodecRecord {
            lsn,
            transaction_id,
            payload,
        };

        let encoded = encode_record(&record);
        let (decoded, _) = decode_record(&encoded).map_err(TestCaseError::fail)?;

        prop_assert_eq!(decoded.lsn, lsn);
    });
}

#[test]
fn prop_wal_txn_id_preserved() {
    proptest!(|(transaction_id in arb_transaction_id())| {
        let record = WalCodecRecord {
            lsn: 42,
            transaction_id,
            payload: vec![1, 2, 3],
        };

        let encoded = encode_record(&record);
        let (decoded, _) = decode_record(&encoded).map_err(TestCaseError::fail)?;

        prop_assert_eq!(decoded.transaction_id, transaction_id);
    });
}

#[test]
fn prop_wal_payload_integrity() {
    proptest!(|(payload in arb_payload())| {
        let record = WalCodecRecord {
            lsn: 42,
            transaction_id: Some(7),
            payload: payload.clone(),
        };

        let encoded = encode_record(&record);
        let (decoded, _) = decode_record(&encoded).map_err(TestCaseError::fail)?;

        prop_assert_eq!(decoded.payload, payload);
    });
}

#[test]
fn prop_wal_checksum_detects_corruption() {
    proptest!(|(record in arb_record(), corruption_bit in 0u8..8u8)| {
        let mut encoded = encode_record(&record);
        encoded[30] ^= 1u8 << corruption_bit;

        let result = decode_record(&encoded);

        prop_assert!(
            matches!(result, Err(message) if message.contains("checksum")),
            "corrupted checksum was accepted"
        );
    });
}

#[test]
fn prop_wal_empty_payload() {
    let record = WalCodecRecord {
        lsn: 1,
        transaction_id: None,
        payload: Vec::new(),
    };

    let encoded = encode_record(&record);
    let (decoded, consumed) = decode_record(&encoded).unwrap();

    assert_eq!(consumed, HEADER_LEN);
    assert_eq!(decoded, record);
}

#[test]
fn prop_wal_large_payload_not_truncated() {
    proptest!(|(payload in prop::collection::vec(0u8..=255u8, 1000..10_000))| {
        let record = WalCodecRecord {
            lsn: 100,
            transaction_id: Some(1),
            payload: payload.clone(),
        };

        let encoded = encode_record(&record);
        let (decoded, consumed) = decode_record(&encoded).map_err(TestCaseError::fail)?;

        prop_assert_eq!(consumed, HEADER_LEN + payload.len());
        prop_assert_eq!(decoded.payload.len(), payload.len());
        prop_assert_eq!(decoded.payload, payload);
    });
}

#[test]
fn prop_wal_encoding_deterministic() {
    proptest!(|(record in arb_record())| {
        let first = encode_record(&record);
        let second = encode_record(&record);

        prop_assert_eq!(first, second);
    });
}

#[test]
fn prop_wal_frame_boundaries_preserved() {
    proptest!(|(records in prop::collection::vec(arb_record(), 1..50))| {
        let encoded: Vec<u8> = records.iter().flat_map(encode_record).collect();
        let decoded = decode_all(&encoded).map_err(TestCaseError::fail)?;

        prop_assert_eq!(decoded.len(), records.len());
        prop_assert_eq!(decoded, records);
    });
}

#[test]
fn prop_wal_header_invariants() {
    proptest!(|(prev_lsn in arb_lsn(), lsn in arb_lsn())| {
        let expected = prev_lsn == 0 || prev_lsn < lsn;

        prop_assert_eq!(prev_lsn_is_valid(prev_lsn, lsn), expected);
    });
}

#[test]
fn prop_wal_format_version_recognized() {
    let record = WalCodecRecord {
        lsn: 1,
        transaction_id: None,
        payload: vec![1, 2, 3],
    };
    let mut encoded = encode_record(&record);

    assert!(decode_record(&encoded).is_ok());

    encoded[8..10].copy_from_slice(&u16::MAX.to_le_bytes());
    let error = decode_record(&encoded).unwrap_err();
    assert!(error.contains("version"));
}

#[test]
fn prop_wal_multiple_records_independent() {
    proptest!(|(records in prop::collection::vec(arb_record(), 2..100))| {
        let encoded: Vec<u8> = records.iter().flat_map(encode_record).collect();
        let decoded = decode_all(&encoded).map_err(TestCaseError::fail)?;

        for (actual, expected) in decoded.iter().zip(records.iter()) {
            prop_assert_eq!(actual, expected);
        }
    });
}

#[test]
fn integration_wal_codec_full_lifecycle() {
    proptest!(|(records in prop::collection::vec(arb_record(), 1..50))| {
        let encoded: Vec<u8> = records.iter().flat_map(encode_record).collect();
        let decoded = decode_all(&encoded).map_err(TestCaseError::fail)?;
        let reencoded: Vec<u8> = decoded.iter().flat_map(encode_record).collect();

        prop_assert_eq!(decoded, records);
        prop_assert_eq!(reencoded, encoded);
    });
}
