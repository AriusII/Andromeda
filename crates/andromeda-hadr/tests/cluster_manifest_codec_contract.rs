use andromeda_hadr::{
    HadrClusterId, HadrClusterManifestMember, HadrClusterManifestV0, HadrClusterManifestVersion,
    HadrClusterQuorumPolicy, HadrEpoch, HadrEvidenceHash, HadrNodeId,
};

fn manifest_fixture() -> HadrClusterManifestV0 {
    HadrClusterManifestV0::new(
        HadrClusterId::new([1; 16]),
        "alpha-cluster",
        HadrEpoch::new(7),
        HadrClusterManifestVersion::new(4),
        HadrNodeId::new(2),
        vec![
            HadrClusterManifestMember::new(HadrNodeId::new(1)),
            HadrClusterManifestMember::new(HadrNodeId::new(2)),
            HadrClusterManifestMember::new(HadrNodeId::new(3)),
        ],
        HadrClusterQuorumPolicy::Majority,
        HadrEvidenceHash::new([0xA1; 32]),
        HadrEvidenceHash::new([0xB2; 32]),
        HadrEvidenceHash::new([0xC3; 32]),
    )
    .expect("valid manifest fixture")
}

#[test]
fn cluster_manifest_roundtrip_preserves_fields() {
    let manifest = manifest_fixture();
    let encoded = manifest.encode().expect("encode manifest");
    let decoded = HadrClusterManifestV0::decode(&encoded).expect("decode manifest");
    assert_eq!(decoded, manifest);
}

#[test]
fn cluster_manifest_codec_bytes_have_stable_layout_contract() {
    let manifest = manifest_fixture();
    let encoded = manifest.encode().expect("encode manifest");

    assert_eq!(&encoded[..16], b"AND-HADR-CMANV0\0");
    assert_eq!(u16::from_le_bytes([encoded[16], encoded[17]]), 1);
    assert_eq!(
        u64::from_le_bytes([
            encoded[18],
            encoded[19],
            encoded[20],
            encoded[21],
            encoded[22],
            encoded[23],
            encoded[24],
            encoded[25]
        ]),
        180
    );

    let payload = &encoded[58..];
    assert_eq!(
        u64::from_le_bytes([
            payload[0], payload[1], payload[2], payload[3], payload[4], payload[5], payload[6],
            payload[7]
        ]),
        7
    );
    assert_eq!(
        u64::from_le_bytes([
            payload[8],
            payload[9],
            payload[10],
            payload[11],
            payload[12],
            payload[13],
            payload[14],
            payload[15]
        ]),
        4
    );
}

#[test]
fn cluster_manifest_decode_rejects_bad_magic() {
    let mut encoded = manifest_fixture().encode().expect("encode manifest");
    encoded[0] ^= 0xFF;
    let error = HadrClusterManifestV0::decode(&encoded).expect_err("bad magic must fail");
    assert!(error.message().contains("magic mismatch"));
}

#[test]
fn cluster_manifest_decode_rejects_unsupported_version() {
    let mut encoded = manifest_fixture().encode().expect("encode manifest");
    encoded[16..18].copy_from_slice(&2u16.to_le_bytes());
    let error = HadrClusterManifestV0::decode(&encoded).expect_err("unsupported version must fail");
    assert!(error.message().contains("version is unsupported"));
}

#[test]
fn cluster_manifest_decode_rejects_truncated_file() {
    let encoded = manifest_fixture().encode().expect("encode manifest");
    let truncated = &encoded[..encoded.len() - 1];
    let error = HadrClusterManifestV0::decode(truncated).expect_err("truncation must fail");
    assert!(error.message().contains("length mismatch"));
}

#[test]
fn cluster_manifest_decode_rejects_checksum_mismatch() {
    let mut encoded = manifest_fixture().encode().expect("encode manifest");
    let last = encoded.last_mut().expect("encoded bytes");
    *last ^= 0x11;
    let error =
        HadrClusterManifestV0::decode(&encoded).expect_err("checksum mismatch must fail decode");
    assert!(error.message().contains("checksum mismatch"));
}

