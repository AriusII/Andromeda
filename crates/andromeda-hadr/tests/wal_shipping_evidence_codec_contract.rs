use andromeda_hadr::quorum_runtime::{FencingPolicy, ReplicationMode};
use andromeda_hadr::shipping_contract::{
    WalShipmentRange, WalShippingEvidenceCodecRejection, WalShippingEvidenceRejectionReason,
    WalShippingEvidenceV0,
};
use andromeda_wal::Lsn;

fn valid_evidence() -> WalShippingEvidenceV0 {
    WalShippingEvidenceV0::new(
        11,
        22,
        7,
        WalShipmentRange {
            first: Lsn::new(100),
            last: Lsn::new(102),
            count: 3,
        },
        Lsn::new(102),
        Lsn::new(102),
        Lsn::new(101),
        ReplicationMode::QuorumEnforced,
        FencingPolicy::BlockOnQuorumLoss,
        0,
        [0xAB; 32],
        Some(WalShippingEvidenceRejectionReason::AckStale),
    )
    .expect("valid evidence")
}

#[test]
fn wal_shipping_evidence_v0_roundtrip_codec_contract() {
    let evidence = valid_evidence();
    let encoded = evidence.encode_le().expect("encode");
    let decoded = WalShippingEvidenceV0::decode_le(&encoded).expect("decode");
    assert_eq!(decoded, evidence);
}

#[test]
fn wal_shipping_evidence_v0_rejects_corruption_and_invalid_ranges() {
    let encoded = valid_evidence().encode_le().expect("encode");

    let mut bad_magic = encoded.clone();
    bad_magic[0] ^= 0xFF;
    assert_eq!(
        WalShippingEvidenceV0::decode_le(&bad_magic).expect_err("magic mismatch"),
        WalShippingEvidenceCodecRejection::MagicMismatch
    );

    let mut bad_version = encoded.clone();
    bad_version[8] = 1;
    bad_version[9] = 0;
    assert_eq!(
        WalShippingEvidenceV0::decode_le(&bad_version).expect_err("version mismatch"),
        WalShippingEvidenceCodecRejection::UnsupportedVersion
    );

    let truncated = &encoded[..encoded.len() - 1];
    assert_eq!(
        WalShippingEvidenceV0::decode_le(truncated).expect_err("truncated frame"),
        WalShippingEvidenceCodecRejection::LengthMismatch
    );

    let mut bad_checksum = encoded.clone();
    let payload_start = 8 + 2 + 4 + 32;
    bad_checksum[payload_start] ^= 0x01;
    assert_eq!(
        WalShippingEvidenceV0::decode_le(&bad_checksum).expect_err("checksum mismatch"),
        WalShippingEvidenceCodecRejection::ChecksumMismatch
    );

    let invalid_range = WalShippingEvidenceV0::new(
        11,
        22,
        7,
        WalShipmentRange {
            first: Lsn::new(100),
            last: Lsn::new(102),
            count: 2,
        },
        Lsn::new(102),
        Lsn::new(102),
        Lsn::new(102),
        ReplicationMode::Asynchronous,
        FencingPolicy::Allow,
        10,
        [0xCC; 32],
        None,
    )
    .expect_err("invalid count/span range");
    assert_eq!(
        invalid_range,
        WalShippingEvidenceCodecRejection::InvalidValidatedRange
    );
}
