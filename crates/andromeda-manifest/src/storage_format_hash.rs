/// Primitive storage-format fingerprint input for deterministic manifest hashing.
///
/// The manifest crate owns the hash algorithm without depending on storage
/// format enum definitions. Callers provide stable kind tags and explicit
/// version components.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManifestFormatHashInput {
    pub kind_tag: u64,
    pub version_major: u32,
    pub version_minor: u32,
}

impl ManifestFormatHashInput {
    pub const fn new(kind_tag: u64, version_major: u32, version_minor: u32) -> Self {
        Self {
            kind_tag,
            version_major,
            version_minor,
        }
    }
}

/// Computes the deterministic fingerprint hash embedded in a storage format manifest.
pub fn storage_format_manifest_hash<I>(
    database_id: u64,
    manifest_version: u64,
    snapshot_id: u64,
    fingerprints: I,
) -> [u8; 32]
where
    I: IntoIterator<Item = ManifestFormatHashInput>,
    I::IntoIter: ExactSizeIterator,
{
    let fingerprints = fingerprints.into_iter();
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
        mix_hash_lane(&mut lanes, 0, fingerprint.kind_tag);
        mix_hash_lane(&mut lanes, 1, u64::from(fingerprint.version_major));
        mix_hash_lane(&mut lanes, 2, u64::from(fingerprint.version_minor));
        mix_hash_lane(
            &mut lanes,
            3,
            fingerprint.kind_tag
                ^ (u64::from(fingerprint.version_major) << 32)
                ^ u64::from(fingerprint.version_minor),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_format_manifest_hash_is_stable_and_order_sensitive() {
        let page = ManifestFormatHashInput::new(1, 1, 0);
        let wal = ManifestFormatHashInput::new(6, 1, 0);

        let first = storage_format_manifest_hash(1, 2, 3, [page, wal]);
        let second = storage_format_manifest_hash(1, 2, 3, [page, wal]);
        let reordered = storage_format_manifest_hash(1, 2, 3, [wal, page]);

        assert_ne!(first, [0; 32]);
        assert_eq!(first, second);
        assert_ne!(first, reordered);
    }
}