#[test]
fn cluster_manifest_decode_rejects_payload_length_mismatch() {
    let mut encoded = manifest_fixture().encode().expect("encode manifest");
    encoded[18..26].copy_from_slice(&0u64.to_le_bytes());
    let error = HadrClusterManifestV0::decode(&encoded).expect_err("payload length mismatch");
    assert!(error.message().contains("length mismatch"));
}

#[test]
fn cluster_manifest_validate_rejects_duplicate_member_ids() {
    let error = HadrClusterManifestV0::new(
        HadrClusterId::new([1; 16]),
        "alpha-cluster",
        HadrEpoch::new(1),
        HadrClusterManifestVersion::new(1),
        HadrNodeId::new(1),
        vec![
            HadrClusterManifestMember::new(HadrNodeId::new(1)),
            HadrClusterManifestMember::new(HadrNodeId::new(1)),
        ],
        HadrClusterQuorumPolicy::Majority,
        HadrEvidenceHash::new([0xA1; 32]),
        HadrEvidenceHash::new([0xB2; 32]),
        HadrEvidenceHash::new([0xC3; 32]),
    )
    .expect_err("duplicate member ids must fail");
    assert!(error.message().contains("must be unique"));
}

#[test]
fn cluster_manifest_validate_rejects_invalid_primary_reference() {
    let error = HadrClusterManifestV0::new(
        HadrClusterId::new([1; 16]),
        "alpha-cluster",
        HadrEpoch::new(1),
        HadrClusterManifestVersion::new(1),
        HadrNodeId::new(9),
        vec![
            HadrClusterManifestMember::new(HadrNodeId::new(1)),
            HadrClusterManifestMember::new(HadrNodeId::new(2)),
        ],
        HadrClusterQuorumPolicy::Majority,
        HadrEvidenceHash::new([0xA1; 32]),
        HadrEvidenceHash::new([0xB2; 32]),
        HadrEvidenceHash::new([0xC3; 32]),
    )
    .expect_err("invalid primary reference must fail");
    assert!(error.message().contains("must reference a member"));
}

#[test]
fn cluster_manifest_validate_rejects_missing_fencing_hash() {
    let error = HadrClusterManifestV0::new(
        HadrClusterId::new([1; 16]),
        "alpha-cluster",
        HadrEpoch::new(1),
        HadrClusterManifestVersion::new(1),
        HadrNodeId::new(1),
        vec![HadrClusterManifestMember::new(HadrNodeId::new(1))],
        HadrClusterQuorumPolicy::Majority,
        HadrEvidenceHash::new([0; 32]),
        HadrEvidenceHash::new([0xB2; 32]),
        HadrEvidenceHash::new([0xC3; 32]),
    )
    .expect_err("missing fencing hash must fail");
    assert!(error.message().contains("fencing token hash"));
}

#[test]
fn cluster_manifest_successor_rejects_stale_epoch_or_version() {
    let previous = manifest_fixture();
    let stale = HadrClusterManifestV0::new(
        HadrClusterId::new([1; 16]),
        "alpha-cluster",
        HadrEpoch::new(7),
        HadrClusterManifestVersion::new(4),
        HadrNodeId::new(2),
        vec![
            HadrClusterManifestMember::new(HadrNodeId::new(1)),
            HadrClusterManifestMember::new(HadrNodeId::new(2)),
        ],
        HadrClusterQuorumPolicy::Fixed { quorum_size: 2 },
        HadrEvidenceHash::new([0xA2; 32]),
        HadrEvidenceHash::new([0xB3; 32]),
        HadrEvidenceHash::new([0xC4; 32]),
    )
    .expect("valid stale manifest fixture");

    let error = stale
        .ensure_advances(&previous)
        .expect_err("successor with stale epoch/version must fail");
    assert!(error.message().contains("strictly advance"));
}
