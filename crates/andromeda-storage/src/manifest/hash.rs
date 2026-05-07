use crate::format_version::{StorageFormatFingerprint, StorageFormatKind};

pub(super) fn storage_format_manifest_hash(
    database_id: u64,
    manifest_version: u64,
    snapshot_id: u64,
    fingerprints: &[StorageFormatFingerprint],
) -> [u8; 32] {
    let mut lanes = [
        0x9e37_79b9_7f4a_7c15,
        0xc2b2_ae3d_27d4_eb4f,
        0x1656_67b1_9e37_79f9,
        0x85eb_ca77_c2b2_ae63,
    ];

    mix_hash_lane(&mut lanes, 0, database_id);
    mix_hash_lane(&mut lanes, 1, manifest_version);
    mix_hash_lane(&mut lanes, 2, snapshot_id);
    mix_hash_lane(&mut lanes, 3, fingerprints.len() as u64);
    for fingerprint in fingerprints {
        mix_hash_lane(&mut lanes, 0, storage_format_kind_tag(fingerprint.kind));
        mix_hash_lane(&mut lanes, 1, u64::from(fingerprint.version.major));
        mix_hash_lane(&mut lanes, 2, u64::from(fingerprint.version.minor));
        mix_hash_lane(
            &mut lanes,
            3,
            storage_format_kind_tag(fingerprint.kind)
                ^ (u64::from(fingerprint.version.major) << 32)
                ^ u64::from(fingerprint.version.minor),
        );
    }

    let mut hash = [0; 32];
    for (index, lane) in lanes.into_iter().enumerate() {
        hash[index * 8..(index + 1) * 8].copy_from_slice(&lane.to_le_bytes());
    }
    hash
}

fn mix_hash_lane(lanes: &mut [u64; 4], index: usize, value: u64) {
    lanes[index] ^= value.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    lanes[index] = lanes[index]
        .rotate_left(27)
        .wrapping_mul(0x94d0_49bb_1331_11eb);
}

fn storage_format_kind_tag(kind: StorageFormatKind) -> u64 {
    match kind {
        StorageFormatKind::Page => 1,
        StorageFormatKind::HeapPage => 2,
        StorageFormatKind::BTreeKey => 3,
        StorageFormatKind::BTreeNode => 4,
        StorageFormatKind::WalRecord => 5,
        StorageFormatKind::WalPayload => 6,
        StorageFormatKind::Manifest => 7,
        StorageFormatKind::Segment => 8,
        StorageFormatKind::Checkpoint => 9,
    }
}
